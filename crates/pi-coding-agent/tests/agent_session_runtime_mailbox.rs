//! Behavioral tests for the agent-session runtime mailbox timers and steering
//! completion (`core/agent-session.ts`).

use std::sync::Arc;
use std::time::Duration;

use pi_agent_core::agent::types::AgentMessage;
use pi_ai::providers::faux::{
    FauxAssistantMessageOptions, FauxModelDefinition, FauxProviderHandle, RegisterFauxProviderOptions,
    faux_assistant_message, faux_provider,
};
use pi_coding_agent::bundled_extensions::agents_core::control_runtime::{
    AgentSteeringRequest, AgentSteeringRequestResult, AgentSteeringRuntimeBinding, request_agent_steering,
};
use pi_coding_agent::bundled_extensions::agents_core::lifecycle_runtime::create_lifecycle_coordinator;
use pi_coding_agent::core::agent_session::{AgentSession, ExtensionBindings, PromptOptions};
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::lifecycle_coordinator::PrepareChildCommandInput;
use pi_coding_agent::core::model_runtime::{CreateModelRuntimeOptions, ModelRuntime};
use pi_coding_agent::core::multi_agent_store::{
    AgentLifecycle, AgentPermission, AgentSnapshot, MultiAgentStore, MultiAgentStoreOptions, SessionPersistence,
    SpawnAgent,
};
use pi_coding_agent::core::resource_loader::StaticResourceLoader;
use pi_coding_agent::core::runtime_coordination_format::format_runtime_mailbox_prompt;
use pi_coding_agent::core::sdk::{CreateAgentSessionOptions, create_agent_session};
use pi_coding_agent::core::session_control_db::{
    CreateMultiAgentChildWithRuntimeOwnershipResult, EnqueueRuntimeMailboxMessageInput, RuntimeMailboxAddress,
    RuntimeMailboxMessageKind, RuntimeMailboxMessageStatus, RuntimeMailboxStoreRef, enqueue_runtime_mailbox_message,
    get_control_db_path, read_runtime_mailbox_message, upsert_multi_agent_mailbox_message,
};
use pi_coding_agent::core::session_manager::{SessionManager, SharedSessionManager, lock_session_manager};
use serde_json::json;

struct TestPersistence {
    control_db_path: String,
    session_path: String,
}

impl SessionPersistence for TestPersistence {
    fn metadata_control_db_path(&self) -> Option<String> {
        Some(self.control_db_path.clone())
    }

    fn session_file(&self) -> Option<String> {
        Some(self.session_path.clone())
    }
}

struct TestEnv {
    _dir: tempfile::TempDir,
    cwd: String,
    _agent_dir: String,
    control_db: String,
    initial_control_db: String,
    session: AgentSession,
    session_manager: SharedSessionManager,
    faux: FauxProviderHandle,
    store: Option<Arc<MultiAgentStore>>,
}

fn faux() -> FauxProviderHandle {
    faux_provider(RegisterFauxProviderOptions {
        provider: Some("faux".to_owned()),
        models: Some(vec![FauxModelDefinition {
            id: "faux-1".to_owned(),
            name: Some("Faux Model".to_owned()),
            reasoning: Some(true),
            input: None,
            cost: None,
            context_window: Some(100_000.0),
            max_tokens: Some(4096.0),
        }]),
        ..Default::default()
    })
}

async fn faux_runtime(faux: &FauxProviderHandle) -> ModelRuntime {
    let mut data = serde_json::Map::new();
    data.insert("faux".to_owned(), json!({"type": "api_key", "key": "faux-key"}));
    let runtime = ModelRuntime::create(CreateModelRuntimeOptions {
        credentials: Some(Arc::new(AuthStorage::in_memory(data))),
        models_path: Some(None),
        refresh_on_create: Some(false),
        ..Default::default()
    })
    .await
    .expect("runtime");
    runtime.register_native_provider(Arc::clone(&faux.provider));
    runtime.refresh(Default::default()).await.expect("refresh");
    runtime
}

