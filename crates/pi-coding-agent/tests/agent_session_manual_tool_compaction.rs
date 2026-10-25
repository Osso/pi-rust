use super::*;
use pi_coding_agent::core::extensions::types::{CompactionEvent, CompactionEventResult, ExtensionContext};
use pi_coding_agent::core::extensions::{ExtensionFactory, HandlerError};
use std::sync::Mutex;

#[tokio::test]
async fn manual_compaction_during_tool_execution_resumes_and_reports_retry_like_frozen() {
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let signal = Arc::new(Mutex::new(None));
    let saved_retry = Arc::new(Mutex::new(Vec::new()));
    let saved_capture = Arc::clone(&saved_retry);
    let (tool_started, tool_release, tool_signal) = (Arc::clone(&started), Arc::clone(&release), Arc::clone(&signal));
    let factory: ExtensionFactory = Arc::new(move |api| {
        let saved_capture = Arc::clone(&saved_capture);
        api.on_session_compact(
            move |event: pi_coding_agent::core::extensions::types::SessionCompactEvent, _ctx: ExtensionContext| {
                saved_capture.lock().unwrap().push(event.will_retry);
                Box::pin(async { Ok(None) }) as pi_ai::request::BoxFuture<'static, Result<Option<()>, HandlerError>>
            },
        )
        .unwrap();
        let (started, release, signal) = (
            Arc::clone(&tool_started),
            Arc::clone(&tool_release),
            Arc::clone(&tool_signal),
        );
        api.register_tool(pi_coding_agent::core::extensions::types::ToolDefinition {
            name: "hold".into(),
            label: "Hold".into(),
            description: "Hold until explicitly released".into(),
            prompt_snippet: None,
            prompt_guidelines: None,
            approval_required: None,
            approval_kind: None,
            parameters: json!({"type":"object","properties":{}}),
            render_shell: None,
            prepare_arguments: None,
            prepare_execution: None,
            execution_mode: None,
            execute: Arc::new(move |_, _, abort, _, _| {
                let (started, release, signal) = (Arc::clone(&started), Arc::clone(&release), Arc::clone(&signal));
                Box::pin(async move {
                    *signal.lock().unwrap() = abort;
                    started.notify_one();
                    release.notified().await;
                    Ok(pi_agent_core::AgentToolResult {
                        content: Vec::new(),
                        details: None,
                        is_error: None,
                        terminate: None,
                    })
                })
            }),
            render_call: None,
            render_result: None,
        })
        .unwrap();
        api.on_compaction(|event:CompactionEvent,_ctx:ExtensionContext| Box::pin(async move {
            Ok(Some(CompactionEventResult {cancel:None,compaction:Some(serde_json::from_value(json!({
                "summary":"saved summary","firstKeptEntryId":event.preparation.first_kept_entry_id,"tokensBefore":event.preparation.tokens_before,
            })).unwrap())}))
        }) as pi_ai::request::BoxFuture<'static,Result<Option<CompactionEventResult>,HandlerError>>).unwrap();
        Box::pin(async { Ok(()) })
    });
    let fixture = fixture_with_timeout_extension(factory).await;
    fixture.session.set_active_tools_by_name(&["hold".into()]).unwrap();
    {
        let mut manager = fixture.session.session_manager();
        for i in 0..4 {
            manager
                .append_message(
                    serde_json::from_value(json!({"role":"user","content":format!("history {i}"),"timestamp":i*2}))
                        .unwrap(),
                )
                .unwrap();
            manager
                .append_message(AgentMessage::Assistant(Box::new(faux_assistant_message(
                    format!("history answer {i}"),
                    Default::default(),
                ))))
                .unwrap();
        }
        let (context, ids) = manager.build_session_context_with_identities();
        fixture
            .session
            .agent()
            .set_messages_with_identities(context.messages, ids)
            .unwrap();
    }
    let end_flags = Arc::new(Mutex::new(Vec::new()));
    let capture = Arc::clone(&end_flags);
    fixture.session.subscribe(Arc::new(move |event| {
        if let AgentSessionEvent::CompactionEnd(end) = event {
            capture.lock().unwrap().push(end.will_retry);
        }
    }));
    fixture.faux.set_responses(vec![
        faux_assistant_message(
            vec![pi_ai::providers::faux::faux_tool_call(
                "hold",
                pi_js_compat::js_json::JsObject::new(),
                Some("hold-call".into()),
            )],
            FauxAssistantMessageOptions {
                stop_reason: Some(pi_ai::types::StopReason::ToolUse),
                ..Default::default()
            },
        )
        .into(),
        faux_assistant_message("resumed", Default::default()).into(),
    ]);
    let session = fixture.session.clone();
    let prompt = tokio::spawn(async move { session.prompt("start tool", Default::default()).await });
    tokio::time::timeout(std::time::Duration::from_secs(5), started.notified())
        .await
        .expect("tool started");
    assert!(!fixture.session.agent().pending_tool_calls().is_empty());
    let session = fixture.session.clone();
    let compact = tokio::spawn(async move { session.compact(None).await });
    let abort = signal.lock().unwrap().clone().expect("tool signal");
    tokio::time::timeout(std::time::Duration::from_secs(5), abort.wait())
        .await
        .expect("compaction aborted the running tool");
    release.notify_one();
    tokio::time::timeout(std::time::Duration::from_secs(5), compact)
        .await
        .expect("compact settled")
        .unwrap()
        .unwrap();
    prompt.await.unwrap().unwrap();
    let final_text = fixture
        .session
        .messages()
        .iter()
        .rev()
        .find_map(|message| match message {
            AgentMessage::Assistant(message) => Some(
                message
                    .content
                    .iter()
                    .filter_map(|block| match block {
                        pi_ai::types::AssistantContent::Text(text) => Some(text.text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .map(|parts| JsString::join(parts, ""))
        .unwrap_or_default();
    let actual = json!({"savedRetry":saved_retry.lock().unwrap().clone(),"retry":end_flags.lock().unwrap().clone(),"calls":fixture.faux.state().lock().unwrap().call_count,"finalText":final_text});
    fixture.session.dispose();
    let mut oracle = tools_oracle::TsOracle::spawn("agent-session-oracle.ts", "packages/coding-agent/src/core/sdk.ts");
    let expected=oracle.request(&json!({"operation":"manual-tool-compaction","dir":fixture._temp_dir.path().join("oracle").display().to_string()}));
    assert_eq!(
        expected,
        json!({"ok":true,"value":{"savedRetry":[true],"retry":[true],"calls":2,"finalText":"resumed"}})
    );
    assert_eq!(actual, expected["value"]);
}
