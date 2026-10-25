use pi_coding_agent::core::package_manager::{
    DefaultPackageManager, PackageSettingsSnapshot, PackageSettingsStore, PackageSource, ParsedSource,
};
use pi_js_compat::js_json::{JsObject, JsString, JsValue};
#[path = "support/oracle.rs"]
mod oracle;
struct Settings;
impl PackageSettingsStore for Settings {
    fn global_settings(&self) -> PackageSettingsSnapshot {
        Default::default()
    }
    fn project_settings(&self) -> PackageSettingsSnapshot {
        Default::default()
    }
    fn set_packages(&self, _: Vec<PackageSource>) {}
    fn set_project_packages(&self, _: Vec<PackageSource>) {}
    fn is_project_trusted(&self) -> bool {
        true
    }
    fn get_npm_command(&self) -> Option<Vec<JsString>> {
        None
    }
}
#[tokio::test]
async fn local_extension_directory_keeps_source_and_base_dir_identity() {
    let temp = tempfile::tempdir().unwrap();
    let raw = JsString::from(temp.path().to_str().unwrap()) + "/" + JsString::from_utf16(vec![0xd800]);
    std::fs::create_dir_all(pi_js_compat::js_json::encode_node_utf8(&raw)).unwrap();
    std::fs::write(
        pi_js_compat::js_json::encode_node_utf8(&(raw.clone() + "/index.ts")),
        "export default () => {};",
    )
    .unwrap();
    let manager = DefaultPackageManager::new(&raw, &raw, std::sync::Arc::new(Settings)).unwrap();
    let result = manager
        .resolve_extension_sources(std::slice::from_ref(&raw), false, true)
        .await
        .unwrap();
    // Frozen package-manager.ts:1297-1302,2074-2086: a directory without a `pi` manifest or
    // resource subdirectories resolves to the directory itself, not its index.ts.
    let mut scenario = JsObject::new();
    scenario.insert("cwd".into(), JsValue::String(raw.clone()));
    scenario.insert("agentDir".into(), JsValue::String(raw.clone()));
    scenario.insert("trusted".into(), JsValue::Bool(true));
    let mut request = JsObject::new();
    request.insert("id".into(), JsValue::Number(1.0));
    request.insert("operation".into(), JsValue::String("resolve_extension_sources".into()));
    request.insert("scenario".into(), JsValue::Object(scenario));
    request.insert("sources".into(), JsValue::Array(vec![JsValue::String(raw.clone())]));
    request.insert("local".into(), JsValue::Bool(false));
    request.insert("temporary".into(), JsValue::Bool(true));
    let mut source = oracle::TsOracle::spawn(
        "package-manager-oracle.ts",
        "packages/coding-agent/src/core/package-manager.ts",
    );
    let response = source.request(&JsValue::Object(request));
    assert_eq!(response.get("ok"), Some(&JsValue::Bool(true)), "{response:?}");
    assert_eq!(
        pi_js_compat::js_json::to_value(&result).unwrap(),
        response["output"]["resolved"]
    );
    assert_eq!(result.extensions.len(), 1);
    assert_eq!(result.extensions[0].path, raw);
    assert_eq!(result.extensions[0].metadata.base_dir, Some(raw));
}

#[test]
fn local_and_npm_source_parse_keeps_native_units() {
    let manager =
        DefaultPackageManager::new(&"/project".into(), &"/agent".into(), std::sync::Arc::new(Settings)).unwrap();
    let raw = JsString::from_utf16(vec![0xd800]);
    let source = JsString::from("./") + &raw;
    let ParsedSource::Local(local) = manager.parse_source(&source) else {
        panic!("local")
    };
    assert_eq!(local.path, source);
    let spec = JsString::from("npm:pkg@") + &raw;
    let ParsedSource::Npm(npm) = manager.parse_source(&spec) else {
        panic!("npm")
    };
    assert_eq!(npm.version, Some(raw));
}