async fn test_env(with_store: bool) -> TestEnv {
    let dir = tempfile::tempdir().expect("temp dir");
    let cwd = dir.path().join("repo").display().to_string();
    std::fs::create_dir_all(&cwd).expect("cwd");
    let agent_dir = dir.path().join("agent").display().to_string();
    std::fs::create_dir_all(&agent_dir).expect("agent dir");
    let session_dir = dir.path().join("sessions").display().to_string();
    let control_db = get_control_db_path(dir.path());
    let initial_control_db = get_control_db_path(&dir.path().join("initial"));
    let faux = faux();
    let runtime = faux_runtime(&faux).await;
    let model = faux.get_model(None).expect("model");
    let mut manager = SessionManager::create(&cwd, Some(&session_dir), None).expect("session manager");
    manager
        .set_metadata_control_db_path(Some(&initial_control_db), Default::default())
        .expect("metadata control db");
    let store = with_store.then(|| Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default())));
    let session_id = manager.get_session_id().to_owned();
    let result = create_agent_session(CreateAgentSessionOptions {
        cwd: Some(cwd.clone()),
        agent_dir: Some(agent_dir.clone()),
        model_runtime: Some(runtime),
        model: Some(model),
        resource_loader: Some(StaticResourceLoader::new()),
        session_manager: Some(manager),
        control_db_path: Some(initial_control_db.clone()),
        multi_agent_store: store.clone(),
        multi_agent_agent_id: with_store.then(|| "agent_1".to_owned()),
        multi_agent_parent_session_id: with_store.then_some(session_id),
        ..Default::default()
    })
    .await
    .expect("session");
    result.session.set_active_tools_by_name(&[]);
    let session_manager = result.session.session_manager_handle();
    let session_path = lock_session_manager(&session_manager)
        .get_session_file()
        .expect("session file")
        .to_owned();
    if let Some(store) = &store {
        store.set_persistence_session_manager(Some(&TestPersistence {
            control_db_path: control_db.clone(),
            session_path,
        }));
    }
    lock_session_manager(&session_manager)
        .set_metadata_control_db_path(Some(&control_db), Default::default())
        .expect("target control db");
    TestEnv {
        _dir: dir,
        cwd,
        _agent_dir: agent_dir,
        control_db,
        initial_control_db,
        session: result.session,
        session_manager,
        faux,
        store,
    }
}

fn session_id(env: &TestEnv) -> String {
    lock_session_manager(&env.session_manager).get_session_id().to_owned()
}

fn session_path(env: &TestEnv) -> String {
    lock_session_manager(&env.session_manager)
        .get_session_file()
        .expect("session file")
        .to_owned()
}

