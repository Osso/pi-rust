//! Frozen question-relay-tools behavior over real, isolated SQLite mailboxes.
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/tool_context.rs"]
mod tool_context;

use pi_coding_agent::bundled_extensions::agents_core::runtime::{MultiAgentExtensionOptions, multi_agent_extension};
use pi_coding_agent::core::extensions::{
    ExtensionContext, ToolDefinition, create_extension_runtime, load_extension_from_factory,
};
use pi_coding_agent::core::multi_agent_store::{MultiAgentStore, MultiAgentStoreOptions, SessionPersistence};
use pi_coding_agent::core::session_control_db::{get_control_db_path, list_runtime_mailbox_messages};
use pi_coding_agent::core::session_manager::lock_session_manager;
use pi_js_compat::js_json::{JsString, JsValue as Value};
use pi_js_compat::{js_json, js_json as json};
use std::sync::Arc;

struct Persistence {
    db: String,
    path: JsString,
}
impl SessionPersistence for Persistence {
    fn metadata_control_db_path(&self) -> Option<pi_js_compat::js_json::JsString> {
        Some(self.db.as_str().into())
    }
    fn session_file(&self) -> Option<pi_js_compat::js_json::JsString> {
        Some(self.path.clone())
    }
}
struct Fixture {
    _dir: tempfile::TempDir,
    store: Arc<MultiAgentStore>,
    db: String,
    session: pi_coding_agent::core::session_manager::SharedSessionManager,
    ask: ToolDefinition,
    answer: ToolDefinition,
    send: ToolDefinition,
}
impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().expect("utf-8 test path");
        let session = tool_context::persisted_session(root, root, "parent-session");
        let db = get_control_db_path(pi_js_compat::node_fs::decode_node_path((dir.path()).as_os_str()))
            .to_utf8()
            .expect("fixture control path is scalar");
        let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default()));
        store.set_persistence_session_manager(Some(&Persistence {
            db: db.clone(),
            path: lock_session_manager(&session).get_session_file().unwrap().to_owned(),
        }));
        for (id, parent) in [("child", "main"), ("sibling", "main"), ("grandchild", "child")] {
            let mut agent = pi_coding_agent::bundled_extensions::agents_core::shared::empty_agent(id);
            agent.parent_id = Some(parent.into());
            agent.display_name = id.into();
            agent.lifecycle = pi_coding_agent::core::multi_agent_store::AgentLifecycle::Running;
            agent.transcript = Some(pi_coding_agent::core::multi_agent_store::AgentTranscriptMetadata {
                session_id: format!("session-{id}").into(),
                path: None,
            });
            pi_coding_agent::core::session_control_db::bootstrap_multi_agent_agent(
                &db,
                lock_session_manager(&session).get_session_file().unwrap(),
                &JsString::from(id),
                &js_json::to_value(&agent).unwrap(),
            )
            .unwrap();
            store.publish_lifecycle_coordinator_snapshot(&agent).unwrap();
        }
        let options = MultiAgentExtensionOptions {
            store: Some(store.clone()),
            ..Default::default()
        };
        let extension = load_extension_from_factory(
            Arc::new(move |pi| {
                let result = multi_agent_extension(pi, &options);
                Box::pin(async move { result })
            }),
            &JsString::from(root),
            Arc::new(pi_coding_agent::core::event_bus::EventBusController::new()),
            create_extension_runtime(),
            &JsString::from("<questions>"),
        )
        .await
        .unwrap();
        let send = extension
            .read()
            .tools
            .get("send_agent_message")
            .unwrap()
            .definition
            .clone();
        let ask = extension
            .read()
            .tools
            .get("ask_questions")
            .expect("child-aware ask_questions registered")
            .definition
            .clone();
        let answer = extension
            .read()
            .tools
            .get("answer_question")
            .expect("answer_question registered")
            .definition
            .clone();
        Self {
            _dir: dir,
            store,
            db,
            session,
            ask,
            answer,
            send,
        }
    }
    async fn context(&self, agent: Option<&str>, requires: bool) -> ExtensionContext {
        let session = if let Some(id) = agent {
            tool_context::in_memory_session("/repo", format!("session-{id}"))
        } else {
            self.session.clone()
        };
        let mut builder = tool_context::TestContextBuilder::new("/repo", session)
            .control_db_path(&self.db)
            .requires_agent_id(requires);
        if let Some(id) = agent {
            builder = builder.agent_id(id);
        }
        builder.build().await.1
    }
}
fn questions(blocking: bool) -> Value {
    json!({"blocking": blocking, "questions": [{"question":"Choose?", "header":"Path", "options":[{"label":"A","description":"first"},{"label":"B"}]}], "metadata":{"trace":1}})
}

