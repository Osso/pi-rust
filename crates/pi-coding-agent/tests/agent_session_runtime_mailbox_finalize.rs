use super::*;
#[path = "support/oracle.rs"]
mod oracle;

#[tokio::test(start_paused = true)]
async fn reserved_finalize_propagates_outbox_claim_failure_after_durable_finalization() {
    let source_dir = tempfile::tempdir().unwrap();
    let mut source = oracle::TsOracle::spawn("storage-prompt-order-oracle.mts", "packages/coding-agent/src/core");
    let expected = source.request(&serde_json::json!({"id":1,"dir":source_dir.path(),"operation":"finalizeFailure"}));
    assert_eq!(expected["ok"], true, "{expected}");
    assert_eq!(expected["output"]["threw"], true, "{expected}");

    let env = test_env(true).await;
    env.session
        .bind_extensions(ExtensionBindings {
            control_db_path: Some(env.control_db.clone().into()),
            ..Default::default()
        })
        .await
        .unwrap();
    let store = Arc::clone(env.store.as_ref().unwrap());
    let agent = reserve_agent(&store, &env, "agent_1", None);
    let recipient = RuntimeMailboxAddress {
        agent_id: None,
        session_id: session_id(&env),
    };
    pi_coding_agent::core::session_control_db::register_runtime_mailbox_listener(
        &env.control_db,
        &recipient,
        std::process::id(),
        Some(&session_path(&env)),
        Default::default(),
    )
    .unwrap();
    let steered = request_agent_steering(
        &store,
        &AgentSteeringRequest {
            agent_id: agent.id.clone(),
            message: "Finish".into(),
            file_refs: None,
            target_checkpoint: None,
        },
        &AgentSteeringRuntimeBinding {
            actor_agent_id: None,
            control_db_path: env.control_db.clone().into(),
            session_id: session_id(&env),
        },
        None,
    )
    .unwrap();
    let AgentSteeringRequestResult::Ok { message, .. } = steered else {
        panic!("steering rejected")
    };
    enqueue_steering_transport(&env, &message.id, "agent_1");
    env.faux.set_responses(vec![
        faux_assistant_message("initial reply", FauxAssistantMessageOptions::default()).into(),
        faux_assistant_message("completed", FauxAssistantMessageOptions::default()).into(),
    ]);
    let db = rusqlite::Connection::open(&env.control_db).unwrap();
    db.execute_batch("CREATE TRIGGER fail_outbox_claim BEFORE UPDATE ON multi_agent_terminal_outbox WHEN NEW.status='claimed' BEGIN SELECT RAISE(ABORT,'fixture claim failure'); END").unwrap();
    let error = env.session.prompt("hello", PromptOptions::default()).await.unwrap_err();
    assert_eq!(error.to_string(), expected["output"]["message"].as_str().unwrap());
    let status: String = db
        .query_row(
            "SELECT status FROM multi_agent_terminal_outbox WHERE agent_id='agent_1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(serde_json::json!(status), expected["output"]["status"]);
    let persisted =
        pi_coding_agent::core::session_control_db::read_multi_agent_state(&env.control_db, session_path(&env))
            .unwrap()
            .unwrap();
    assert_eq!(persisted.agents[0]["lifecycle"], json!("completed"));
}