fn user_texts(session: &AgentSession) -> Vec<String> {
    session
        .messages()
        .iter()
        .filter_map(|message| match message {
            AgentMessage::User(user) => match &user.content {
                pi_ai::types::UserContent::Text(text) => Some(text.clone()),
                pi_ai::types::UserContent::Blocks(blocks) => Some(
                    blocks
                        .iter()
                        .filter_map(|block| match block {
                            pi_ai::types::TextOrImageContent::Text(text) => Some(text.text.clone()),
                            pi_ai::types::TextOrImageContent::Image(_) => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                ),
            },
            _ => None,
        })
        .collect()
}

fn enqueue_stored_message(env: &TestEnv, body: &str, message_id: &str, recipient_agent: Option<&str>) -> i64 {
    let session_path = session_path(env);
    let to_agent = recipient_agent.unwrap_or("main");
    upsert_multi_agent_mailbox_message(
        &env.control_db,
        &session_path,
        message_id,
        &json!({
            "body": body,
            "fromAgentId": "agent_1",
            "id": message_id,
            "kind": "message",
            "status": "pending",
            "toAgentId": to_agent,
        }),
    )
    .expect("upsert mailbox message");
    enqueue_runtime_mailbox_message(
        &env.control_db,
        &EnqueueRuntimeMailboxMessageInput {
            kind: RuntimeMailboxMessageKind::Message,
            recipient: RuntimeMailboxAddress {
                agent_id: recipient_agent.map(str::to_owned),
                session_id: session_id(env),
            },
            sender: RuntimeMailboxAddress {
                agent_id: Some("agent_1".to_owned()),
                session_id: "child-session".to_owned(),
            },
            store_ref: RuntimeMailboxStoreRef {
                message_id: message_id.to_owned(),
                session_path,
            },
        },
    )
    .expect("enqueue runtime message")
}

fn enqueue_steering_transport(env: &TestEnv, message_id: &str, agent_id: &str) -> i64 {
    enqueue_runtime_mailbox_message(
        &env.control_db,
        &EnqueueRuntimeMailboxMessageInput {
            kind: RuntimeMailboxMessageKind::Steer,
            recipient: RuntimeMailboxAddress {
                agent_id: Some(agent_id.to_owned()),
                session_id: session_id(env),
            },
            sender: RuntimeMailboxAddress {
                agent_id: None,
                session_id: session_id(env),
            },
            store_ref: RuntimeMailboxStoreRef {
                message_id: message_id.to_owned(),
                session_path: session_path(env),
            },
        },
    )
    .expect("enqueue runtime steering")
}

async fn advance(millis: u64) {
    // Let spawned tasks register their timers at the current paused instant.
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }
    tokio::time::advance(Duration::from_millis(millis)).await;
    for _ in 0..200 {
        tokio::task::yield_now().await;
    }
}

fn reserve_agent(store: &Arc<MultiAgentStore>, env: &TestEnv, id: &str, parent_id: Option<&str>) -> AgentSnapshot {
    let coordinator = create_lifecycle_coordinator(store).expect("coordinator");
    let prepared = coordinator.prepare_child(&PrepareChildCommandInput {
        spawn: SpawnAgent {
            parent_id: Some(parent_id.unwrap_or("main").to_owned()),
            display_name: format!("Agent {id}"),
            agent_type: "worker".to_owned(),
            origin: None,
            cwd: env.cwd.clone(),
            permission: AgentPermission {
                policy: "on-request".to_owned(),
                inherited_from: None,
                narrowed: true,
            },
            worktree: None,
            model: None,
            account: None,
            slot: None,
            transcript: Some(pi_coding_agent::core::multi_agent_store::AgentTranscriptMetadata {
                session_id: session_id(env),
                path: Some(session_path(env)),
            }),
            event_stream: None,
            worker: None,
        },
        agent_id: Some(id.to_owned()),
        detached: None,
        result: None,
    });
    let created = coordinator
        .commit_running_child(
            &prepared,
            &session_id(env),
            pi_coding_agent::bundled_extensions::agents_core::lifecycle_runtime::runtime_process_identity(),
            None,
        )
        .expect("commit child");
    let agent = match created {
        CreateMultiAgentChildWithRuntimeOwnershipResult::Ok { agent, .. } => *agent,
        other => panic!("unexpected child creation: {other:?}"),
    };
    store.publish_lifecycle_coordinator_snapshot(&agent);
    agent
}

#[tokio::test(start_paused = true)]
async fn polling_claims_mailbox_messages_only_after_listener_binding() {
    let env = test_env(false).await;
    let message_id = enqueue_stored_message(&env, "Constructor poll wake", "runtime_test_poll", None);

    // The constructor poll timer watches the initial control DB; the target row
    // lives in a different database with no listener yet.
    advance(30_000).await;
    assert!(user_texts(&env.session).is_empty());
    let message = read_runtime_mailbox_message(&env.control_db, message_id)
        .expect("read message")
        .expect("message");
    assert_eq!(message.status, RuntimeMailboxMessageStatus::Pending);

    env.session
        .bind_extensions(ExtensionBindings {
            control_db_path: Some(env.control_db.clone()),
            ..Default::default()
        })
        .await;
    env.faux.set_responses(vec![
        faux_assistant_message("mailbox reply", FauxAssistantMessageOptions::default()).into(),
    ]);
    advance(30_000).await;
    assert_eq!(
        user_texts(&env.session),
        vec![format_runtime_mailbox_prompt(
            &read_runtime_mailbox_message(&env.control_db, message_id)
                .expect("read message")
                .expect("message"),
            &session_id(&env)
        )]
    );
    let message = read_runtime_mailbox_message(&env.control_db, message_id)
        .expect("read message")
        .expect("message");
    assert_eq!(message.status, RuntimeMailboxMessageStatus::Delivered);
    assert_ne!(env.control_db, env.initial_control_db);
}

#[tokio::test(start_paused = true)]
async fn polling_does_not_claim_messages_without_a_matching_listener() {
    let env = test_env(false).await;
    env.session
        .bind_extensions(ExtensionBindings {
            control_db_path: Some(env.control_db.clone()),
            ..Default::default()
        })
        .await;
    let other_session = "another-session";
    let message_id = enqueue_stored_message_for(&env, "Other recipient", "runtime_test_other", None, other_session);
    advance(30_000).await;
    assert!(user_texts(&env.session).is_empty());
    let message = read_runtime_mailbox_message(&env.control_db, message_id)
        .expect("read message")
        .expect("message");
    assert_eq!(message.status, RuntimeMailboxMessageStatus::Pending);
}

fn enqueue_stored_message_for(
    env: &TestEnv,
    body: &str,
    message_id: &str,
    recipient_agent: Option<&str>,
    recipient_session: &str,
) -> i64 {
    let session_path = session_path(env);
    upsert_multi_agent_mailbox_message(
        &env.control_db,
        &session_path,
        message_id,
        &json!({
            "body": body,
            "fromAgentId": "agent_1",
            "id": message_id,
            "kind": "message",
            "status": "pending",
            "toAgentId": recipient_agent.unwrap_or("main"),
        }),
    )
    .expect("upsert mailbox message");
    enqueue_runtime_mailbox_message(
        &env.control_db,
        &EnqueueRuntimeMailboxMessageInput {
            kind: RuntimeMailboxMessageKind::Message,
            recipient: RuntimeMailboxAddress {
                agent_id: recipient_agent.map(str::to_owned),
                session_id: recipient_session.to_owned(),
            },
            sender: RuntimeMailboxAddress {
                agent_id: Some("agent_1".to_owned()),
                session_id: "child-session".to_owned(),
            },
            store_ref: RuntimeMailboxStoreRef {
                message_id: message_id.to_owned(),
                session_path,
            },
        },
    )
    .expect("enqueue runtime message")
}

#[tokio::test(start_paused = true)]
async fn steering_delivery_marks_the_store_message_and_completes_the_agent() {
    let env = test_env(true).await;
    env.session
        .bind_extensions(ExtensionBindings {
            control_db_path: Some(env.control_db.clone()),
            ..Default::default()
        })
        .await;
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
        Some(&session_path(&env)),
        Default::default(),
    )
    .expect("register sender listener");
    let steered = request_agent_steering(
        &store,
        &AgentSteeringRequest {
            agent_id: running.id.clone(),
            message: "Continue verification".to_owned(),
            file_refs: None,
            target_checkpoint: None,
        },
        &AgentSteeringRuntimeBinding {
            actor_agent_id: None,
            control_db_path: env.control_db.clone(),
            session_id: session_id(&env),
        },
        None,
    )
    .expect("steering request");
    let AgentSteeringRequestResult::Ok { message, .. } = steered else {
        panic!("steering rejected: {steered:?}");
    };
    let transport_id = enqueue_steering_transport(&env, &message.id, "agent_1");
    env.faux.set_responses(vec![
        faux_assistant_message("initial reply", FauxAssistantMessageOptions::default()).into(),
        faux_assistant_message("steer done", FauxAssistantMessageOptions::default()).into(),
    ]);

    env.session
        .prompt("hello", PromptOptions::default())
        .await
        .expect("prompt");

    let transport = read_runtime_mailbox_message(&env.control_db, transport_id)
        .expect("read transport")
        .expect("transport");
    assert_eq!(transport.status, RuntimeMailboxMessageStatus::Delivered);
    let agent = store.get_agent("agent_1").expect("agent");
    assert_eq!(agent.lifecycle, AgentLifecycle::Completed);
    assert_eq!(
        agent.result.as_ref().and_then(|result| result.summary.as_deref()),
        Some("steer done")
    );
    let status = store
        .list_mailbox_messages()
        .into_iter()
        .find(|candidate| candidate.id == message.id)
        .map(|candidate| candidate.status);
    assert_eq!(
        status,
        Some(pi_coding_agent::core::multi_agent_store::MailboxMessageStatusValue::Delivered)
    );
}

