//! Real persisted steering lifecycle while an additional prompt only queues.
use super::*;

use super::finalize::oracle as frozen;

#[tokio::test]
async fn queued_prompt_does_not_complete_an_active_steered_turn() {
    // Other fixtures refresh and replace the process-global faux stream registration.
    const CHILD: &str = "PI_MAILBOX_PROMPT_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "prompt_boundary::queued_prompt_does_not_complete_an_active_steered_turn",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "mailbox prompt child failed:\n{}\n{}",
            String::from_utf8(output.stdout).unwrap(),
            String::from_utf8(output.stderr).unwrap()
        );
        return;
    }
    use pi_ai::providers::faux::FauxResponseStep;
    use pi_coding_agent::core::agent_session::StreamingBehavior;
    let env = test_env(true).await;
    env.session
        .bind_extensions(ExtensionBindings {
            control_db_path: Some(env.control_db.clone().into()),
            ..Default::default()
        })
        .await
        .expect("bind mailbox");
    let store = Arc::clone(env.store.as_ref().expect("store"));
    let running = reserve_agent(&store, &env, "agent_1", None);
    let sender = RuntimeMailboxAddress {
        agent_id: None,
        session_id: session_id(&env),
    };
    pi_coding_agent::core::session_control_db::register_runtime_mailbox_listener(
        &env.control_db,
        &sender,
        std::process::id(),
        Some(&JsString::from(&session_path(&env))),
        Default::default(),
    )
    .expect("sender listener");
    let steered = request_agent_steering(
        &store,
        &AgentSteeringRequest {
            agent_id: running.id.clone(),
            message: "Continue verification".into(),
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
    .expect("steer");
    let AgentSteeringRequestResult::Ok { message, .. } = steered else {
        panic!("steering rejected: {steered:?}");
    };
    enqueue_steering_transport(&env, &message.id, "agent_1");
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let (started_provider, release_provider) = (Arc::clone(&started), Arc::clone(&release));
    env.faux.set_responses(vec![
        faux_assistant_message("initial reply", Default::default()).into(),
        FauxResponseStep::Factory(Arc::new(move |_, _, _, _| {
            let (started, release) = (Arc::clone(&started_provider), Arc::clone(&release_provider));
            Box::pin(async move {
                started.notify_one();
                release.notified().await;
                Ok(faux_assistant_message("steer done", Default::default()))
            })
        })),
        faux_assistant_message("follow-up done", Default::default()).into(),
    ]);
    let session = env.session.clone();
    let turn = tokio::spawn(async move { session.prompt("hello", Default::default()).await });
    tokio::time::timeout(Duration::from_secs(5), started.notified())
        .await
        .expect("steered provider started");
    env.session
        .prompt(
            "additional input",
            PromptOptions {
                streaming_behavior: Some(StreamingBehavior::FollowUp),
                ..Default::default()
            },
        )
        .await
        .expect("queue follow-up");
    let while_running = store.get_agent(&running.id).expect("running snapshot");
    release.notify_one();
    tokio::time::timeout(Duration::from_secs(5), turn)
        .await
        .expect("turn settled")
        .expect("join")
        .expect("prompt");
    let completed = store.get_agent(&running.id).expect("completed snapshot");
    env.session.dispose();
    assert_eq!(while_running.lifecycle, AgentLifecycle::Running);
    assert_eq!(completed.lifecycle, AgentLifecycle::Completed);
    assert_eq!(
        completed.result.as_ref().and_then(|result| result.summary.as_ref()),
        Some(&JsString::from("follow-up done"))
    );
    let mut oracle = frozen::TsOracle::spawn("agent-session-oracle.ts", "packages/coding-agent/src/core/sdk.ts");
    let expected = oracle.request(&serde_json::json!({
        "operation": "prompt-steering-lifecycle", "dir": env._dir.path().join("oracle").display().to_string()
    }));
    assert_eq!(
        expected,
        serde_json::json!({"ok": true, "value": {
            "whileRunning": "running", "finalLifecycle": "completed", "summary": "follow-up done"
        }})
    );
    let actual = serde_json::json!({
        "whileRunning": while_running.lifecycle,
        "finalLifecycle": completed.lifecycle,
        "summary": completed.result.as_ref().and_then(|result| result.summary.as_ref()),
    });
    assert_eq!(actual, expected["value"]);
}
