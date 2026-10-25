//! Behavioral tests for the agent-session runtime mailbox timers and steering
//! completion (`core/agent-session.ts`).

#[path = "agent_session_runtime_mailbox_delivery.rs"]
mod delivery;
#[path = "agent_session_runtime_mailbox_finalize.rs"]
mod finalize;

#[path = "agent_session_runtime_mailbox_prompt.rs"]
mod prompt_boundary;

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
use pi_coding_agent::core::model_registry::ModelRegistry;
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
use pi_js_compat::js_json::JsString;
use pi_js_compat::{js_json, js_json as json};

struct TestPersistence {
    control_db_path: String,
    session_path: JsString,
}

impl SessionPersistence for TestPersistence {
    fn metadata_control_db_path(&self) -> Option<pi_js_compat::js_json::JsString> {
        Some(self.control_db_path.as_str().into())
    }

    fn session_file(&self) -> Option<pi_js_compat::js_json::JsString> {
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
            name: Some("Faux Model".into()),
            reasoning: Some(true),
            input: None,
            cost: None,
            context_window: Some(100_000.0),
            max_tokens: Some(4096.0),
        }]),
        ..Default::default()
    })
}

async fn faux_runtime(faux: &FauxProviderHandle) -> ModelRegistry {
    let mut data = serde_json::Map::new();
    data.insert(
        "faux".to_owned(),
        serde_json::json!({"type": "api_key", "key": "faux-key"}),
    );
    let runtime = ModelRegistry::in_memory(AuthStorage::in_memory(data)).unwrap();
    scripted_registry::register_scripted_provider(&runtime, Arc::clone(&faux.provider));
    runtime.refresh().unwrap();
    runtime
}

async fn test_env(with_store: bool) -> TestEnv {
    test_env_with_tools(with_store, Vec::new()).await
}

async fn test_env_with_tools(
    with_store: bool,
    custom_tool_definitions: Vec<pi_coding_agent::core::extensions::types::ToolDefinition>,
) -> TestEnv {
    test_env_with_resources(with_store, custom_tool_definitions, None).await
}

async fn test_env_with_resources(
    with_store: bool,
    custom_tool_definitions: Vec<pi_coding_agent::core::extensions::types::ToolDefinition>,
    extensions: Option<pi_coding_agent::core::extensions::types::LoadExtensionsResult>,
) -> TestEnv {
    let dir = tempfile::tempdir().expect("temp dir");
    let cwd = dir.path().join("repo").to_str().expect("utf-8 test path").to_owned();
    std::fs::create_dir_all(&cwd).expect("cwd");
    let agent_dir = dir.path().join("agent").to_str().expect("utf-8 test path").to_owned();
    std::fs::create_dir_all(&agent_dir).expect("agent dir");
    let session_dir = dir
        .path()
        .join("sessions")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let control_db = get_control_db_path(pi_js_compat::node_fs::decode_node_path((dir.path()).as_os_str()))
        .to_utf8()
        .expect("fixture control path is scalar");
    let initial_control_db = get_control_db_path(pi_js_compat::node_fs::decode_node_path(
        (&dir.path().join("initial")).as_os_str(),
    ))
    .to_utf8()
    .expect("fixture control path is scalar");
    let faux = faux();
    let runtime = faux_runtime(&faux).await;
    let model = faux.get_model(None).expect("model");
    let mut manager = SessionManager::create(&JsString::from(&cwd), Some(&JsString::from(&session_dir)), None)
        .expect("session manager");
    manager
        .set_metadata_control_db_path(Some(&JsString::from(&initial_control_db)), Default::default())
        .expect("metadata control db");
    let store = with_store.then(|| Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default())));
    let session_id = manager.get_session_id().to_owned();
    let result = create_agent_session(CreateAgentSessionOptions {
        cwd: Some(cwd.clone().into()),
        agent_dir: Some(agent_dir.clone().into()),
        model_registry: Some(runtime),
        model: Some(model),
        resource_loader: Some(Arc::new(StaticResourceLoader {
            extensions,
            ..Default::default()
        })),
        session_manager: Some(manager),
        control_db_path: Some(initial_control_db.clone().into()),
        multi_agent_store: store.clone(),
        custom_tool_definitions,
        multi_agent_agent_id: with_store.then(|| "agent_1".into()),
        multi_agent_parent_session_id: with_store.then_some(session_id),
        ..Default::default()
    })
    .await
    .expect("session");
    result.session.set_active_tools_by_name(&[]).unwrap();
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
        .set_metadata_control_db_path(Some(&JsString::from(&control_db)), Default::default())
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

