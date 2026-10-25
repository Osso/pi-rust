use pi_coding_agent::core::extensions::loader::{
    ExtensionRegistry, inline_extension_name, is_extension_factory_disabled, is_extension_path_disabled,
};
use pi_js_compat::js_json::JsString;

#[path = "support/oracle.rs"]
mod oracle;

#[tokio::test]
async fn loader_resolves_relative_entries_and_manifest_absolute_paths_like_frozen() {
    use pi_coding_agent::core::extensions::loader::discover_extension_paths;
    use serde_json::json;
    let root = tempfile::tempdir().unwrap();
    let cwd = root.path().join("project");
    let agent = root.path().join("agent");
    let package = cwd.join("package");
    std::fs::create_dir_all(&package).unwrap();
    let entry = cwd.join("entry.ts");
    let factory_source = "export default function () {}\n";
    std::fs::write(&entry, factory_source).unwrap();
    std::fs::write(package.join("index.ts"), factory_source).unwrap();
    std::fs::write(
        package.join("package.json"),
        json!({"pi":{"extensions":[entry]}}).to_string(),
    )
    .unwrap();
    let cwd = JsString::from(cwd.to_str().unwrap());
    let agent = JsString::from(agent.to_str().unwrap());
    let entry = JsString::from(entry.to_str().unwrap());
    let mut registry = ExtensionRegistry::new();
    registry.register(entry.clone(), std::sync::Arc::new(|_| Box::pin(async { Ok(()) })));
    let mut oracle = oracle::TsOracle::spawn(
        "extensions-loader-oracle.ts",
        "packages/coding-agent/src/core/extensions/loader.ts",
    );
    let expected = oracle.request(&json!({"id":1,"paths":["entry.ts"],"cwd":cwd}));
    assert_eq!(expected["ok"], true, "{expected}");
    let actual = registry
        .load_extensions(&["entry.ts".into()], &cwd, None, None)
        .await
        .unwrap();
    assert!(actual.errors.is_empty(), "{:?}", actual.errors);
    let metadata = actual
        .extensions
        .iter()
        .map(|extension| {
            let extension = extension.read();
            json!({"path":extension.path,"resolvedPath":extension.resolved_path,"sourceInfo":extension.source_info})
        })
        .collect::<Vec<_>>();
    assert_eq!(json!(metadata), expected["extensions"]);
    let expected = oracle.request(&json!({"id":2,"discover":true,"paths":["package"],"cwd":cwd,"agentDir":agent}));
    assert_eq!(expected["ok"], true, "{expected}");
    let paths = discover_extension_paths(&["package".into()], &cwd, &agent).unwrap();
    assert_eq!(json!(paths), json!([entry]));
    assert_eq!(
        json!(paths),
        json!(
            expected["extensions"]
                .as_array()
                .unwrap()
                .iter()
                .map(|extension| extension["path"].clone())
                .collect::<Vec<_>>()
        )
    );
}

#[tokio::test]
async fn loader_path_errors_propagate_instead_of_loading_an_unresolved_path() {
    use serde_json::json;
    let mut oracle = oracle::TsOracle::spawn(
        "extensions-loader-oracle.ts",
        "packages/coding-agent/src/core/extensions/loader.ts",
    );
    let cwd = JsString::from("/tmp");
    for (id, paths, cwd) in [
        (3, vec!["file://remote/entry.ts"], cwd),
        (4, vec![], "file://remote/project".into()),
    ] {
        let expected = oracle.request(&json!({"id":id,"paths":paths,"cwd":cwd}));
        assert_eq!(expected["ok"], false, "{expected}");
        let paths = paths.into_iter().map(JsString::from).collect::<Vec<_>>();
        let actual = ExtensionRegistry::new()
            .load_extensions(&paths, &cwd, None, None)
            .await
            .err()
            .expect("path error");
        assert_eq!(actual.message, JsString::from(expected["message"].as_str().unwrap()));
    }
}

#[test]
fn disabled_extension_identity_preserves_unpaired_units() {
    let name = JsString::from_utf16(vec![b'x' as u16, 0xd800]);
    let inline = JsString::from("<first-party:") + &name + ">";
    assert_eq!(inline_extension_name(&inline), Some(name.clone()));
    assert!(is_extension_factory_disabled(&[name.clone()], &inline));
    assert!(!is_extension_factory_disabled(&["x�".into()], &inline));
    let path = JsString::from("/extensions/") + &name;
    assert!(is_extension_path_disabled(&[name], &path, &path));
    assert!(!is_extension_path_disabled(&["x�".into()], &path, &path));
}

#[test]
fn registry_retains_distinct_native_extension_ids() {
    let raw = JsString::from_utf16(vec![0xd800]);
    let replacement = JsString::from("�");
    let mut registry = ExtensionRegistry::new();
    registry.register(raw.clone(), std::sync::Arc::new(|_| Box::pin(async { Ok(()) })));
    registry.register(replacement.clone(), std::sync::Arc::new(|_| Box::pin(async { Ok(()) })));
    assert_eq!(registry.ids(), vec![raw.clone(), replacement.clone()]);
    assert!(registry.get(&raw).is_some());
    assert!(registry.get(&replacement).is_some());
}
