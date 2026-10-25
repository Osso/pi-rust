//! Real child-process boundaries for the frozen enrich queue and shutdown hooks.
use pi_coding_agent::core::extensions::{
    ExtensionRunner, create_extension_runtime,
    loader::{ExtensionFactory, load_extension_from_factory},
    types::*,
};
use pi_coding_agent::core::{
    event_bus::{EventBus, EventBusController},
    model_registry::ModelRegistry,
    session_manager::SessionManager,
};
use pi_js_compat::{js_json::JsString, process_env};
use std::{
    fs,
    sync::{Arc, Mutex},
    time::Duration,
};

#[tokio::test]
async fn queues_enrichment_and_shutdown_waits_for_graceful_close() {
    let temp = tempfile::tempdir().unwrap();
    let script = temp.path().join("enrich.py");
    let started = temp.path().join("started");
    let release = temp.path().join("release");
    let terminated = temp.path().join("terminated");
    let body = format!(
        r#"#!/usr/bin/python3
import json, pathlib, signal, sys, time
root = pathlib.Path({root:?})
def terminate(*args):
    time.sleep(0.15)
    (root / 'terminated').write_text('closed')
    sys.exit(0)
signal.signal(signal.SIGTERM, terminate)
prompt = json.loads(sys.stdin.readline())['prompt']
with (root / 'started').open('a') as log:
    log.write(prompt + '\n')
while not (root / 'release').exists():
    time.sleep(0.01)
print(json.dumps({{'hookSpecificOutput': {{'additionalContext': prompt}}}}))
"#,
        root = temp.path().to_str().unwrap()
    );
    executable_script::write_executable_script(&script, &body);
    process_env::set("PI_CLAUDE_MEMORY", script.to_str().unwrap());
    let runtime = create_extension_runtime();
    let factory: ExtensionFactory = Arc::new(|api| {
        let result = pi_coding_agent::bundled_extensions::claude_memory_enrich::claude_memory_enrich_extension(api);
        Box::pin(async move { result })
    });
    let bus: Arc<dyn EventBus> = Arc::new(EventBusController::new());
    let extension = load_extension_from_factory(
        factory,
        &JsString::from("/tmp"),
        bus,
        runtime.clone(),
        &JsString::from("<test>"),
    )
    .await
    .unwrap();
    let sessions = Arc::new(Mutex::new(
        SessionManager::in_memory(Some(&JsString::from("/tmp")), None).unwrap(),
    ));
    let models = Arc::new(
        ModelRegistry::in_memory(pi_coding_agent::core::auth_storage::AuthStorage::in_memory(
            serde_json::Map::new(),
        ))
        .unwrap(),
    );
    let runner = ExtensionRunner::new(vec![extension.clone()], runtime, "/tmp", sessions, models, None);
    let before = extension.read().handlers.before_agent_start[0].clone();
    let first = before.call(event("first"), runner.create_context());
    let second = before.call(event("second"), runner.create_context());
    let flow = async {
        wait_for_file(&started).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(
            fs::read_to_string(&started).unwrap(),
            "first\n",
            "only one child may start at a time"
        );
        fs::write(&release, "go").unwrap();
    };
    let (first, second, ()) = tokio::join!(first, second, flow);
    assert!(first.unwrap().unwrap().system_prompt.unwrap().contains("first"));
    assert!(second.unwrap().unwrap().system_prompt.unwrap().contains("second"));
    assert_eq!(fs::read_to_string(&started).unwrap(), "first\nsecond\n");
    fs::remove_file(&release).unwrap();
    fs::remove_file(&started).unwrap();
    let shutdown = extension
        .read()
        .handlers
        .session_shutdown
        .first()
        .cloned()
        .expect("frozen shutdown hook");
    let active = before.call(event("active"), runner.create_context());
    let shutdown_flow = async {
        wait_for_file(&started).await;
        shutdown
            .call(
                SessionShutdownEvent {
                    reason: SessionShutdownReason::Quit,
                    target_session_file: None,
                },
                runner.create_context(),
            )
            .await
            .unwrap();
        assert_eq!(
            fs::read_to_string(&terminated).unwrap(),
            "closed",
            "shutdown waits for SIGTERM completion"
        );
    };
    let (result, ()) = tokio::join!(active, shutdown_flow);
    assert!(result.unwrap().is_none(), "shutdown does not inject partial context");
    assert!(
        before
            .call(event("after-shutdown"), runner.create_context())
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(fs::read_to_string(&started).unwrap(), "active\n");
    process_env::clear();
}

fn event(prompt: &str) -> BeforeAgentStartEvent {
    BeforeAgentStartEvent {
        prompt: prompt.into(),
        images: None,
        system_prompt: "BASE".into(),
        system_prompt_options: Default::default(),
    }
}

async fn wait_for_file(path: &std::path::Path) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !path.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("child started");
}

#[path = "support/executable_script.rs"]
mod executable_script;