#[tokio::test]
async fn child_nonblocking_relays_formatted_request_and_correlates_answers() {
    let f = Fixture::new().await;
    let result = (f.ask.execute)(
        "call-1".into(),
        json!(questions(false)),
        None,
        None,
        Some(f.context(Some("child"), true).await),
    )
    .await
    .unwrap();
    assert_eq!(result.details.as_ref().unwrap()["requestId"], json!("call-1"));
    assert_eq!(result.details.as_ref().unwrap()["queued"], json!(true));
    let requests = f
        .store
        .list_pending_mailbox_messages_for_agent(&JsString::from("main"))
        .unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].thread_id.as_ref(),
        Some(&JsString::from("ask_questions:child:call-1"))
    );
    assert_eq!(
        requests[0].body.as_ref().unwrap(),
        "Child agent child asks (requestId: call-1). The child continues meanwhile.\nReply with answer_question using agentId \"child\" and requestId \"call-1\", answers keyed by exact question text. Ask the user with ask_questions first when you cannot answer.\n\n1. [Path] Choose?\n   Choose one, or give another answer:\n   - A — first\n   - B"
    );
    (f.answer.execute)(
        "answer".into(),
        json!({"agentId":"child", "requestId":"call-1", "answers":{"Choose?":"A"}}),
        None,
        None,
        Some(f.context(None, false).await),
    )
    .await
    .unwrap();
    let messages = list_runtime_mailbox_messages(&f.db).unwrap();
    let answer = messages
        .iter()
        .find(|message| message.store_ref.message_id == "ask_questions:child:call-1")
        .unwrap();
    assert_eq!(
        js_json::parse_lossless(&answer.body).unwrap(),
        json!({"type":"ask_questions_answer", "requestId":"call-1", "answers":{"Choose?":"A"}, "cancelled":false})
    );
    assert_eq!(answer.recipient.session_id, "session-child");
}

#[tokio::test]
async fn child_blocking_cancellation_preserves_sent_request() {
    let f = Fixture::new().await;
    let controller_cancel = pi_js_compat::abort::AbortController::new();
    let cancel = controller_cancel.signal();
    controller_cancel.abort(None);
    let result = (f.ask.execute)(
        "cancelled".into(),
        json!(questions(true)),
        Some(cancel),
        None,
        Some(f.context(Some("child"), true).await),
    )
    .await
    .unwrap();
    assert_eq!(result.details.as_ref().unwrap()["cancelled"], json!(true));
    assert_eq!(
        f.store
            .list_pending_mailbox_messages_for_agent(&JsString::from("main"))
            .unwrap()
            .len(),
        1
    );
    assert!(result.details.as_ref().unwrap().get("runtimeMailboxDelivery").is_none());
}

#[tokio::test]
async fn missing_identity_explicit_blocking_and_direct_child_route_are_enforced() {
    let f = Fixture::new().await;
    let missing = (f.ask.execute)(
        "bad".into(),
        json!({"questions":[{"question":"Free?"}]}),
        None,
        None,
        Some(f.context(Some("child"), true).await),
    )
    .await
    .unwrap_err();
    assert_eq!(
        missing.message,
        "ask_questions requires explicit blocking: true or false"
    );
    let missing = (f.ask.execute)(
        "bad".into(),
        json!(questions(true)),
        None,
        None,
        Some(f.context(None, true).await),
    )
    .await
    .unwrap_err();
    assert_eq!(
        missing.message,
        "ask_questions in a child agent requires its agent identity and control database"
    );
    let denied = (f.answer.execute)(
        "bad".into(),
        json!({"agentId":"grandchild","requestId":"call","answers":{}}),
        None,
        None,
        Some(f.context(None, false).await),
    )
    .await
    .unwrap_err();
    assert_eq!(
        denied.message,
        "Could not answer grandchild: not a direct child of this agent"
    );
}

