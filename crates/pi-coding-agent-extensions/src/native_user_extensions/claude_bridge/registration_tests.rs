use super::*;
use crate::core::extensions::ExtensionRegistry;
use crate::core::resource_loader::{DefaultResourceLoader, DefaultResourceLoaderOptions, ResourceLoader};
use crate::core::settings_manager::{SettingsManager, SettingsManagerCreateOptions};
use pi_js_compat::js_json::{self, JsString};
use std::sync::{Arc, Mutex};

fn register(registry: &mut ExtensionRegistry) {
    register_with_ownership(registry, Arc::new(ownership::Ownership::default()));
}

#[tokio::test]
async fn claude_bridge_usage_events_are_claimed_without_overwriting_an_earlier_claim() {
    use crate::core::event_bus::{EventBus, EventBusController, EventRequest};
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(".pi")).unwrap();
    std::fs::write(
        dir.path().join(".pi/claude-bridge.json"),
        r#"{"provider":{"accountProfiles":[]}}"#,
    )
    .unwrap();
    let mut registry = ExtensionRegistry::new();
    register(&mut registry);
    let bus = Arc::new(EventBusController::new());
    let loaded = registry
        .load_extensions(
            &["<native:pi-claude-bridge>".into()],
            &JsString::from(dir.path().to_str().unwrap()),
            Some(bus.clone()),
            None,
        )
        .await
        .unwrap();
    assert!(loaded.errors.is_empty());
    let mut all = EventRequest::new(JsValue::Null);
    assert!(
        bus.emit_request(&"claude-bridge:usage-all-request".into(), &mut all)
            .is_empty()
    );
    let response = all.response.expect("usage-all handler claims request").await.unwrap();
    assert_eq!(response, JsValue::Array(Vec::new()));
    let mut single = EventRequest::new(JsValue::object([("args", JsValue::String("reset".into()))]));
    bus.emit_request(&"claude-bridge:usage-request".into(), &mut single);
    assert!(
        single.response.is_some(),
        "single usage request is claimed synchronously"
    );
    let mut claimed = EventRequest::new(JsValue::Null);
    claimed.response = Some(Box::pin(async { Ok(JsValue::String("previous claim".into())) }));
    bus.emit_request(&"claude-bridge:usage-all-request".into(), &mut claimed);
    assert_eq!(
        claimed.response.unwrap().await.unwrap(),
        JsValue::String("previous claim".into())
    );
}

