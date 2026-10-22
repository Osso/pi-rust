use pi_coding_agent::core::package_manager::{
    DefaultPackageManager, PackageSettingsSnapshot, PackageSettingsStore, PackageSource, ParsedSource,
};
use pi_js_compat::js_json::JsString;
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
    let manager = DefaultPackageManager::new(&raw, &raw, std::sync::Arc::new(Settings));
    let result = manager
        .resolve_extension_sources(&[raw.clone()], false, true)
        .await
        .unwrap();
    assert_eq!(result.extensions.len(), 1);
    assert_eq!(result.extensions[0].path, raw.clone() + "/index.ts");
    assert_eq!(result.extensions[0].metadata.base_dir, Some(raw));
}

#[test]
fn local_and_npm_source_parse_keeps_native_units() {
    let manager = DefaultPackageManager::new(&"/project".into(), &"/agent".into(), std::sync::Arc::new(Settings));
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
