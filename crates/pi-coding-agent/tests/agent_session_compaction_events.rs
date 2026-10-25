use super::*;
use pi_agent_core::agent::types::{AgentContext, PrepareNextTurnContext};
use std::time::Duration;

fn normalize_saved_event(event: &pi_coding_agent::core::extensions::types::SessionCompactEvent) -> serde_json::Value {
    let mut wire =
        serde_json::to_value(pi_coding_agent::core::extensions::types::ExtensionEvent::SessionCompact(event.clone()))
            .unwrap();
    let entry = wire.get_mut("compactionEntry").unwrap();
    for field in ["id", "parentId", "timestamp", "firstKeptEntryId"] {
        if let Some(value) = entry.get_mut(field).filter(|value| value.is_string()) {
            *value = json!(format!("<{field}>"));
        }
    }
    if let Some(value) = entry.get_mut("durationMs").filter(|value| value.is_number()) {
        *value = json!("<durationMs>");
    }
    wire
}

async fn assert_saved_event_boundary(operation: &str) {
    let auto = operation.starts_with("auto-");
    let cancel = operation.contains("cancel");
    let env = env_with(
        json!({"enabled":false,"keepRecentTokens":1,"reserveTokens":0,"thresholdPercent":50}),
        None,
        Some(Arc::new(move |event| {
            Some(CompactionEventResult {
                cancel: cancel.then_some(true),
                compaction: (!cancel).then(|| extension_result("saved summary", &event.preparation)),
            })
        })),
    )
    .await;
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    {
        let mut captured = env.captured.lock().unwrap();
        captured.saved_started = Some(Arc::clone(&started));
        captured.saved_release = Some(Arc::clone(&release));
    }
    let target = Arc::new(Mutex::new(Some(env.session.clone())));
    let callback_target = Arc::clone(&target);
    let end_states = Arc::new(Mutex::new(Vec::new()));
    let callback_states = Arc::clone(&end_states);
    env.session.subscribe(Arc::new(move |event| {
        if matches!(event, AgentSessionEvent::CompactionEnd(_)) {
            callback_states
                .lock()
                .unwrap()
                .push(callback_target.lock().unwrap().as_ref().unwrap().is_compacting());
        }
    }));
    let anchor = if auto {
        let mut model = env.session.model();
        model.context_window = 1000.0;
        model.auto_compaction_threshold = Some(900.0);
        env.session.agent().set_model(model);
        let anchor = append_usage_anchor(&env.session, 800.0);
        env.session
            .send_custom_message(
                &"tail".into(),
                pi_agent_core::harness::messages::CustomMessageContent::Text("x".repeat(800).into()),
                false,
                None,
                false,
                None,
            )
            .await
            .unwrap();
        Some(anchor)
    } else {
        None
    };
    let session = env.session.clone();
    let mut task = tokio::spawn(async move {
        if let Some(anchor) = anchor {
            let state = session.state();
            session.agent().prepare_next_turn_with_context_hook().unwrap()(
                PrepareNextTurnContext {
                    message: anchor,
                    tool_results: Vec::new(),
                    new_messages: Vec::new(),
                    context: AgentContext {
                        system_prompt: state.system_prompt,
                        messages: state.messages,
                        tools: Some(state.tools),
                    },
                },
                None,
            )
            .await
            .expect("next turn");
            Ok(())
        } else {
            session.compact(None).await.map(|_| ())
        }
    });
    let mut completed = None;
    let handler_started = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::select! {
            _ = started.notified() => true,
            result = &mut task => { completed = Some(result.expect("operation task")); false }
        }
    })
    .await
    .expect("operation finishes or reaches saved handler");
    let mut settled_before_release = !handler_started;
    if handler_started && let Ok(result) = tokio::time::timeout(Duration::from_millis(100), &mut task).await {
        completed = Some(result.expect("operation task"));
        settled_before_release = true;
    }
    release.notify_one();
    let result = match completed {
        Some(result) => result,
        None => task.await.expect("operation task"),
    };
    *target.lock().unwrap() = None;
    let error = result.err().map(|error| error.message().clone());
    let snapshots = {
        let captured = env.captured.lock().unwrap();
        let entries = env.session.session_manager().get_entries();
        captured.saved.iter().map(|event| {
            let envelope = normalize_saved_event(event);
            let serialized = &envelope["compactionEntry"];
            let hook = &captured.compaction[0].preparation;
            json!({"entryType":serialized["type"],"summary":event.compaction_entry.summary,
                "fromExtension":event.from_extension,"fromHook":event.compaction_entry.from_hook,
                "reason":event.reason,"willRetry":event.will_retry,
                "entryMatchesSaved":entries.iter().any(|entry| entry.id() == Some(&event.compaction_entry.id)),
                "matchesHook":event.compaction_entry.first_kept_entry_id == hook.first_kept_entry_id && event.compaction_entry.tokens_before == hook.tokens_before,
                "durationPresent":event.compaction_entry.duration_ms.is_some(),"details":event.compaction_entry.details})
        }).collect::<Vec<_>>()
    };
    let wires = env
        .captured
        .lock()
        .unwrap()
        .saved
        .iter()
        .map(normalize_saved_event)
        .collect::<Vec<_>>();
    let actual = json!({"wires":wires,"handlerStarted":handler_started,"settledBeforeRelease":settled_before_release,
        "order":env.captured.lock().unwrap().saved_order,"endCompacting":end_states.lock().unwrap().clone(),
        "afterCompacting":env.session.is_compacting(),"error":error,"snapshots":snapshots});
    env.session.dispose();
    let mut oracle =
        callbacks_oracle::TsOracle::spawn("agent-session-oracle.ts", "packages/coding-agent/src/core/sdk.ts");
    let expected =
        oracle.request(&json!({"operation":operation,"dir":env._dir.path().join("oracle").display().to_string()}));
    assert_eq!(expected["ok"], true, "{expected}");
    let mut expected = expected;
    let raw_wires = expected["value"].as_object_mut().unwrap().remove("rawWires").unwrap();
    for wire in raw_wires.as_array().unwrap() {
        let decoded: pi_coding_agent::core::extensions::types::ExtensionEvent =
            serde_json::from_value(wire.clone()).unwrap();
        let encoded = serde_json::to_value(decoded).unwrap();
        assert_eq!(JsValue::from(encoded), JsValue::from(wire.clone()));
    }
    let mut expected_projection = expected.clone();
    expected_projection["value"]
        .as_object_mut()
        .unwrap()
        .remove("wires")
        .unwrap();
    let snapshots = if cancel {
        Vec::new()
    } else {
        vec![
            json!({"entryType":"compaction","summary":"saved summary","fromExtension":true,"fromHook":true,
        "reason":if auto { "threshold" } else { "manual" },"willRetry":false,"entryMatchesSaved":true,"matchesHook":true,"durationPresent":true,"details":{"origin":"extension"}}),
        ]
    };
    let order = if cancel {
        vec!["compaction_end"]
    } else {
        vec!["session_compact:start", "session_compact:end", "compaction_end"]
    };
    assert_eq!(
        expected_projection,
        json!({"ok":true,"value":{"handlerStarted":!cancel,"settledBeforeRelease":cancel,"order":order,
        "endCompacting":[auto || cancel],"afterCompacting":false,"error":if cancel && !auto { Some("Compaction cancelled") } else { None },"snapshots":snapshots}})
    );
    assert_eq!(JsValue::from(actual), JsValue::from(expected["value"].clone()));
}

#[tokio::test]
async fn manual_saved_event_is_awaited_before_completion_like_frozen() {
    assert_saved_event_boundary("manual-compact-events").await;
}
#[tokio::test]
async fn auto_saved_event_is_awaited_and_controller_lives_through_completion_like_frozen() {
    assert_saved_event_boundary("auto-compact-events").await;
}
#[tokio::test]
async fn manual_cancel_keeps_controller_during_completion_like_frozen() {
    assert_saved_event_boundary("manual-cancel-events").await;
}
#[tokio::test]
async fn auto_cancel_keeps_controller_during_completion_like_frozen() {
    assert_saved_event_boundary("auto-cancel-events").await;
}
