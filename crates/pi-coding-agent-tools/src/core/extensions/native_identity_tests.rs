use super::loader::*;
use crate::core::event_bus::EventBusController;
use crate::core::extensions::{HandlerError, create_extension_runtime};
use pi_js_compat::js_json::JsString;
use std::fs;
use std::sync::Arc;

fn factory() -> ExtensionFactory {
    Arc::new(|_| Box::pin(async { Ok(()) }))
}

#[tokio::test]
async fn native_package_name_uses_nearest_manifest_within_three_directories() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("package/src/nested")).unwrap();
    fs::write(dir.path().join("package.json"), r#"{"name":"outer"}"#).unwrap();
    fs::write(dir.path().join("package/package.json"), r#"{"name":"@user/bridge"}"#).unwrap();
    fs::write(dir.path().join("package/src/nested/index.ts"), "").unwrap();
    let mut registry = ExtensionRegistry::new();
    registry.register_hidden("<native:@user/bridge>", factory());
    let path = JsString::from("package/src/nested/index.ts");
    let result = registry
        .load_extensions(
            std::slice::from_ref(&path),
            &JsString::from(dir.path().to_str().unwrap()),
            None,
            None,
        )
        .await
        .unwrap();
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(result.extensions.len(), 1);
    let extension = result.extensions[0].read();
    assert_eq!(extension.path, path);
    assert_eq!(extension.source_info.source, "local");
    assert_eq!(extension.source_info.path, path);
    assert_eq!(
        extension.resolved_path,
        dir.path().join("package/src/nested/index.ts").to_str().unwrap()
    );
}

#[tokio::test]
async fn native_file_stem_loads_discovered_extension() {
    let dir = tempfile::tempdir().unwrap();
    let agent = dir.path().join("agent");
    fs::create_dir_all(agent.join("extensions")).unwrap();
    let path = agent.join("extensions/sample-native.ts");
    fs::write(&path, "").unwrap();
    let mut registry = ExtensionRegistry::new();
    registry.register_hidden("<native:sample-native>", factory());
    let result = registry
        .discover_and_load_extensions(
            &[],
            &JsString::from(dir.path().to_str().unwrap()),
            &JsString::from(agent.to_str().unwrap()),
            None,
            &[],
        )
        .await
        .unwrap();
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(result.extensions.len(), 1);
    assert_eq!(result.extensions[0].read().path, path.to_str().unwrap());
}

#[tokio::test]
async fn native_factories_do_not_load_without_referencing_paths() {
    let dir = tempfile::tempdir().unwrap();
    let mut registry = ExtensionRegistry::new();
    registry.register_hidden("<native:hidden>", factory());
    registry.register("<native:visible>", factory());
    registry.register("<first-party:ordinary>", factory());
    let (extensions, errors) = registry
        .load_registered(
            &[],
            &JsString::from(dir.path().to_str().unwrap()),
            Arc::new(EventBusController::new()),
            create_extension_runtime(),
        )
        .await;
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(extensions.len(), 1);
    assert_eq!(extensions[0].read().path, "<first-party:ordinary>");
}

#[tokio::test]
async fn native_unknown_path_keeps_frozen_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = JsString::from(dir.path().join("unknown.ts").to_str().unwrap());
    fs::write(dir.path().join("unknown.ts"), "").unwrap();
    let result = ExtensionRegistry::new()
        .load_extensions(
            std::slice::from_ref(&path),
            &JsString::from(dir.path().to_str().unwrap()),
            None,
            None,
        )
        .await
        .unwrap();
    assert!(result.extensions.is_empty());
    assert_eq!(result.errors.len(), 1);
    assert_eq!(result.errors[0].path, path);
    assert_eq!(
        result.errors[0].error,
        pi_js_compat::js_format!("Extension does not export a valid factory function: {}", path)
    );
}

#[tokio::test]
async fn native_settings_packages_resolve_manifest_entries_to_native_identity() {
    use crate::core::resource_loader::{DefaultResourceLoader, DefaultResourceLoaderOptions, ResourceLoader};
    use crate::core::settings_manager::{SettingsManager, SettingsManagerCreateOptions};
    use std::sync::Mutex;

    let dir = tempfile::tempdir().unwrap();
    let agent = dir.path().join("agent");
    let package = dir.path().join("Repos/bridge");
    fs::create_dir_all(&agent).unwrap();
    fs::create_dir_all(package.join("src")).unwrap();
    fs::write(agent.join("settings.json"), r#"{"packages":["../Repos/bridge"]}"#).unwrap();
    fs::write(
        package.join("package.json"),
        r#"{"name":"pi-claude-bridge","pi":{"extensions":["./src/index.ts"]}}"#,
    )
    .unwrap();
    fs::write(package.join("src/index.ts"), "").unwrap();
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
    registry.register_hidden("<native:pi-claude-bridge>", factory());
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
    let extension = result.extensions[0].read();
    assert_eq!(extension.path, package.join("src/index.ts").to_str().unwrap());
    assert_eq!(extension.source_info.path, extension.path);
}

#[tokio::test]
async fn native_path_factory_takes_precedence_over_package_identity() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("package.json"), r#"{"name":"bridge"}"#).unwrap();
    let path = JsString::from(dir.path().join("index.ts").to_str().unwrap());
    fs::write(dir.path().join("index.ts"), "").unwrap();
    let mut registry = ExtensionRegistry::new();
    registry.register_hidden(
        "<native:bridge>",
        Arc::new(|_| Box::pin(async { Err(HandlerError::new("identity must not override path")) })),
    );
    registry.register(path.clone(), factory());
    let result = registry
        .load_extensions(&[path], &JsString::from(dir.path().to_str().unwrap()), None, None)
        .await
        .unwrap();
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(result.extensions.len(), 1);
}

#[tokio::test]
async fn native_identity_does_not_search_a_fourth_directory() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("a/b/c")).unwrap();
    fs::write(dir.path().join("package.json"), r#"{"name":"too-far"}"#).unwrap();
    fs::write(dir.path().join("a/b/c/stem.ts"), "").unwrap();
    let mut registry = ExtensionRegistry::new();
    registry.register_hidden(
        "<native:too-far>",
        Arc::new(|_| Box::pin(async { Err(HandlerError::new("outside search boundary")) })),
    );
    registry.register_hidden("<native:stem>", factory());
    let result = registry
        .load_extensions(
            &[JsString::from("a/b/c/stem.ts")],
            &JsString::from(dir.path().to_str().unwrap()),
            None,
            None,
        )
        .await
        .unwrap();
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(result.extensions.len(), 1);
}