#[tokio::test]
async fn claude_bridge_later_instances_defer_provider_registration_without_replacing_parent() {
    let dir = tempfile::tempdir().unwrap();
    let ownership = Arc::new(ownership::Ownership::default());
    let mut registry = ExtensionRegistry::new();
    registry.register_hidden(
        "<fixture:ownership>",
        Arc::new(move |pi| {
            let result = activate_with_ownership(pi, ownership.clone());
            Box::pin(async { result })
        }),
    );
    let cwd = JsString::from(dir.path().to_str().unwrap());
    let first = registry
        .load_extensions(&["<fixture:ownership>".into()], &cwd, None, None)
        .await
        .unwrap();
    assert!(first.errors.is_empty());
    let pending = first.runtime.pending_provider_registrations();
    assert_eq!(pending.len(), 1);
    let parent_stream = pending[0].config.stream_simple.clone().unwrap();
    let second = registry
        .load_extensions(&["<fixture:ownership>".into()], &cwd, None, None)
        .await
        .unwrap();
    assert!(second.errors.is_empty());
    assert!(
        second.runtime.pending_provider_registrations().is_empty(),
        "a later instance must wait for its session registry"
    );
    let runner = lifecycle_tests::runner(dir.path(), second.clone(), None);
    let models = runner.create_context().model_registry().unwrap();
    models
        .register_provider("claude-bridge", pending[0].config.clone())
        .unwrap();
    let mut start = crate::core::extensions::types::ExtensionEvent::SessionStart(
        crate::core::extensions::types::SessionStartEvent {
            reason: crate::core::extensions::types::SessionStartReason::Startup,
            previous_session_file: None,
        },
    );
    runner.emit(&mut start).await;
    let config = models.get_provider("claude-bridge").unwrap();
    let mut context = pi_ai::types::Context {
        system_prompt: None,
        messages: Vec::new(),
        tools: None,
    };
    let mut model = pi_ai::providers::models_data::builtin_model("anthropic", "claude-opus-5-5").unwrap();
    model.provider = "claude-bridge".into();
    model.api = "claude-bridge".into();
    // An orphan result makes the existing stream observable without credentials or a child process.
    context.messages.push(js_json::from_value(&js_json::parse_lossless(r#"{"role":"toolResult","toolCallId":"gone","toolName":"read","content":[],"isError":false,"timestamp":1}"#).unwrap()).unwrap());
    let preserved = config.stream_simple.unwrap()(model.clone(), context.clone(), None)
        .unwrap()
        .result()
        .await
        .unwrap();
    assert_eq!(preserved.stop_reason, pi_ai::types::StopReason::Stop);
    let independent = registry
        .load_extensions(&["<fixture:ownership>".into()], &cwd, None, None)
        .await
        .unwrap();
    let runner = lifecycle_tests::runner(dir.path(), independent, None);
    assert!(
        runner
            .create_context()
            .model_registry()
            .unwrap()
            .get_provider("claude-bridge")
            .is_none()
    );
    runner.emit(&mut start).await;
    let independent = runner
        .create_context()
        .model_registry()
        .unwrap()
        .get_provider("claude-bridge")
        .expect("child independent registry receives its own provider");
    let output = independent.stream_simple.unwrap()(model, context, None)
        .unwrap()
        .result()
        .await
        .unwrap();
    assert_eq!(output.stop_reason, pi_ai::types::StopReason::Stop);
    drop(parent_stream);
}

#[tokio::test]
async fn claude_bridge_settings_package_loads_hidden_native_factory_and_registers_models() {
    let dir = tempfile::tempdir().unwrap();
    let agent = dir.path().join("pi/agent");
    let package = dir.path().join("Repos/pi-claude-bridge");
    std::fs::create_dir_all(&agent).unwrap();
    std::fs::create_dir_all(package.join("src")).unwrap();
    std::fs::write(agent.join("settings.json"), r#"{"packages":["../../Repos/pi-claude-bridge"],"enabledModels":["claude-bridge/claude-opus-5-5","claude-bridge/claude-haiku-5-5"]}"#).unwrap();
    std::fs::write(
        package.join("package.json"),
        r#"{"name":"pi-claude-bridge","pi":{"extensions":["./src/index.ts"]}}"#,
    )
    .unwrap();
    std::fs::write(package.join("src/index.ts"), "").unwrap();
    let cwd = JsString::from(dir.path().to_str().unwrap());
    let agent_dir = JsString::from(agent.to_str().unwrap());
    let settings = SettingsManager::create(
        &cwd,
        Some(&agent_dir),
        SettingsManagerCreateOptions {
            project_trusted: Some(true),
        },
    )
    .unwrap();
    let mut registry = ExtensionRegistry::new();
    register(&mut registry);
    let loader = DefaultResourceLoader::new(DefaultResourceLoaderOptions {
        cwd,
        agent_dir,
        settings_manager: Some(Arc::new(Mutex::new(settings))),
        extension_registry: Some(Arc::new(registry)),
        ..Default::default()
    })
    .unwrap();
    loader.reload().await.unwrap();
    let result = loader.get_extensions();
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(result.extensions.len(), 1);
    let registrations = result.runtime.pending_provider_registrations();
    let provider = registrations
        .iter()
        .find(|p| p.name == "claude-bridge")
        .expect("native provider registered during activation");
    let models = provider.config.models.as_ref().unwrap();
    for id in ["claude-opus-5-5", "claude-haiku-5-5"] {
        assert!(models.iter().any(|m| m.id == id));
    }
    assert!(provider.config.stream_simple.is_some());
}
