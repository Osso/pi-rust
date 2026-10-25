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
    let cwd: JsString = directory.path().to_str().expect("utf-8 test path").into();
    let session_dir: JsString = directory
        .path()
        .join("sessions")
        .to_str()
        .expect("utf-8 test path")
        .into();
    let manager = Arc::new(Mutex::new(
        SessionManager::create(&cwd, Some(&session_dir), None).unwrap(),
    ));
    let control_db_path: JsString = directory
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .into();
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
    let runtime = support::runtime();
    let mut runner = ExtensionRunner::new(
        Vec::new(),
        runtime,
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
    let handler = create_multi_agent_pi_request_handler(&options);
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
async fn public_spawn_preserves_profile_context_when_unrelated_tools_are_malformed() {
    use pi_coding_agent::bundled_extensions::agents_core::shared::SpawnContext;
    let mut source = oracle::TsOracle::spawn(
        "agents-core-drift-oracle.ts",
        "packages/coding-agent/extensions/agents-core/src/runtime.ts",
    );
    let expected = source.request(&serde_json::json!({"kind":"profileContext"}));
    assert_eq!(expected["context"], "fresh");
    assert_eq!(expected["dispatched"], false);
    let directory = tempfile::tempdir().unwrap();
    let cwd: JsString = directory.path().to_str().expect("utf-8 test path").into();
    let session_dir: JsString = directory
        .path()
        .join("sessions")
        .to_str()
        .expect("utf-8 test path")
        .into();
    let manager = Arc::new(Mutex::new(
        SessionManager::create(&cwd, Some(&session_dir), None).unwrap(),
    ));
    let db: JsString = directory
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .into();
    lock_session_manager(&manager)
        .set_metadata_control_db_path(Some(&db), Default::default())
        .unwrap();
    let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default()));
    store.set_persistence_session_manager(Some(&*lock_session_manager(&manager)));
    let configured = js_json::parse_lossless(r#"{"agents":{"bad":{"context":"fresh","tools":7}}}"#)
        .unwrap()
        .as_object()
        .unwrap()
        .clone();
    let settings = SettingsManager::in_memory(configured, SettingsManagerCreateOptions::default());
    let runtime = support::runtime();
    let mut runner = ExtensionRunner::new(
        Vec::new(),
        runtime,
        &cwd,
        manager,
        support::model_registry().await,
        Some(Arc::new(Mutex::new(settings))),
    );
    support::bind(&mut runner);
    let observed = Arc::new(Mutex::new(None));
    let observed_context = observed.clone();
    let options = MultiAgentExtensionOptions {
        store: Some(store),
        create_child_session: Some(ChildAgentSessionFactory::new(move |request| {
            *observed_context.lock().unwrap() = Some(request.context);
            Box::pin(async { Err("factory observed profile context".into()) })
        })),
        ..Default::default()
    };
    let handler = create_multi_agent_pi_request_handler(&options);
    let result = handler
        .handle(
            &pi_js_compat::js_json!({"method":"agents.spawn","params":{"agentType":"bad","prompt":"inspect fixture"}}),
            &runner.create_context(),
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(*observed.lock().unwrap(), Some(SpawnContext::Fresh));
    assert_eq!(result["dispatched"], pi_js_compat::js_json!(false));
    assert_eq!(
        result["agent"]["lifecycle"],
        pi_js_compat::js_json!(expected["lifecycle"].as_str().unwrap())
    );
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
        Ok(())
    });
    runtime.set_actions(actions);
    let api = captured.lock().unwrap().take().unwrap();
    let error = register_agents_core_tools(&api, &MultiAgentExtensionOptions::default()).unwrap_err();
    assert_eq!(error.message, JsString::from(expected["message"].as_str().unwrap()));
}

struct NoDispatchSession;

impl ChildAgentSession for NoDispatchSession {
    fn messages(&self) -> Vec<Arc<pi_agent_core::AgentMessage>> {
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
    let cwd: JsString = directory.path().to_str().expect("utf-8 test path").into();
    let session_dir: JsString = directory
        .path()
        .join("sessions")
        .to_str()
        .expect("utf-8 test path")
        .into();
    let manager = Arc::new(Mutex::new(
        SessionManager::create(&cwd, Some(&session_dir), None).unwrap(),
    ));
    let control_db_path: JsString = directory
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .into();
    lock_session_manager(&manager)
        .set_metadata_control_db_path(Some(&control_db_path), Default::default())
        .unwrap();
    let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default()));
    store.set_persistence_session_manager(Some(&*lock_session_manager(&manager)));
    let persistence = store.get_persistence_target().unwrap();
    let runtime = support::runtime();
    let mut runner = ExtensionRunner::new(
        Vec::new(),
        runtime,
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
    let handler = create_multi_agent_pi_request_handler(&options);
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

#[test]
fn public_background_completion_uses_live_ui_and_reports_async_getter_failure() {
    let mut source = oracle::TsOracle::spawn(
        "agents-core-drift-oracle.ts",
        "packages/coding-agent/extensions/agents-core/src/runtime.ts",
    );
    for invalidated in [false, true] {
        let expected = source.request(&serde_json::json!({"kind":"uiCallback","invalidated":invalidated}));
        assert_eq!(expected["before"], 1);
        assert_eq!(expected["after"], if invalidated { 1 } else { 2 });
        assert_eq!(expected["lifecycle"], "completed");
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .env(
                "PI_TEST_BACKGROUND_UI",
                if invalidated { "invalidated" } else { "active" },
            )
            .args([
                "--ignored",
                "--exact",
                "background_ui_lifetime_child",
                "--test-threads=1",
                "--nocapture",
            ])
            .output()
            .unwrap();
        if invalidated {
            assert_eq!(
                output.status.code(),
                Some(1),
                "async UI failure must reach the process uncaught handler"
            );
            let failure = expected["failure"].as_str().unwrap();
            let expected_diagnostic = serde_json::json!(format!(
                "MultiAgentStore lifecycle notification listener failed: {failure}"
            ));
            assert!(
                expected["diagnostics"]
                    .as_array()
                    .unwrap()
                    .contains(&expected_diagnostic)
            );
            // The actual frozen ExtensionRunner also reports a stale mirror observer
            // (oracle diagnostics; agents-core-ui-fidelity-red.log). Preserve it;
            // compare the terminal uncaught line, not unrelated observer formatting.
            let stderr = String::from_utf8_lossy(&output.stderr);
            let uncaught = format!("Error: {failure}");
            assert_eq!(stderr.lines().last(), Some(uncaught.as_str()));
        } else {
            assert!(expected["diagnostics"].as_array().unwrap().is_empty());
            assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        }
    }
}

struct ControlledCompletionSession {
    started: tokio::sync::Notify,
    complete: Arc<tokio::sync::Notify>,
}

impl ChildAgentSession for ControlledCompletionSession {
    fn messages(&self) -> Vec<Arc<pi_agent_core::AgentMessage>> {
        Vec::new()
    }
    fn transcript(&self) -> Option<AgentTranscriptMetadata> {
        None
    }
    fn prompt(&self, _text: JsString) -> pi_ai::request::BoxFuture<'static, Result<(), JsString>> {
        self.started.notify_one();
        let complete = self.complete.clone();
        Box::pin(async move {
            complete.notified().await;
            Ok(())
        })
    }
}

