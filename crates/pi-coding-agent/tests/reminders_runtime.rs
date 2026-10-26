use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::shared_bundled::RecordingUi;
use crate::shared_extensions as support;
use pi_coding_agent::core::extensions::types::{
    ExtensionMode, SessionShutdownEvent, SessionShutdownReason, SessionStartEvent, SessionStartReason,
};
use pi_coding_agent::core::extensions::{ExtensionRunner, create_extension_runtime, load_extension_from_factory};
use pi_coding_agent::core::resource_loader::InlineExtension;
use pi_coding_agent::core::session_manager::{NewSessionOptions, SessionManager};
use pi_coding_agent::core::settings_manager::SettingsManager;
use pi_coding_agent::main_runner::create_first_party_extension_factories;
use pi_js_compat::js_json::JsString;

const ID: &str = "00000000-0000-4000-8000-000000000001";

#[tokio::test(start_paused = true)]
async fn reminders_runtime() {
    if std::env::var_os("PI_REMINDERS_RUNTIME_CHILD").is_none() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("notify-send");
        std::fs::write(
            &script,
            "#!/bin/sh\nprintf '%s\\n' \"$@\" >> \"$PI_REMINDERS_NOTIFY_LOG\"\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let path = format!("{}:{}", dir.path().display(), std::env::var("PATH").unwrap_or_default());
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "reminders_runtime::reminders_runtime", "--nocapture"])
            .env("PI_REMINDERS_RUNTIME_CHILD", "1")
            .env("PI_REMINDERS_NOTIFY_LOG", dir.path().join("notifications"))
            .env("PATH", path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("notifications")).unwrap(),
            "--app-name=pi\nPi reminder\n#1 Check garden\n"
        );
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_str().unwrap();
    let path: JsString = dir.path().join("control.db").to_str().unwrap().into();
    let factories =
        create_first_party_extension_factories(None, pi_ai::http::default_fetch(), cwd.into(), Arc::new(Vec::new));
    let factory = factories
        .into_iter()
        .find_map(|entry| match entry {
            InlineExtension::Factory {
                extension_path: Some(path),
                factory,
            } if path == "<first-party:reminders>" => Some(factory),
            _ => None,
        })
        .expect("first-party reminders factory");
    let runtime = create_extension_runtime();
    let extension = load_extension_from_factory(
        factory,
        &cwd.into(),
        support::event_bus(),
        runtime.clone(),
        &"<first-party:reminders>".into(),
    )
    .await
    .unwrap();
    let tool = extension.read().tools.get("reminders").unwrap().definition.clone();
    let command = extension
        .read()
        .commands
        .get(&JsString::from("reminders"))
        .unwrap()
        .handler
        .clone();
    let handlers = extension.read().handlers.clone();
    let settings = pi_js_compat::js_json!({"reminders":{"deliverySessionId":ID}})
        .as_object()
        .unwrap()
        .clone();
    let settings = Arc::new(Mutex::new(SettingsManager::in_memory(settings, Default::default())));
    let manager = Arc::new(Mutex::new(
        SessionManager::in_memory(
            Some(&cwd.into()),
            Some(NewSessionOptions {
                id: Some(ID.into()),
                ..Default::default()
            }),
        )
        .unwrap(),
    ));
    let mut runner = ExtensionRunner::new(
        vec![extension],
        runtime,
        cwd,
        manager.clone(),
        support::model_registry().await,
        Some(settings),
    );
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut actions = support::actions();
    let recorded = sent.clone();
    actions.send_message = Arc::new(move |message, options| {
        recorded.lock().unwrap().push((message, options));
        Ok(())
    });
    let mut context_actions = support::context_actions();
    let db = path.clone();
    context_actions.get_control_db_path = Some(Arc::new(move || Some(db.clone())));
    runner.bind_core(actions, context_actions, None).unwrap();
    let ui = Arc::new(RecordingUi::default());
    runner.set_ui_context(Some(ui.clone()), ExtensionMode::Rpc);
    runner.bind_command_context(None);
    let result = (tool.execute)(
        "add".into(),
        pi_js_compat::js_json!({"action":"add","title":"Check garden","dueAt":"2020-01-01T00:00Z"}),
        None,
        None,
        Some(runner.create_context()),
    )
    .await
    .unwrap();
    let pi_ai::types::TextOrImageContent::Text(text) = &result.content[0] else {
        panic!("expected text")
    };
    assert!(text.text.to_utf8().unwrap().starts_with("Added reminder #1"));
    command("list all".into(), runner.create_command_context())
        .await
        .unwrap();
    assert!(
        ui.last_notification()
            .unwrap()
            .0
            .to_utf8()
            .unwrap()
            .contains("Check garden")
    );
    manager
        .lock()
        .unwrap()
        .new_session(Some(NewSessionOptions {
            id: Some("00000000-0000-4000-8000-000000000002".into()),
            ..Default::default()
        }))
        .unwrap();
    handlers.session_start[0]
        .call(
            SessionStartEvent {
                reason: SessionStartReason::Startup,
                previous_session_file: None,
            },
            runner.create_context(),
        )
        .await
        .unwrap();
    tokio::time::advance(Duration::from_secs(120)).await;
    tokio::task::yield_now().await;
    assert!(sent.lock().unwrap().is_empty());
    manager
        .lock()
        .unwrap()
        .new_session(Some(NewSessionOptions {
            id: Some(ID.into()),
            ..Default::default()
        }))
        .unwrap();
    handlers.session_start[0]
        .call(
            SessionStartEvent {
                reason: SessionStartReason::Startup,
                previous_session_file: None,
            },
            runner.create_context(),
        )
        .await
        .unwrap();
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_secs(59)).await;
    tokio::task::yield_now().await;
    assert!(sent.lock().unwrap().is_empty());
    tokio::time::advance(Duration::from_secs(1)).await;
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }
    {
        let messages = sent.lock().unwrap();
        assert_eq!(messages.len(), 1);
        let (message, options) = &messages[0];
        assert_eq!(message.custom_type, "reminder");
        assert!(message.display);
        assert_eq!(message.details.as_ref().unwrap()["id"], pi_js_compat::js_json!(1));
        assert!(
            !message.details.as_ref().unwrap()["deliveryToken"]
                .as_string()
                .unwrap()
                .is_empty()
        );
        let pi_coding_agent::core::extensions::types::CustomMessageContent::Text(text) = &message.content else {
            panic!("expected reminder text")
        };
        assert!(text.to_utf8().unwrap().contains("context, not authorization"));
        let options = options.as_ref().unwrap();
        assert_eq!(
            options.deliver_as,
            Some(pi_coding_agent::core::extensions::types::MessageDelivery::FollowUp)
        );
        assert_eq!(options.trigger_turn, Some(true));
    }
    handlers.session_shutdown[0]
        .call(
            SessionShutdownEvent {
                reason: SessionShutdownReason::Quit,
                target_session_file: None,
            },
            runner.create_context(),
        )
        .await
        .unwrap();
    for _ in 0..100 {
        if std::fs::read_to_string(std::env::var("PI_REMINDERS_NOTIFY_LOG").unwrap())
            .is_ok_and(|s| s.contains("#1 Check garden\n"))
        {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("notify-send did not write its arguments");
}
