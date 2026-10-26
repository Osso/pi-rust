use super::*;
use crate::core::extensions::ExtensionRegistry;
use crate::core::resource_loader::{DefaultResourceLoader, DefaultResourceLoaderOptions, ResourceLoader};
use crate::core::settings_manager::{SettingsManager, SettingsManagerCreateOptions};
use pi_js_compat::js_json::JsString;
use std::sync::{Arc, Mutex};

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
