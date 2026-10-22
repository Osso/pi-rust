use pi_agent_core::AgentError;
use pi_agent_core::harness::session::{InMemorySessionStorage, Session};
use pi_agent_core::harness::*;
use pi_ai::models::{CreateModelsOptions, create_models};
use pi_ai::providers::faux::{RegisterFauxProviderOptions, faux_provider};
use pi_ai::types::{AssistantContent, StopReason};
use pi_js_compat::js_json::JsString;
use std::sync::{Arc, Mutex};

fn harness() -> AgentHarness {
    let env = Arc::new(NodeExecutionEnv::new(NodeExecutionEnvOptions {
        cwd: "/tmp".into(),
        ..Default::default()
    }));
    let storage = Arc::new(InMemorySessionStorage::new(Default::default()).unwrap());
    AgentHarness::new(AgentHarnessOptions::new(
        env,
        Session::new(storage),
        Arc::new(create_models(CreateModelsOptions::default())),
        faux_provider(RegisterFauxProviderOptions::default())
            .get_model(None)
            .unwrap(),
    ))
    .unwrap()
}

#[tokio::test]
async fn rejected_before_provider_request_is_persisted_as_exact_failure_message() {
    let harness = harness();
    let error = JsString::from_utf16(vec![0xd800, 0x21]);
    let hook_error = error.clone();
    let _remove_hook = harness.on(
        HarnessHookKind::BeforeProviderRequest,
        Arc::new(move |_| {
            let error = hook_error.clone();
            Box::pin(async move { Err(AgentError::new(error).into()) })
        }),
    );
    let events = Arc::new(Mutex::new(Vec::new()));
    let observed = events.clone();
    let _remove_listener = harness.subscribe(Arc::new(move |event, _| {
        observed.lock().unwrap().push(event.kind());
        Box::pin(async { Ok(()) })
    }));
    let result = harness.prompt("hello".into(), None).await.unwrap();
    assert_eq!(result.stop_reason, StopReason::Error);
    assert_eq!(result.error_message, Some(error));
    assert!(matches!(result.content.as_slice(), [AssistantContent::Text(text)] if text.text.is_empty()));
    let context = harness.session().build_context().await.unwrap();
    assert_eq!(context.messages.len(), 2);
    assert_eq!(context.messages[1].as_assistant(), Some(&result));
    assert_eq!(events.lock().unwrap().last(), Some(&"settled"));
    harness.wait_for_idle().await;
}

#[tokio::test]
async fn unpaired_header_name_rejects_before_provider_dispatch() {
    let harness = harness();
    harness
        .set_stream_options(AgentHarnessStreamOptions {
            headers: Some([(JsString::from_utf16(vec![0xd800]), "value".into())].into()),
            ..Default::default()
        })
        .await;
    let result = harness.prompt("hello".into(), None).await.unwrap();
    assert_eq!(result.stop_reason, StopReason::Error);
    assert!(result.error_message.unwrap().to_utf8().unwrap().contains("surrogate"));
    assert!(matches!(result.content.as_slice(), [AssistantContent::Text(text)] if text.text.is_empty()));
}

#[tokio::test]
async fn harness_preserves_classified_hook_error_before_loop_acquisition() {
    let harness = harness();
    let _remove = harness.on(
        HarnessHookKind::BeforeAgentStart,
        Arc::new(|_| Box::pin(async { Err(AgentHarnessError::new(AgentHarnessErrorCode::Auth, "auth\u{10000}")) })),
    );
    let error = harness.prompt("hello".into(), None).await.unwrap_err();
    assert_eq!(error.code, AgentHarnessErrorCode::Auth);
    assert_eq!(error.name, "AgentHarnessError");
    assert!(harness.session().get_entries().await.unwrap().is_empty());
}

#[tokio::test]
async fn jsonl_harness_persists_failure_across_storage_reopen() {
    use pi_agent_core::harness::session::jsonl::*;
    let temp = tempfile::tempdir().unwrap();
    let path: JsString = temp.path().join("session.jsonl").to_str().unwrap().into();
    let env = Arc::new(NodeExecutionEnv::new(NodeExecutionEnvOptions {
        cwd: temp.path().to_str().unwrap().into(),
        ..Default::default()
    }));
    let storage = JsonlSessionStorage::create(
        env.clone(),
        path.clone(),
        JsonlSessionStorageCreateOptions {
            cwd: env.cwd().clone(),
            session_id: "disk".into(),
            parent_session_path: None,
        },
    )
    .await
    .unwrap();
    let harness = AgentHarness::new(AgentHarnessOptions::new(
        env.clone(),
        Session::new(Arc::new(storage)),
        Arc::new(create_models(Default::default())),
        faux_provider(Default::default()).get_model(None).unwrap(),
    ))
    .unwrap();
    let _remove = harness.on(
        HarnessHookKind::BeforeProviderRequest,
        Arc::new(|_| Box::pin(async { Err(AgentError::new(JsString::from_utf16(vec![0xd800])).into()) })),
    );
    let result = harness.prompt("disk request".into(), None).await.unwrap();
    let reopened = Session::new(Arc::new(JsonlSessionStorage::open(env, path).await.unwrap()));
    let context = reopened.build_context().await.unwrap();
    assert_eq!(context.messages.len(), 2);
    assert_eq!(context.messages[1].as_assistant(), Some(&result));
}

