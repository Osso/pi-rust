//! Source-backed RED-intent fixtures. Unexecuted until the coding library compiles.
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/extensions.rs"]
mod support;

use pi_coding_agent::bundled_extensions::agents_core::{
    registration::register_agents_core_tools,
    runtime::{MultiAgentExtensionOptions, create_multi_agent_pi_request_handler},
    shared::{ChildAgentSession, ChildAgentSessionFactory},
};
use pi_coding_agent::core::extensions::{ExtensionApi, ExtensionRunner, load_extension_from_factory};
use pi_coding_agent::core::multi_agent_store::{AgentTranscriptMetadata, MultiAgentStore, MultiAgentStoreOptions};
use pi_coding_agent::core::session_manager::{SessionManager, lock_session_manager};
use pi_coding_agent::core::settings_manager::{SettingsManager, SettingsManagerCreateOptions};
use pi_js_compat::js_json::{self, JsString};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

fn frozen_error(kind: &str) -> serde_json::Value {
    let mut source = oracle::TsOracle::spawn(
        "agents-core-drift-oracle.ts",
        "packages/coding-agent/extensions/agents-core/src/runtime.ts",
    );
    let response = source.request(&serde_json::json!({"kind":kind}));
    assert_eq!(response["ok"], false);
    response
}

#[tokio::test]
async fn public_spawn_propagates_the_accessed_malformed_model_error_before_child_creation() {
    let expected = frozen_error("profile");
    assert_eq!(expected["name"], "TypeError");
    assert_eq!(expected["message"], "modelReference.trim is not a function");
    let directory = tempfile::tempdir().unwrap();
    let cwd: JsString = directory.path().to_str().unwrap().into();
    let session_dir: JsString = directory.path().join("sessions").to_str().unwrap().into();
    let manager = Arc::new(Mutex::new(
        SessionManager::create(&cwd, Some(&session_dir), None).unwrap(),
    ));
    let control_db_path: JsString = directory.path().join("control.sqlite").to_str().unwrap().into();
    lock_session_manager(&manager)
        .set_metadata_control_db_path(Some(&control_db_path), Default::default())
        .unwrap();
    let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default()));
    store.set_persistence_session_manager(Some(&*lock_session_manager(&manager)));
    let configured = js_json::parse_lossless(r#"{"agents":{"bad":{"model":1}}}"#)
        .unwrap()
        .as_object()
        .unwrap()
        .clone();
    let settings = SettingsManager::in_memory(configured, SettingsManagerCreateOptions::default());
    let mut runner = ExtensionRunner::new(
        Vec::new(),
        support::runtime(),
        &cwd,
        manager,
        support::model_registry().await,
        Some(Arc::new(Mutex::new(settings))),
    );
    support::bind(&mut runner);
    let called = Arc::new(AtomicBool::new(false));
    let factory_called = called.clone();
    let options = MultiAgentExtensionOptions {
        store: Some(store),
        create_child_session: Some(ChildAgentSessionFactory::new(move |_| {
            factory_called.store(true, Ordering::SeqCst);
            Box::pin(async { Err("child creation must not be reached".into()) })
        })),
        ..Default::default()
    };
    let handler = create_multi_agent_pi_request_handler(&options, None);
    let error = handler
        .handle(
            &pi_js_compat::js_json!({"method":"agents.spawn","params":{"agentType":"bad","prompt":"inspect fixture"}}),
            &runner.create_context(),
            None,
            None,
        )
        .await
        .unwrap_err();
    assert_eq!(error.message, JsString::from(expected["message"].as_str().unwrap()));
    assert!(!called.load(Ordering::SeqCst));
}