#[tokio::test]
async fn blocking_reply_claims_only_child_and_request_and_returns_acceptance_descriptor() {
    use pi_coding_agent::core::session_control_db::{RuntimeMailboxAddress, register_runtime_mailbox_listener};
    let f = Fixture::new().await;
    register_runtime_mailbox_listener(
        &f.db,
        &RuntimeMailboxAddress {
            session_id: "session-child".into(),
            agent_id: Some("child".into()),
        },
        std::process::id(),
        None,
        Default::default(),
    )
    .unwrap();
    let parent = f.context(None, false).await;
    for (child, request, cancelled) in [
        ("sibling", "same", false),
        ("child", "other", false),
        ("child", "same", true),
    ] {
        (f.answer.execute)(
            "reply".into(),
            json!({"agentId":child,"requestId":request,"answers":{"Choose?":""},"cancelled":cancelled}),
            None,
            None,
            Some(parent.clone()),
        )
        .await
        .unwrap();
    }
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        (f.ask.execute)(
            "same".into(),
            json!(questions(true)),
            None,
            None,
            Some(f.context(Some("child"), true).await),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(result.details.as_ref().unwrap()["cancelled"], json!(true));
    assert_eq!(result.details.as_ref().unwrap()["answers"], json!({"Choose?":""}));
    let delivery = &result.details.as_ref().unwrap()["runtimeMailboxDelivery"];
    assert_eq!(delivery["messages"].as_array().unwrap().len(), 1);
    assert_eq!(
        delivery["messages"][0]["storeRef"]["messageId"],
        json!("ask_questions:child:same")
    );
    let messages = list_runtime_mailbox_messages(&f.db).unwrap();
    let claimed = messages
        .iter()
        .filter(|message| {
            message.status == pi_coding_agent::core::session_control_db::RuntimeMailboxMessageStatus::Claimed
        })
        .collect::<Vec<_>>();
    assert_eq!(claimed.len(), 1, "answer stays claimed until tool result receipt");
    assert_eq!(claimed[0].store_ref.message_id, "ask_questions:child:same");
}

#[tokio::test]
async fn mailbox_body_and_remote_host_are_literal_and_unknown_local_sessions_are_rejected() {
    let f = Fixture::new().await;
    let context = f.context(None, false).await;
    let remote=(f.send.execute)("remote".into(),json!({"body":"  exact body\n","toAgentId":"child-on-peer","toSessionId":"remote-session","toHost":"peer-host"}),None,None,Some(context.clone())).await.unwrap();
    assert_ne!(remote.is_error, Some(true));
    let messages = list_runtime_mailbox_messages(&f.db).unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].body, "  exact body\n");
    let db = rusqlite::Connection::open(&f.db).unwrap();
    let data: String = db
        .query_row(
            "SELECT data FROM multi_agent_mailbox_messages WHERE message_id = ?",
            [&messages[0].store_ref.message_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&data).unwrap()["recipientHost"],
        json!("peer-host")
    );
    for (params, reason) in [
        (
            json!({"body":"x","toAgentId":"main","toHost":"   "}),
            "toHost must be non-empty.",
        ),
        (
            json!({"body":"x","toAgentId":"main","toHost":"peer-host"}),
            "toHost requires toSessionId.",
        ),
        (
            json!({"body":"x","toAgentId":"main","toSessionId":"unknown-local"}),
            "Unknown local session unknown-local; pass toHost for a session on another host.",
        ),
    ] {
        let result = (f.send.execute)("invalid".into(), json!(params), None, None, Some(context.clone()))
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(true));
        let text = JsString::join(
            result.content.iter().filter_map(|content| match content {
                pi_ai::types::TextOrImageContent::Text(text) => Some(&text.text),
                pi_ai::types::TextOrImageContent::Image(_) => None,
            }),
            "\n",
        );
        assert!(text.contains(reason), "{text:?}");
    }
    assert_eq!(
        list_runtime_mailbox_messages(&f.db).unwrap().len(),
        1,
        "invalid routes never create messages"
    );
}

