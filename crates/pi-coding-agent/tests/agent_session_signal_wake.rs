//! SIGUSR2 runtime-mailbox wake at the real OS boundary
//! (`core/agent-session.ts` `_startRuntimeMailboxSignalWake`).
//!
//! Kept in its own test binary: the signal handler is process-global, so a
//! second session in the same process would receive the wake too.

use pi_js_compat::js_json::JsString;
use std::sync::Arc;
use std::time::{Duration, Instant};

use pi_agent_core::agent::types::AgentMessage;
use pi_ai::providers::faux::{
    FauxAssistantMessageOptions, FauxModelDefinition, RegisterFauxProviderOptions, faux_assistant_message,
    faux_provider,
};
use pi_coding_agent::core::agent_session::ExtensionBindings;
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::model_registry::ModelRegistry;
use pi_coding_agent::core::resource_loader::StaticResourceLoader;
use pi_coding_agent::core::sdk::{CreateAgentSessionOptions, create_agent_session};
use pi_coding_agent::core::session_control_db::{
    EnqueueRuntimeMailboxMessageInput, RuntimeMailboxAddress, RuntimeMailboxMessageKind, RuntimeMailboxStoreRef,
    enqueue_runtime_mailbox_message, get_control_db_path, upsert_multi_agent_mailbox_message,
};
use pi_coding_agent::core::session_manager::{SessionManager, lock_session_manager};
use serde_json::json;

#[tokio::test]
async fn sigusr2_wakes_the_idle_session_without_the_poll_interval() {
    let dir = tempfile::tempdir().expect("temp dir");
    let cwd = dir.path().join("repo").display().to_string();
    std::fs::create_dir_all(&cwd).expect("cwd");
    let agent_dir = dir.path().join("agent").display().to_string();
    std::fs::create_dir_all(&agent_dir).expect("agent dir");
    let session_dir = dir.path().join("sessions").display().to_string();
    let control_db = get_control_db_path(pi_js_compat::node_fs::decode_node_path((dir.path()).as_os_str()))
        .to_utf8()
        .expect("fixture control path is scalar");

    let faux = faux_provider(RegisterFauxProviderOptions {
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
    });
    let mut data = serde_json::Map::new();
    data.insert("faux".to_owned(), json!({"type": "api_key", "key": "faux-key"}));
    let runtime = ModelRegistry::in_memory(AuthStorage::in_memory(data)).unwrap();
    scripted_registry::register_scripted_provider(&runtime, Arc::clone(&faux.provider));
    runtime.refresh().unwrap();
    let model = faux.get_model(None).expect("model");
    let mut manager = SessionManager::create(&JsString::from(&cwd), Some(&JsString::from(&session_dir)), None)
        .expect("session manager");
    manager
        .set_metadata_control_db_path(Some(&JsString::from(&control_db)), Default::default())
        .expect("metadata control db");
    let result = create_agent_session(CreateAgentSessionOptions {
        cwd: Some(JsString::from(cwd.clone())),
        agent_dir: Some(JsString::from(agent_dir)),
        model_registry: Some(runtime),
        model: Some(model),
        resource_loader: Some(StaticResourceLoader::new()),
        session_manager: Some(manager),
        control_db_path: Some(JsString::from(control_db.clone())),
        ..Default::default()
    })
    .await
    .expect("session");
    result.session.set_active_tools_by_name(&[]).unwrap();
    let session = result.session;
    let session_id = lock_session_manager(&session.session_manager_handle())
        .get_session_id()
        .to_owned();
    let session_path = lock_session_manager(&session.session_manager_handle())
        .get_session_file()
        .expect("session file")
        .to_owned();

    session
        .bind_extensions(ExtensionBindings {
            control_db_path: Some(JsString::from(control_db.clone())),
            ..Default::default()
        })
        .await
        .expect("bind extensions");
    faux.set_responses(vec![
        faux_assistant_message("signal reply", FauxAssistantMessageOptions::default()).into(),
    ]);
    let message_id = "runtime_test_signal";
    upsert_multi_agent_mailbox_message(
        &control_db,
        &session_path,
        message_id,
        &pi_js_compat::js_json!({
            "body": "Need parent review",
            "fromAgentId": "agent_1",
            "id": message_id,
            "kind": "message",
            "status": "pending",
            "toAgentId": "main",
        }),
    )
    .expect("upsert mailbox message");
    enqueue_runtime_mailbox_message(
        &control_db,
        &EnqueueRuntimeMailboxMessageInput {
            recipient_host: None,
            kind: RuntimeMailboxMessageKind::Message,
            recipient: RuntimeMailboxAddress {
                agent_id: None,
                session_id: session_id.clone(),
            },
            sender: RuntimeMailboxAddress {
                agent_id: Some(JsString::from("agent_1")),
                session_id: JsString::from("child-session"),
            },
            store_ref: RuntimeMailboxStoreRef {
                message_id: JsString::from(message_id.to_owned()),
                session_path,
            },
        },
    )
    .expect("enqueue runtime message");

    // The 30s poll timer cannot fire inside this window; only the SIGUSR2 wake
    // installed by the session can deliver the message.
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && user_texts(&session).is_empty() {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let texts = user_texts(&session);
    assert_eq!(texts.len(), 1, "signal wake delivered {} prompts", texts.len());
    assert!(texts[0].contains("Need parent review"));
}

fn user_texts(session: &pi_coding_agent::core::agent_session::AgentSession) -> Vec<JsString> {
    session
        .messages()
        .iter()
        .filter_map(|message| match message {
            AgentMessage::User(user) => match &user.content {
                pi_ai::types::UserContent::Text(text) => Some(text.clone()),
                pi_ai::types::UserContent::Blocks(blocks) => Some(JsString::join(
                    blocks.iter().filter_map(|block| match block {
                        pi_ai::types::TextOrImageContent::Text(text) => Some(&text.text),
                        pi_ai::types::TextOrImageContent::Image(_) => None,
                    }),
                    "\n",
                )),
            },
            _ => None,
        })
        .collect()
}

use crate::shared_scripted_registry as scripted_registry;

// Shared support is compiled once per integration target.
#[path = "support/scripted_registry.rs"]
pub mod shared_scripted_registry;