#[tokio::test(start_paused = true)]
async fn steered_parent_terminal_result_waits_for_its_active_child() {
    let env = test_env(true).await;
    env.session
        .bind_extensions(ExtensionBindings {
            control_db_path: Some(env.control_db.clone()),
            ..Default::default()
        })
        .await;
    let store = Arc::clone(env.store.as_ref().expect("store"));
    let parent = reserve_agent(&store, &env, "agent_1", None);
    let child = reserve_agent(&store, &env, "agent_2", Some("agent_1"));
    let sender = RuntimeMailboxAddress {
        agent_id: None,
        session_id: session_id(&env),
    };
    pi_coding_agent::core::session_control_db::register_runtime_mailbox_listener(
        &env.control_db,
        &sender,
        std::process::id(),
        Some(&session_path(&env)),
        Default::default(),
    )
    .expect("register sender listener");
    let steered = request_agent_steering(
        &store,
        &AgentSteeringRequest {
            agent_id: parent.id.clone(),
            message: "Complete after child".to_owned(),
            file_refs: None,
            target_checkpoint: None,
        },
        &AgentSteeringRuntimeBinding {
            actor_agent_id: None,
            control_db_path: env.control_db.clone(),
            session_id: session_id(&env),
        },
        None,
    )
    .expect("steering request");
    let AgentSteeringRequestResult::Ok { message, .. } = steered else {
        panic!("steering rejected: {steered:?}");
    };
    enqueue_steering_transport(&env, &message.id, "agent_1");
    env.faux.set_responses(vec![
        faux_assistant_message("initial reply", FauxAssistantMessageOptions::default()).into(),
        faux_assistant_message("preserved parent result", FauxAssistantMessageOptions::default()).into(),
    ]);

    env.session
        .prompt("hello", PromptOptions::default())
        .await
        .expect("prompt");
    assert_eq!(
        store.get_agent("agent_1").expect("parent").lifecycle,
        AgentLifecycle::Running
    );

    let coordinator = create_lifecycle_coordinator(&store).expect("coordinator");
    let ownership = pi_coding_agent::core::session_control_db::read_multi_agent_runtime_ownership(
        &env.control_db,
        &session_path(&env),
        &child.id,
    )
    .expect("ownership read")
    .expect("ownership");
    let finalized = coordinator
        .finalize_child(
            &pi_coding_agent::core::lifecycle_coordinator::FinalizeChildCommandInput {
                agent: child,
                ownership,
                error: None,
                result: Some(pi_coding_agent::core::multi_agent_store::AgentResult {
                    summary: Some("child done".to_owned()),
                    file_refs: None,
                    duration_ms: None,
                    tool_call_id: None,
                }),
                terminal_lifecycle: "completed".to_owned(),
            },
        )
        .expect("finalize child");
    let pi_coding_agent::core::lifecycle_coordinator::LifecycleCommandResult::Ok { agent } = finalized else {
        panic!("child finalize rejected");
    };
    store.publish_lifecycle_coordinator_snapshot(&agent);

    let parent = store.get_agent("agent_1").expect("parent");
    assert_eq!(parent.lifecycle, AgentLifecycle::Completed);
    assert_eq!(
        parent.result.as_ref().and_then(|result| result.summary.as_deref()),
        Some("preserved parent result")
    );
}

#[tokio::test(start_paused = true)]
async fn polling_delivers_an_idle_prompt_after_the_interval() {
    let env = test_env(false).await;
    env.session
        .bind_extensions(ExtensionBindings {
            control_db_path: Some(env.control_db.clone()),
            ..Default::default()
        })
        .await;
    env.faux.set_responses(vec![
        faux_assistant_message("idle reply", FauxAssistantMessageOptions::default()).into(),
    ]);
    enqueue_stored_message(&env, "Idle notice", "runtime_test_idle", None);
    advance(30_000).await;
    let texts = user_texts(&env.session);
    assert_eq!(texts.len(), 1);
    assert!(texts[0].contains("Idle notice"), "prompt: {:?}", texts);
}
