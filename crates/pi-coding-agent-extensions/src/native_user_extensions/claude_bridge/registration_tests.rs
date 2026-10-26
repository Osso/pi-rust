use super::*;
use crate::core::extensions::ExtensionRegistry;
use crate::core::resource_loader::{DefaultResourceLoader, DefaultResourceLoaderOptions, ResourceLoader};
use crate::core::settings_manager::{SettingsManager, SettingsManagerCreateOptions};
use pi_js_compat::js_json::JsString;
use std::sync::{Arc, Mutex};

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