fn session_id(env: &TestEnv) -> JsString {
    lock_session_manager(&env.session_manager).get_session_id().to_owned()
}

fn session_path(env: &TestEnv) -> JsString {
    lock_session_manager(&env.session_manager)
        .get_session_file()
        .expect("session file")
        .to_owned()
}

fn user_texts(session: &AgentSession) -> Vec<JsString> {
    session
        .messages()
        .iter()
        .filter_map(|message| match message {
            AgentMessage::User(user) => match &user.content {
                pi_ai::types::UserContent::Text(text) => Some(text.clone()),
                pi_ai::types::UserContent::Blocks(blocks) => Some(JsString::join(
                    blocks.iter().filter_map(|block| match block {
                        pi_ai::types::TextOrImageContent::Text(text) => Some(text.text.clone()),
                        pi_ai::types::TextOrImageContent::Image(_) => None,
                    }),
                    "\n",
                )),
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
            recipient_host: None,
            kind: RuntimeMailboxMessageKind::Message,
            recipient: RuntimeMailboxAddress {
                agent_id: recipient_agent.map(JsString::from),
                session_id: session_id(env).into(),
            },
            sender: RuntimeMailboxAddress {
                agent_id: Some("agent_1".to_owned().into()),
                session_id: "child-session".to_owned().into(),
            },
            store_ref: RuntimeMailboxStoreRef {
                message_id: message_id.to_owned().into(),
                session_path: session_path.into(),
            },
        },
    )
    .expect("enqueue runtime message")
}

fn enqueue_steering_transport(env: &TestEnv, message_id: &JsString, agent_id: &str) -> i64 {
    enqueue_runtime_mailbox_message(
        &env.control_db,
        &EnqueueRuntimeMailboxMessageInput {
            recipient_host: None,
            kind: RuntimeMailboxMessageKind::Steer,
            recipient: RuntimeMailboxAddress {
                agent_id: Some(agent_id.to_owned().into()),
                session_id: session_id(env).into(),
            },
            sender: RuntimeMailboxAddress {
                agent_id: None,
                session_id: session_id(env).into(),
            },
            store_ref: RuntimeMailboxStoreRef {
                message_id: message_id.to_owned().into(),
                session_path: session_path(env).into(),
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
    let prepared = coordinator
        .prepare_child(&PrepareChildCommandInput {
            spawn: SpawnAgent {
                parent_id: Some(parent_id.unwrap_or("main").to_owned().into()),
                display_name: format!("Agent {id}").into(),
                agent_type: "worker".into(),
                origin: None,
                cwd: env.cwd.clone().into(),
                permission: AgentPermission {
                    policy: "on-request".into(),
                    inherited_from: None,
                    narrowed: true,
                },
                worktree: None,
                model: None,
                account: None,
                slot: None,
                transcript: Some(pi_coding_agent::core::multi_agent_store::AgentTranscriptMetadata {
                    session_id: session_id(env).into(),
                    path: Some(session_path(env).into()),
                }),
                event_stream: None,
                worker: None,
            },
            agent_id: Some(id.to_owned().into()),
            detached: None,
            result: None,
        })
        .unwrap();
    let created = coordinator
        .commit_running_child(
            &prepared,
            &JsString::from(&session_id(env)),
            pi_coding_agent::bundled_extensions::agents_core::lifecycle_runtime::runtime_process_identity(),
            None,
        )
        .expect("commit child");
    let agent = match created {
        CreateMultiAgentChildWithRuntimeOwnershipResult::Ok { agent, .. } => *agent,
        other => panic!("unexpected child creation: {other:?}"),
    };
    store.publish_lifecycle_coordinator_snapshot(&agent).unwrap();
    agent
}

#[tokio::test(start_paused = true)]
async fn acceptance_tracks_distinct_messages_once_and_releases_only_unaccepted_claims() {
    use pi_coding_agent::core::runtime_mailbox_acceptance::{
        RuntimeMailboxAcceptance, RuntimeMailboxAcceptanceRef, release_unaccepted_runtime_mailbox_messages,
    };
    use pi_coding_agent::core::session_control_db::{
        claim_runtime_mailbox_messages, register_runtime_mailbox_listener,
    };
    let env = test_env(false).await;
    let recipient = RuntimeMailboxAddress {
        agent_id: None,
        session_id: session_id(&env).into(),
    };
    register_runtime_mailbox_listener(
        &env.control_db,
        &recipient,
        std::process::id(),
        Some(&JsString::from(&session_path(&env))),
        Default::default(),
    )
    .expect("listener");
    let first_id = enqueue_stored_message(&env, "first", "first", None);
    let second_id = enqueue_stored_message(&env, "second", "second", None);
    let claims = claim_runtime_mailbox_messages(&env.control_db, &recipient, 10).expect("claim");
    let refs = claims.iter().map(RuntimeMailboxAcceptanceRef::from).collect::<Vec<_>>();
    assert_eq!(refs.len(), 2);
    let first: AgentMessage =
        js_json::from_value(&json!({"role":"user", "content":"first", "timestamp":1})).expect("first user");
    let second: AgentMessage =
        js_json::from_value(&json!({"role":"user", "content":"second", "timestamp":2})).expect("second user");
    let mut acceptance = RuntimeMailboxAcceptance::default();
    {
        let mut manager = lock_session_manager(&env.session_manager);
        acceptance
            .accept_messages(&mut manager, &JsString::from(&env.control_db), &refs[..1], Some(&first))
            .expect("accept first before any assistant");
        release_unaccepted_runtime_mailbox_messages(&manager, &JsString::from(&env.control_db), &refs)
            .expect("release second");
    }
    assert!(
        read_runtime_mailbox_message(&env.control_db, first_id)
            .expect("first removed")
            .is_none()
    );
    assert_eq!(
        read_runtime_mailbox_message(&env.control_db, second_id)
            .expect("read second")
            .expect("second pending")
            .status,
        RuntimeMailboxMessageStatus::Pending
    );
    assert!(!acceptance.consume_persisted_message(&second));
    assert!(acceptance.consume_persisted_message(&first));
    assert!(!acceptance.consume_persisted_message(&first));
    let second_claim = claim_runtime_mailbox_messages(&env.control_db, &recipient, 10).expect("reclaim second");
    let second_refs = second_claim
        .iter()
        .map(RuntimeMailboxAcceptanceRef::from)
        .collect::<Vec<_>>();
    acceptance
        .accept_messages(
            &mut lock_session_manager(&env.session_manager),
            &JsString::from(&env.control_db),
            &second_refs,
            Some(&second),
        )
        .expect("accept distinct second");
    assert!(!acceptance.consume_persisted_message(&first));
    assert!(acceptance.consume_persisted_message(&second));
    assert!(!acceptance.consume_persisted_message(&second));
    let receipts = pi_coding_agent::core::runtime_mailbox_transcript::read_accepted_runtime_mailbox_store_refs(Some(
        &JsString::from(&session_path(&env)),
    ))
    .expect("receipts");
    assert_eq!(receipts.len(), 2);
}

#[tokio::test(start_paused = true)]
async fn rejected_unpersisted_acceptance_keeps_claim_until_finally_release() {
    use pi_coding_agent::core::runtime_mailbox_acceptance::{
        RuntimeMailboxAcceptance, RuntimeMailboxAcceptanceRef, release_unaccepted_runtime_mailbox_messages,
    };
    use pi_coding_agent::core::session_control_db::{
        claim_runtime_mailbox_messages, register_runtime_mailbox_listener,
    };
    let env = test_env(false).await;
    let recipient = RuntimeMailboxAddress {
        agent_id: None,
        session_id: session_id(&env).into(),
    };
    register_runtime_mailbox_listener(
        &env.control_db,
        &recipient,
        std::process::id(),
        Some(&JsString::from(&session_path(&env))),
        Default::default(),
    )
    .expect("listener");
    let id = enqueue_stored_message(&env, "retry me", "retry", None);
    let claims = claim_runtime_mailbox_messages(&env.control_db, &recipient, 10).expect("claim");
    let refs = claims.iter().map(RuntimeMailboxAcceptanceRef::from).collect::<Vec<_>>();
    let mut recipient_session =
        SessionManager::in_memory(Some(&JsString::from(&env.cwd)), None).expect("memory session");
    let failure = RuntimeMailboxAcceptance::default()
        .accept_messages(&mut recipient_session, &JsString::from(&env.control_db), &refs, None)
        .expect_err("must persist receipt");
    assert_eq!(
        failure.message(),
        "Mailbox acceptance requires a persisted recipient session"
    );
    assert_eq!(
        read_runtime_mailbox_message(&env.control_db, id)
            .expect("read claim")
            .expect("claim remains")
            .status,
        RuntimeMailboxMessageStatus::Claimed
    );
    release_unaccepted_runtime_mailbox_messages(&recipient_session, &JsString::from(&env.control_db), &refs)
        .expect("finally release");
    assert_eq!(
        read_runtime_mailbox_message(&env.control_db, id)
            .expect("read release")
            .expect("retry remains")
            .status,
        RuntimeMailboxMessageStatus::Pending
    );
}

#[tokio::test(start_paused = true)]
async fn question_tool_result_persists_receipt_before_consuming_answer() {
    let route = Arc::new(std::sync::Mutex::new((String::new(), JsString::default())));
    let target = route.clone();
    let mut tool = pi_coding_agent::core::tools::ask_questions::create_ask_questions_tool_definition();
    tool.name = "receive_answer".into();
    tool.parameters = serde_json::json!({"type":"object","properties":{}});
    tool.execute = Arc::new(move |_, _, _, _, _| {
        let (db, session) = target.lock().unwrap().clone();
        Box::pin(async move {
            let claims = pi_coding_agent::core::session_control_db::take_runtime_mailbox_messages_for_delivery(
                &db,
                &RuntimeMailboxAddress {
                    session_id: session.into(),
                    agent_id: None,
                },
                |_| Ok(true),
                1,
            )
            .unwrap();
            assert_eq!(claims.len(), 1);
            Ok(pi_agent_core::AgentToolResult {
                content: vec![pi_ai::types::TextOrImageContent::Text(pi_ai::types::TextContent::new(
                    "Parent answered questions: A",
                ))],
                details: Some(
                    json!({"runtimeMailboxDelivery":{"controlDbPath":db,"messages":[{"id":claims[0].id,"storeRef":claims[0].store_ref}]}}),
                ),
                is_error: None,
                terminate: None,
            })
        })
    });
    let env = test_env_with_tools(false, vec![tool]).await;
    *route.lock().unwrap() = (env.control_db.clone(), session_id(&env));
    pi_coding_agent::core::session_control_db::register_runtime_mailbox_listener(
        &env.control_db,
        &RuntimeMailboxAddress {
            session_id: session_id(&env).into(),
            agent_id: None,
        },
        std::process::id(),
        Some(&JsString::from(&session_path(&env))),
        Default::default(),
    )
    .unwrap();
    let id = enqueue_stored_message(&env, "answer", "ask_questions:child:call", None);
    env.session
        .set_active_tools_by_name(&["receive_answer".into()])
        .unwrap();
    env.faux.set_responses(vec![
        faux_assistant_message(
            vec![pi_ai::providers::faux::faux_tool_call(
                "receive_answer",
                Default::default(),
                Some("call".into()),
            )],
            Default::default(),
        )
        .into(),
        faux_assistant_message("done", Default::default()).into(),
    ]);
    env.session.prompt("ask", PromptOptions::default()).await.unwrap();
    let receipts = pi_coding_agent::core::runtime_mailbox_transcript::read_accepted_runtime_mailbox_store_refs(Some(
        &JsString::from(&session_path(&env)),
    ))
    .unwrap();
    assert_eq!(receipts.len(), 1, "tool result must carry durable answer receipt");
    assert!(read_runtime_mailbox_message(&env.control_db, id).unwrap().is_none());
    let records = std::fs::read_to_string(pi_js_compat::js_json::encode_node_utf8(&session_path(&env))).unwrap();
    assert_eq!(
        records
            .lines()
            .filter(|line| line.contains("\"role\":\"toolResult\""))
            .count(),
        1,
        "tool result persisted once"
    );
}

#[tokio::test(start_paused = true)]
async fn idle_mailbox_prompt_completes_tool_result_and_next_assistant_turn() {
    let env = test_env(false).await;
    assert!(!env.session.runtime_coordination_inbound_disabled());
    env.session.set_active_tools_by_name(&["read".to_owned()]).unwrap();
    env.session
        .bind_extensions(ExtensionBindings {
            control_db_path: Some(env.control_db.clone().into()),
            ..Default::default()
        })
        .await
        .expect("bind runtime mailbox extensions");
    pi_coding_agent::core::session_control_db::register_runtime_mailbox_listener(
        &env.control_db,
        &RuntimeMailboxAddress {
            agent_id: None,
            session_id: session_id(&env).into(),
        },
        std::process::id(),
        Some(&JsString::from(&session_path(&env))),
        Default::default(),
    )
    .expect("listener ready for idle drain");
    advance(30_000).await;
    let fixture_path = std::path::Path::new(&env.cwd).join("mailbox-tool.txt");
    std::fs::write(&fixture_path, "mailbox tool result").expect("tool fixture");
    env.faux.set_responses(vec![
        faux_assistant_message(
            vec![pi_ai::providers::faux::faux_tool_call(
                "read",
                json!({"path":fixture_path}).as_object().unwrap().clone(),
                Some("mailbox-read".to_owned()),
            )],
            FauxAssistantMessageOptions::default(),
        )
        .into(),
        faux_assistant_message("mailbox prompt completed", FauxAssistantMessageOptions::default()).into(),
    ]);
    let id = enqueue_stored_message(&env, "Read the fixture", "mailbox-tool-turn", None);
    assert!(!env.session.is_streaming());

    tokio::time::timeout(Duration::from_secs(5), env.session.drain_runtime_coordination())
        .await
        .unwrap_or_else(|_| {
            panic!(
                "idle mailbox prompt deadlocked after its tool result: {:#?}",
                env.session.messages()
            )
        })
        .expect("mailbox prompt");

    let messages = env.session.messages();
    let result_index = messages
        .iter()
        .position(|message| {
            matches!(message, AgentMessage::ToolResult(result)
            if result.tool_call_id == "mailbox-read" && !result.is_error)
        })
        .unwrap_or_else(|| panic!("successful read tool result missing: {messages:#?}"));
    assert!(
        messages[result_index + 1..].iter().any(|message| matches!(
            message,
            AgentMessage::Assistant(assistant) if assistant.content.iter().any(|block| matches!(
                block,
                pi_ai::types::AssistantContent::Text(text) if text.text == "mailbox prompt completed"
            ))
        )),
        "assistant must continue after the tool result"
    );
    assert_eq!(user_texts(&env.session).len(), 1, "one accepted mailbox prompt");
    assert!(
        read_runtime_mailbox_message(&env.control_db, id)
            .expect("accepted row")
            .is_none()
    );
    let receipts = pi_coding_agent::core::runtime_mailbox_transcript::read_accepted_runtime_mailbox_store_refs(Some(
        &JsString::from(&session_path(&env)),
    ))
    .expect("durable receipt");
    assert_eq!(receipts.len(), 1);
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
            control_db_path: Some(env.control_db.clone().into()),
            ..Default::default()
        })
        .await
        .expect("bind runtime mailbox extensions");
    env.faux.set_responses(vec![
        faux_assistant_message("mailbox reply", FauxAssistantMessageOptions::default()).into(),
    ]);
    let second_id = enqueue_stored_message(&env, "Second distinct message", "runtime_test_poll_second", None);
    let second = read_runtime_mailbox_message(&env.control_db, second_id)
        .expect("read second")
        .expect("second");
    let expected_prompt = pi_js_compat::js_format!(
        "{}\n\n{}",
        format_runtime_mailbox_prompt(&message, &session_id(&env)).expect("first prompt"),
        format_runtime_mailbox_prompt(&second, &session_id(&env)).expect("second prompt")
    );
    advance(30_000).await;
    assert_eq!(user_texts(&env.session), vec![expected_prompt]);
    let accepted = pi_coding_agent::core::runtime_mailbox_transcript::read_accepted_runtime_mailbox_store_refs(Some(
        &JsString::from(&session_path(&env)),
    ))
    .expect("receipt refs");
    assert_eq!(accepted.len(), 2, "both distinct messages must have durable receipts");
    let transcript =
        std::fs::read_to_string(pi_js_compat::js_json::encode_node_utf8(&session_path(&env))).expect("transcript");
    let user_records = transcript
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("record"))
        .filter(|entry| entry["type"] == "message" && entry["message"]["role"] == "user")
        .count();
    assert_eq!(
        user_records, 1,
        "accepted prompt must not be appended again at MessageEnd"
    );
    assert!(
        read_runtime_mailbox_message(&env.control_db, second_id)
            .expect("second removed")
            .is_none()
    );
    // Frozen session-control-db.ts:1092-1101 deletes delivered canonical rows.
    assert!(
        read_runtime_mailbox_message(&env.control_db, message_id)
            .expect("read message")
            .is_none()
    );
    assert_ne!(env.control_db, env.initial_control_db);
}

#[tokio::test(start_paused = true)]
async fn polling_does_not_claim_messages_without_a_matching_listener() {
    let env = test_env(false).await;
    env.session
        .bind_extensions(ExtensionBindings {
            control_db_path: Some(env.control_db.clone().into()),
            ..Default::default()
        })
        .await
        .expect("bind runtime mailbox extensions");
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
            recipient_host: None,
            kind: RuntimeMailboxMessageKind::Message,
            recipient: RuntimeMailboxAddress {
                agent_id: recipient_agent.map(JsString::from),
                session_id: recipient_session.to_owned().into(),
            },
            sender: RuntimeMailboxAddress {
                agent_id: Some("agent_1".to_owned().into()),
                session_id: "child-session".to_owned().into(),
            },
            store_ref: RuntimeMailboxStoreRef {
                message_id: message_id.to_owned().into(),
                session_path: session_path.into(),
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
            control_db_path: Some(env.control_db.clone().into()),
            ..Default::default()
        })
        .await
        .expect("bind runtime mailbox extensions");
    let store = Arc::clone(env.store.as_ref().expect("store"));
    let running = reserve_agent(&store, &env, "agent_1", None);
    let sender = RuntimeMailboxAddress {
        agent_id: None,
        session_id: session_id(&env).into(),
    };
    pi_coding_agent::core::session_control_db::register_runtime_mailbox_listener(
        &env.control_db,
        &sender,
        std::process::id(),
        Some(&JsString::from(&session_path(&env))),
        Default::default(),
    )
    .expect("register sender listener");
    let steered = request_agent_steering(
        &store,
        &AgentSteeringRequest {
            agent_id: running.id.clone(),
            message: "Continue verification".to_owned().into(),
            file_refs: None,
            target_checkpoint: None,
        },
        &AgentSteeringRuntimeBinding {
            actor_agent_id: None,
            control_db_path: env.control_db.clone().into(),
            session_id: session_id(&env).into(),
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
        faux_assistant_message("steer done", FauxAssistantMessageOptions::default()).into(),
    ]);

    env.session
        .prompt("hello", PromptOptions::default())
        .await
        .expect("prompt");

    // Frozen mailbox rowids can be reused by the new terminal notification.
    // Assert the immutable store reference, not absence of a retired rowid.
    assert!(
        pi_coding_agent::core::session_control_db::get_multi_agent_mailbox_message_status(
            &env.control_db,
            &session_path(&env),
            &message.id,
        )
        .expect("steering status")
        .is_none()
    );
    let agent = store.get_agent(&JsString::from("agent_1")).expect("agent");
    assert_eq!(agent.lifecycle, AgentLifecycle::Completed);
    assert_eq!(
        agent.result.as_ref().and_then(|result| result.summary.as_ref()),
        Some(&JsString::from("steer done"))
    );
    let status = store
        .list_mailbox_messages()
        .unwrap()
        .into_iter()
        .find(|candidate| candidate.id == message.id)
        .map(|candidate| candidate.status);
    // Frozen multi-agent-store.ts:366-368 removes the steering projection on delivery.
    assert_eq!(status, None);
}