#[tokio::test]
async fn relay_requests_validation_and_cancellation_match_frozen_whole_module() {
    let mut oracle = oracle::TsOracle::spawn("question-relay-tools-oracle.ts", "packages/coding-agent");
    for (params, agent_id, requires) in [
        (questions(false), Some("child"), true),
        (js_json::parse_lossless(r#"{"blocking":false,"questions":[{"question":"Q\ud800","header":"\udc00","options":[{"label":"A\ud800","description":"D\udc00"}]}],"metadata":{"key\ud800":"value\udc00"}}"#).unwrap(),Some("child"),true),
        (questions(true), Some("child"), true),
        (
            json!({"blocking":false,"questions":[{"question":"Free?","header":"Free"},{"question":"Many?","multiSelect":true,"options":[{"label":"A","description":""},{"label":"B"}]}]}),
            Some("child"),
            true,
        ),
        (json!({"questions":[{"question":"Free?"}]}), Some("child"), true),
        (
            json!({"blocking":false,"questions":[{"question":"Free?","multiSelect":true}]}),
            Some("child"),
            true,
        ),
        (questions(false), None, true),
    ] {
        let f = Fixture::new().await;
        let expected = oracle.request(
            &json!({"id":"call","params":params,"agentId":agent_id,"requiresId":requires,"db":f.db,"cancelled":true}),
        );
        let controller_cancel = pi_js_compat::abort::AbortController::new();
        let cancel = controller_cancel.signal();
        controller_cancel.abort(None);
        let result = (f.ask.execute)(
            "call".into(),
            json!(params.clone()),
            Some(cancel),
            None,
            Some(f.context(agent_id, requires).await),
        )
        .await;
        let sent = f
            .store
            .list_pending_mailbox_messages_for_agent(&JsString::from("main"))
            .unwrap()
            .into_iter()
            .map(|message| json!({"message":message.body,"threadId":message.thread_id}))
            .collect::<Vec<_>>();
        let actual = match result {
            Ok(result) => {
                json!({"result":{"content":result.content,"details":result.details,"isError":result.is_error},"sent":sent})
            }
            Err(error) => json!({"error":error.message,"sent":sent}),
        };
        assert_eq!(actual, expected, "{params:?}");
    }
}

#[tokio::test]
async fn delivered_answer_parsing_and_unchecked_payload_fields_match_frozen() {
    use pi_coding_agent::core::session_control_db::{
        EnqueueStoredRuntimeMailboxMessageInput, RuntimeMailboxAddress, RuntimeMailboxMessageKind,
        RuntimeMailboxStoreRef, enqueue_stored_runtime_mailbox_message, register_runtime_mailbox_listener,
    };
    let mut oracle = oracle::TsOracle::spawn("question-relay-tools-oracle.ts", "packages/coding-agent");
    for body in [
        "not json",
        "null",
        r#"{"type":"other"}"#,
        r#"{"type":"ask_questions_answer","answers":{"Choose?":"A"}}"#,
        r#"{"type":"ask_questions_answer","answers":{"Q\ud800":"A\udc00"},"cancelled":false}"#,
        r#"{"type":"ask_questions_answer","answers":null,"cancelled":false}"#,
        r#"{"type":"ask_questions_answer","answers":{"Choose?":""},"cancelled":"declined"}"#,
    ] {
        let f = Fixture::new().await;
        let expected = oracle.request(
            &json!({"id":"call","params":questions(true),"agentId":"child","requiresId":true,"answerBody":body}),
        );
        let recipient = RuntimeMailboxAddress {
            agent_id: Some("child".into()),
            session_id: "session-child".into(),
        };
        register_runtime_mailbox_listener(&f.db, &recipient, std::process::id(), None, Default::default()).unwrap();
        enqueue_stored_runtime_mailbox_message(&f.db,&EnqueueStoredRuntimeMailboxMessageInput{recipient_host:None,recipient,sender:RuntimeMailboxAddress{agent_id:None,session_id:"parent".into()},kind:RuntimeMailboxMessageKind::Reply,store_ref:RuntimeMailboxStoreRef{session_path:lock_session_manager(&f.session).get_session_file().unwrap().into(),message_id:"ask_questions:child:call".into()},message:json!({"body":body,"id":"ask_questions:child:call","kind":"reply","status":"pending","fromAgentId":"main","toAgentId":"child"}),updated_at:None}).unwrap();
        let result = (f.ask.execute)(
            "call".into(),
            json!(questions(true)),
            None,
            None,
            Some(f.context(Some("child"), true).await),
        )
        .await;
        let sent = f
            .store
            .list_pending_mailbox_messages_for_agent(&JsString::from("main"))
            .unwrap()
            .into_iter()
            .map(|message| json!({"message":message.body,"threadId":message.thread_id}))
            .collect::<Vec<_>>();
        let actual = match result {
            Ok(mut result) => {
                let delivery = &mut result.details.as_mut().unwrap()["runtimeMailboxDelivery"];
                delivery["controlDbPath"] = json!("<db>");
                delivery["messages"].as_array_mut().unwrap()[0]["storeRef"]["sessionPath"] = json!("<store>");
                json!({"result":{"content":result.content,"details":result.details,"isError":result.is_error},"sent":sent})
            }
            Err(error) => json!({"error":error.message,"sent":sent}),
        };
        assert_eq!(actual, expected, "answer body {body}");
    }
}

#[tokio::test(start_paused = true)]
async fn blocking_child_waits_for_a_later_answer_on_the_500ms_poll() {
    use pi_coding_agent::core::session_control_db::{RuntimeMailboxAddress, register_runtime_mailbox_listener};
    let f = Fixture::new().await;
    register_runtime_mailbox_listener(
        &f.db,
        &RuntimeMailboxAddress {
            agent_id: Some("child".into()),
            session_id: "session-child".into(),
        },
        std::process::id(),
        None,
        Default::default(),
    )
    .unwrap();
    let waiting = tokio::spawn((f.ask.execute)(
        "later".into(),
        json!(questions(true)),
        None,
        None,
        Some(f.context(Some("child"), true).await),
    ));
    tokio::task::yield_now().await;
    assert_eq!(
        f.store
            .list_pending_mailbox_messages_for_agent(&JsString::from("main"))
            .unwrap()
            .len(),
        1
    );
    assert!(!waiting.is_finished(), "child must block before a parent answer exists");
    (f.answer.execute)(
        "reply".into(),
        json!({"agentId":"child","requestId":"later","answers":{"Choose?":"B"}}),
        None,
        None,
        Some(f.context(None, false).await),
    )
    .await
    .unwrap();
    tokio::time::advance(std::time::Duration::from_millis(499)).await;
    tokio::task::yield_now().await;
    assert!(!waiting.is_finished(), "poll must not run early");
    tokio::time::advance(std::time::Duration::from_millis(1)).await;
    let result = waiting.await.unwrap().unwrap();
    assert_eq!(result.details.as_ref().unwrap()["answers"], json!({"Choose?":"B"}));
    assert_eq!(result.details.as_ref().unwrap()["cancelled"], json!(false));
}

#[tokio::test]
async fn inactive_child_unavailable_mailbox_and_failed_parent_transport_are_explicit() {
    let f = Fixture::new().await;
    let mut child = f.store.get_agent(&JsString::from("child")).unwrap();
    child.lifecycle = pi_coding_agent::core::multi_agent_store::AgentLifecycle::Completed;
    f.store.publish_lifecycle_coordinator_snapshot(&child).unwrap();
    let error = (f.answer.execute)(
        "reply".into(),
        json!({"agentId":"child","requestId":"call","answers":{}}),
        None,
        None,
        Some(f.context(None, false).await),
    )
    .await
    .unwrap_err();
    assert_eq!(error.message, "Could not answer child: inactive (completed)");
    child.lifecycle = pi_coding_agent::core::multi_agent_store::AgentLifecycle::Running;
    child.transcript = None;
    f.store.publish_lifecycle_coordinator_snapshot(&child).unwrap();
    let error = (f.answer.execute)(
        "reply".into(),
        json!({"agentId":"child","requestId":"call","answers":{}}),
        None,
        None,
        Some(f.context(None, false).await),
    )
    .await
    .unwrap_err();
    assert_eq!(error.message, "Could not answer child: runtime mailbox is unavailable");
    f.store.set_persistence_session_manager(None);
    let error = (f.ask.execute)(
        "call".into(),
        json!(questions(false)),
        None,
        None,
        Some(f.context(Some("child"), true).await),
    )
    .await
    .unwrap_err();
    assert_eq!(
        error.message,
        "Could not send questions to parent: Could not contact parent: runtime mailbox transport is unavailable."
    );
}