fn faux_harness() -> (AgentHarness, pi_ai::providers::faux::FauxProviderHandle) {
    use pi_ai::models::MutableModels;
    let faux = faux_provider(RegisterFauxProviderOptions {
        api: Some("fixture-api".into()),
        provider: Some("fixture".into()),
        ..Default::default()
    });
    let models = create_models(Default::default());
    models.set_provider(faux.provider.clone());
    let env = Arc::new(NodeExecutionEnv::new(NodeExecutionEnvOptions {
        cwd: "/tmp".into(),
        ..Default::default()
    }));
    let harness = AgentHarness::new(AgentHarnessOptions::new(
        env,
        Session::new(Arc::new(InMemorySessionStorage::new(Default::default()).unwrap())),
        Arc::new(models),
        faux.get_model(None).unwrap(),
    ))
    .unwrap();
    (harness, faux)
}

#[tokio::test]
async fn abort_waits_for_run_clears_only_active_queues_and_preserves_next_turn() {
    use pi_ai::providers::faux::{FauxResponseStep, faux_assistant_message};
    let (harness, faux) = faux_harness();
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let request_signal = Arc::new(Mutex::new(None));
    let next_context = Arc::new(Mutex::new(Vec::new()));
    let start = entered.clone();
    let finish = release.clone();
    let signal = request_signal.clone();
    let capture = next_context.clone();
    faux.set_responses(vec![
        FauxResponseStep::Factory(Arc::new(move |_, options, _, _| {
            let start = start.clone();
            let finish = finish.clone();
            *signal.lock().unwrap() = options.unwrap().stream.request.signal;
            Box::pin(async move {
                start.notify_one();
                finish.notified().await;
                Ok(faux_assistant_message("cancelled reply", Default::default()))
            })
        })),
        FauxResponseStep::Factory(Arc::new(move |context, _, _, _| {
            *capture.lock().unwrap() = context.messages;
            Box::pin(async { Ok(faux_assistant_message("next reply", Default::default())) })
        })),
    ]);
    let run_harness = harness.clone();
    let run = tokio::spawn(async move { run_harness.prompt("hello".into(), None).await });
    entered.notified().await;
    assert_eq!(
        harness.prompt("busy".into(), None).await.unwrap_err().code,
        AgentHarnessErrorCode::Busy
    );
    assert_eq!(
        harness.compact(None).await.unwrap_err().message,
        JsString::from("compact() requires idle harness")
    );
    assert_eq!(
        harness.navigate_tree("busy".into(), None).await.unwrap_err().message,
        JsString::from("navigateTree() requires idle harness")
    );
    harness.steer("steer".into(), None).await.unwrap();
    harness.follow_up("follow".into(), None).await.unwrap();
    harness.next_turn("next".into(), None).await.unwrap();
    let cleared = Arc::new(tokio::sync::Notify::new());
    let observed = cleared.clone();
    let settled_release = Arc::new(tokio::sync::Notify::new());
    let settled_entered = Arc::new(tokio::sync::Notify::new());
    let settled_gate = settled_release.clone();
    let settled_seen = settled_entered.clone();
    let first_settled = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let _remove = harness.subscribe(Arc::new(move |event,_| {
        if matches!(&event,HarnessEvent::QueueUpdate {steer,follow_up,next_turn} if steer.is_empty() && follow_up.is_empty() && next_turn.len() == 1) {observed.notify_one();}
        let gate = settled_gate.clone(); let seen = settled_seen.clone();
        let wait = matches!(event,HarnessEvent::Settled {..}) && first_settled.swap(false,std::sync::atomic::Ordering::SeqCst);
        Box::pin(async move {if wait {seen.notify_one();gate.notified().await;} Ok(())})
    }));
    let abort_harness = harness.clone();
    let abort = tokio::spawn(async move { abort_harness.abort().await });
    cleared.notified().await;
    settled_entered.notified().await;
    assert!(!abort.is_finished());
    assert!(request_signal.lock().unwrap().as_ref().unwrap().is_cancelled());
    let idle_harness = harness.clone();
    let idle = tokio::spawn(async move { idle_harness.wait_for_idle().await });
    assert!(!idle.is_finished());
    release.notify_one();
    settled_release.notify_one();
    let first = run.await.unwrap().unwrap();
    assert_eq!(first.stop_reason, StopReason::Aborted);
    let result = abort.await.unwrap().unwrap();
    assert_eq!(result.cleared_steer.len(), 1);
    assert_eq!(result.cleared_follow_up.len(), 1);
    idle.await.unwrap();
    harness.prompt("again".into(), None).await.unwrap();
    let text: Vec<_> = next_context
        .lock()
        .unwrap()
        .iter()
        .filter_map(|message| match message {
            pi_ai::types::Message::User(user) => Some(serde_json::to_string(user).unwrap()),
            _ => None,
        })
        .collect();
    assert_eq!(text.len(), 3);
    assert!(text[1].contains("next"));
    assert!(text[2].contains("again"));
    assert!(
        !text
            .iter()
            .any(|text| text.contains("steer") || text.contains("follow"))
    );
}