#[derive(Default)]
struct BackgroundUi {
    notifications: Mutex<Vec<JsString>>,
    terminal_notification: tokio::sync::Notify,
}

impl pi_coding_agent::core::extensions::ExtensionUiContext for BackgroundUi {
    fn notify(
        &self,
        message: &JsString,
        _level: Option<pi_coding_agent::core::extensions::NotifyType>,
    ) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        let mut notifications = self.notifications.lock().unwrap();
        notifications.push(message.clone());
        if notifications.len() > 1 {
            self.terminal_notification.notify_one();
        }
        Ok(())
    }
}

struct NoDesktopNotifier;
impl pi_coding_agent::core::desktop_notification::DesktopNotifier for NoDesktopNotifier {
    fn notify(
        &self,
        _notification: &pi_coding_agent::core::desktop_notification::DesktopNotification,
    ) -> Result<
        Option<Box<dyn pi_coding_agent::core::desktop_notification::DesktopNotificationHandle>>,
        pi_js_compat::callback_error::CallbackError,
    > {
        Ok(None)
    }
}

#[tokio::test]
#[ignore = "subprocess fixture only"]
async fn background_ui_lifetime_child() {
    use pi_coding_agent::core::extensions::ExtensionMode;
    let Some(mode) = std::env::var_os("PI_TEST_BACKGROUND_UI") else {
        return;
    };
    pi_coding_agent::utils::uncaught::install_uncaught_handler();
    let directory = tempfile::tempdir().unwrap();
    let cwd: JsString = directory.path().to_str().expect("utf-8 test path").into();
    let session_dir: JsString = directory
        .path()
        .join("sessions")
        .to_str()
        .expect("utf-8 test path")
        .into();
    let manager = Arc::new(Mutex::new(
        SessionManager::create(&cwd, Some(&session_dir), None).unwrap(),
    ));
    let db: JsString = directory
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .into();
    lock_session_manager(&manager)
        .set_metadata_control_db_path(Some(&db), Default::default())
        .unwrap();
    let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default()));
    store.set_persistence_session_manager(Some(&*lock_session_manager(&manager)));
    let session = Arc::new(ControlledCompletionSession {
        started: Default::default(),
        complete: Arc::new(Default::default()),
    });
    let factory_session = session.clone();
    let options = MultiAgentExtensionOptions {
        store: Some(store.clone()),
        desktop_notifier: Some(Arc::new(NoDesktopNotifier)),
        create_child_session: Some(ChildAgentSessionFactory::new(move |_| {
            let session = factory_session.clone();
            Box::pin(async move { Ok(session as Arc<dyn ChildAgentSession>) })
        })),
        ..Default::default()
    };
    let runtime = support::runtime();
    let extension = load_extension_from_factory(
        Arc::new(move |pi| {
            let result = register_agents_core_tools(pi, &options);
            Box::pin(async move { result })
        }),
        &cwd,
        support::event_bus(),
        runtime.clone(),
        &JsString::from("<background-ui-drift>"),
    )
    .await
    .unwrap();
    let mut runner = ExtensionRunner::new(
        vec![extension],
        runtime,
        &cwd,
        manager,
        support::model_registry().await,
        None,
    );
    support::bind(&mut runner);
    let ui = Arc::new(BackgroundUi::default());
    runner.set_ui_context(Some(ui.clone()), ExtensionMode::Rpc);
    let command = runner.get_command("bg").unwrap().handler;
    command("inspect fixture".into(), runner.create_command_context())
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(3), session.started.notified())
        .await
        .unwrap();
    assert_eq!(ui.notifications.lock().unwrap().len(), 1);
    if mode == "invalidated" {
        runner.invalidate(Some("ui getter failed after dispatch started"));
    }
    session.complete.notify_one();
    tokio::time::timeout(std::time::Duration::from_secs(3), ui.terminal_notification.notified())
        .await
        .unwrap();
    assert_eq!(ui.notifications.lock().unwrap().len(), 2);
    assert_eq!(
        store.list_agents()[0].lifecycle,
        pi_coding_agent::core::multi_agent_store::AgentLifecycle::Completed
    );
}