#[tokio::test(start_paused = true)]
async fn steered_parent_terminal_result_waits_for_its_active_child() {
    let env = test_env(true).await;
    env.session
        .bind_extensions(ExtensionBindings {
            control_db_path: Some(env.control_db.clone().into()),
            ..Default::default()
        })
        .await
        .expect("bind runtime mailbox extensions");
    let store = Arc::clone(env.store.as_ref().expect("store"));
    let parent = reserve_agent(&store, &env, "agent_1", None);
    let child = reserve_agent(&store, &env, "agent_2", Some("agent_1"));
    let sender = RuntimeMailboxAddress {
        agent_id: None,
        session_id: session_id(&env).into(),
    };
    pi_coding_agent::core::session_control_db::register_runtime_mailbox_listener(
        &env.control_db,
        &sender,
        std::process::id(),
        Some(&JsString::from(&session_path(&env))),
        Default::default(),
    )
    .expect("register sender listener");
    let steered = request_agent_steering(
        &store,
        &AgentSteeringRequest {
            agent_id: parent.id.clone(),
            message: "Complete after child".to_owned().into(),
            file_refs: None,
            target_checkpoint: None,
        },
        &AgentSteeringRuntimeBinding {
            actor_agent_id: None,
            control_db_path: env.control_db.clone().into(),
            session_id: session_id(&env).into(),
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
        store.get_agent(&JsString::from("agent_1")).expect("parent").lifecycle,
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
                    summary: Some("child done".to_owned().into()),
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
    store.publish_lifecycle_coordinator_snapshot(&agent).unwrap();

    let parent = store.get_agent(&JsString::from("agent_1")).expect("parent");
    assert_eq!(parent.lifecycle, AgentLifecycle::Completed);
    assert_eq!(
        parent.result.as_ref().and_then(|result| result.summary.as_ref()),
        Some(&JsString::from("preserved parent result"))
    );
}

#[tokio::test(start_paused = true)]
async fn polling_delivers_an_idle_prompt_after_the_interval() {
    let env = test_env(false).await;
    env.session
        .bind_extensions(ExtensionBindings {
            control_db_path: Some(env.control_db.clone().into()),
            ..Default::default()
        })
        .await
        .expect("bind runtime mailbox extensions");
    env.faux.set_responses(vec![
        faux_assistant_message("idle reply", FauxAssistantMessageOptions::default()).into(),
    ]);
    enqueue_stored_message(&env, "Idle notice", "runtime_test_idle", None);
    advance(30_000).await;
    let texts = user_texts(&env.session);
    assert_eq!(texts.len(), 1);
    assert!(texts[0].contains("Idle notice"), "prompt: {:?}", texts);
}

#[path = "support/scripted_registry.rs"]
mod scripted_registry;