#[tokio::test]
async fn abort_keeps_original_single_error_and_aggregates_multiple_failures() {
    for multiple in [false, true] {
        let harness = harness();
        let _remove = harness.subscribe(Arc::new(move |event, _| {
            Box::pin(async move {
                if matches!(event, HarnessEvent::QueueUpdate { .. })
                    || (multiple && matches!(event, HarnessEvent::Abort(_)))
                {
                    return Err(AgentHarnessError::new(
                        AgentHarnessErrorCode::Auth,
                        JsString::from_utf16(vec![0xd800]),
                    ));
                }
                Ok(())
            })
        }));
        let error = harness.abort().await.unwrap_err();
        if multiple {
            assert_eq!(error.code, AgentHarnessErrorCode::Hook);
            assert_eq!(error.message, JsString::from("Abort completed with errors"));
            let cause = error.cause.unwrap();
            let aggregate = cause.downcast_ref::<HarnessAggregateError>().unwrap();
            assert_eq!(aggregate.errors.len(), 2);
            for cause in &aggregate.errors {
                assert_eq!(
                    cause.downcast_ref::<AgentHarnessError>().unwrap().code,
                    AgentHarnessErrorCode::Auth
                );
            }
        } else {
            assert_eq!(error.code, AgentHarnessErrorCode::Auth);
            assert_eq!(error.message, JsString::from_utf16(vec![0xd800]));
        }
    }
}

#[tokio::test]
async fn request_snapshot_refreshes_after_turn_and_defers_pending_mutations() {
    use pi_ai::providers::faux::{FauxResponseStep, faux_assistant_message};
    let (harness, faux) = faux_harness();
    let records = Arc::new(Mutex::new(Vec::new()));
    faux.set_responses(
        (0..2)
            .map(|index| {
                let records = records.clone();
                FauxResponseStep::Factory(Arc::new(move |context, options, _, _| {
                    let options = options.unwrap();
                    records
                        .lock()
                        .unwrap()
                        .push((context, options.stream.request.timeout_ms, options.reasoning));
                    Box::pin(async move { Ok(faux_assistant_message(format!("reply-{index}"), Default::default())) })
                }))
            })
            .collect(),
    );
    harness
        .set_stream_options(AgentHarnessStreamOptions {
            timeout_ms: Some(1.25),
            ..Default::default()
        })
        .await;
    let once = Arc::new(Mutex::new(false));
    let modify = harness.clone();
    let _remove = harness.on(
        HarnessHookKind::BeforeProviderRequest,
        Arc::new(move |_| {
            let harness = modify.clone();
            let first = !std::mem::replace(&mut *once.lock().unwrap(), true);
            Box::pin(async move {
                if first {
                    harness
                        .set_stream_options(AgentHarnessStreamOptions {
                            timeout_ms: Some(7.5),
                            ..Default::default()
                        })
                        .await;
                    harness.set_thinking_level(ThinkingLevel::High).await?;
                    harness
                        .append_message(pi_agent_core::AgentMessage::from(pi_ai::types::UserMessage {
                            role: pi_ai::types::UserRole::Tag,
                            content: pi_ai::types::UserContent::Text("pending".into()),
                            timestamp: 0.0,
                            input_source: None,
                            provider_native: None,
                        }))
                        .await?;
                    harness.follow_up("follow".into(), None).await?;
                }
                Ok(None)
            })
        }),
    );
    harness.prompt("hello".into(), None).await.unwrap();
    let records = records.lock().unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].1, Some(1.25));
    assert_eq!(records[1].1, Some(7.5));
    assert!(records[0].2.is_none());
    assert_eq!(records[1].2, Some(pi_ai::types::ThinkingLevel::High));
    let first = serde_json::to_string(&records[0].0).unwrap();
    let second = serde_json::to_string(&records[1].0).unwrap();
    assert!(!first.contains("pending"));
    assert!(second.contains("pending"));
    assert!(second.contains("follow"));
}