#[tokio::test]
async fn public_registration_propagates_late_tool_failure_instead_of_returning_success() {
    let expected = frozen_error("registration");
    let runtime = support::runtime();
    let captured = Arc::new(Mutex::new(None::<ExtensionApi>));
    let captured_api = captured.clone();
    let _extension = load_extension_from_factory(
        Arc::new(move |pi| {
            *captured_api.lock().unwrap() = Some(pi.clone());
            Box::pin(async { Ok(()) })
        }),
        &JsString::from("/fixture"),
        support::event_bus(),
        runtime.clone(),
        &JsString::from("<drift-registration>"),
    )
    .await
    .unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let callback_calls = calls.clone();
    let callback_runtime = runtime.clone();
    let mut actions = runtime.actions();
    actions.refresh_tools = Arc::new(move || {
        if callback_calls.fetch_add(1, Ordering::SeqCst) == 1 {
            callback_runtime.invalidate(Some("late registration failed"));
        }
    });
    runtime.set_actions(actions);
    let api = captured.lock().unwrap().take().unwrap();
    let error = register_agents_core_tools(&api, &MultiAgentExtensionOptions::default()).unwrap_err();
    assert_eq!(error.message, JsString::from(expected["message"].as_str().unwrap()));
}

struct NoDispatchSession;

impl ChildAgentSession for NoDispatchSession {
    fn messages(&self) -> Vec<pi_agent_core::AgentMessage> {
        Vec::new()
    }
    fn prompt(&self, _text: JsString) -> pi_ai::request::BoxFuture<'static, Result<(), JsString>> {
        Box::pin(async { Err("dispatch must not be reached".into()) })
    }
    fn transcript(&self) -> Option<AgentTranscriptMetadata> {
        None
    }
}

#[tokio::test]
async fn public_spawn_does_not_commit_an_agent_after_the_session_getter_fails() {
    use pi_coding_agent::core::session_control_db::read_multi_agent_state;
    let expected = frozen_error("sessionGetter");
    assert_eq!(expected["constructed"], true);
    assert_eq!(expected["agents"], 0);
    let directory = tempfile::tempdir().unwrap();
    let cwd: JsString = directory.path().to_str().unwrap().into();
    let session_dir: JsString = directory.path().join("sessions").to_str().unwrap().into();
    let manager = Arc::new(Mutex::new(
        SessionManager::create(&cwd, Some(&session_dir), None).unwrap(),
    ));
    let control_db_path: JsString = directory.path().join("control.sqlite").to_str().unwrap().into();
    lock_session_manager(&manager)
        .set_metadata_control_db_path(Some(&control_db_path), Default::default())
        .unwrap();
    let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default()));
    store.set_persistence_session_manager(Some(&*lock_session_manager(&manager)));
    let persistence = store.get_persistence_target().unwrap();
    let mut runner = ExtensionRunner::new(
        Vec::new(),
        support::runtime(),
        &cwd,
        manager,
        support::model_registry().await,
        None,
    );
    support::bind(&mut runner);
    let runner = Arc::new(runner);
    let callback_runner = runner.clone();
    let options = MultiAgentExtensionOptions {
        store: Some(store),
        create_child_session: Some(ChildAgentSessionFactory::new(move |_| {
            callback_runner.invalidate(Some("session getter failed after child construction"));
            Box::pin(async { Ok(Arc::new(NoDispatchSession) as Arc<dyn ChildAgentSession>) })
        })),
        ..Default::default()
    };
    let handler = create_multi_agent_pi_request_handler(&options, None);
    let result = handler
        .handle(
            &pi_js_compat::js_json!({"method":"agents.spawn","params":{"prompt":"inspect fixture"}}),
            &runner.create_context(),
            None,
            None,
        )
        .await;
    let admitted = read_multi_agent_state(&persistence.control_db_path, &persistence.session_path)
        .unwrap()
        .map_or(0, |state| state.agents.len());
    assert_eq!(
        admitted,
        expected["agents"].as_u64().unwrap() as usize,
        "failed session lookup must not admit a child"
    );
    assert_eq!(
        result.unwrap_err().message,
        JsString::from(expected["message"].as_str().unwrap())
    );
}
