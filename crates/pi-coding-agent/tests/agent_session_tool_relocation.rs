use super::*;
use std::sync::Mutex;

#[tokio::test]
async fn tool_result_persists_then_awaits_relocation_before_agent_end_and_activation() {
    use pi_coding_agent::core::extensions::types::{HandlerError, RelocateCallback};
    let mut definition = custom_grep_definition(Arc::new(Mutex::new(Vec::new())));
    definition.approval_required = Some(false);
    definition.execute = Arc::new(|id, _, _, _, ctx| {
        Box::pin(async move {
            ctx.expect("runner context")
                .relocate_after_tool_result(&id, &"/next".into())?;
            Ok(pi_agent_core::AgentToolResult::text("relocating"))
        })
    });
    let fixture = fixture_with_multi_agent(
        Default::default(),
        Vec::new(),
        Some(vec!["grep".to_owned(), "end_turn".to_owned()]),
        None,
        FixtureOverrides {
            custom_tool_definitions: vec![definition],
            ..Default::default()
        },
    )
    .await;
    let observations = Arc::new(Mutex::new(Vec::new()));
    let owner = Arc::new(Mutex::new(Some(fixture.session.clone())));
    let captured = Arc::clone(&observations);
    let target = Arc::clone(&owner);
    let prepare: RelocateCallback = Arc::new(move |cwd| {
        assert_eq!(cwd, "/next");
        let captured = Arc::clone(&captured);
        let target = Arc::clone(&target);
        Box::pin(async move {
            tokio::task::yield_now().await;
            let session = target.lock().unwrap().as_ref().unwrap().clone();
            let stored=session.session_manager().get_entries().iter().any(|entry| entry.as_message().is_some_and(|message| matches!(message.message.as_ref(),AgentMessage::ToolResult(result) if result.tool_call_id=="relocate-call")));
            captured.lock().unwrap().push(format!("prepare:persisted={stored}"));
            Ok(())
        }) as pi_ai::request::BoxFuture<'static, Result<(), HandlerError>>
    });
    let captured = Arc::clone(&observations);
    fixture.session.bind_session_runtime_relocation(
        Arc::new(|_| Box::pin(async { Ok(()) })),
        prepare,
        Arc::new(move || {
            let captured = Arc::clone(&captured);
            Box::pin(async move {
                captured.lock().unwrap().push("activate".to_owned());
                Ok(())
            })
        }),
    );
    let captured = Arc::clone(&observations);
    fixture.session.subscribe(Arc::new(move |event| {
        if let AgentSessionEvent::AgentEnd(event) = event {
            captured.lock().unwrap().push(format!(
                "agent-end:relocation={}",
                event.session_continuation
                    == Some(pi_coding_agent::core::extensions::types::AgentEndSessionContinuation::CwdRelocation)
            ));
        }
    }));
    fixture.faux.set_responses(vec![
        tool_call_response("grep", json!({"pattern":"needle"}), "relocate-call"),
        completed_timeout_response("unused"),
    ]);
    fixture
        .session
        .prompt("relocate after this result", Default::default())
        .await
        .unwrap();
    *owner.lock().unwrap() = None;
    fixture.session.dispose();
    assert_eq!(
        *observations.lock().unwrap(),
        vec!["prepare:persisted=true", "agent-end:relocation=true", "activate"]
    );
    assert_eq!(fixture.faux.state().lock().unwrap().call_count, 1);
}
