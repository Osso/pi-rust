//! Settings keep escaped unpaired UTF-16 units through load, getters and persisted writes.

use pi_coding_agent::core::settings_manager::{SettingsManager, SettingsManagerCreateOptions};
use pi_js_compat::js_json::JsString;

fn raw(prefix: &str, unit: u16) -> JsString {
    JsString::from_utf16(prefix.encode_utf16().chain([unit]).collect())
}

#[test]
fn escaped_unpaired_units_survive_getters_and_writes() {
    let directory = tempfile::tempdir().unwrap();
    let agent_dir = directory.path().join("agent");
    let cwd = directory.path().join("project");
    std::fs::create_dir_all(&agent_dir).unwrap();
    std::fs::create_dir_all(&cwd).unwrap();
    std::fs::write(
        agent_dir.join("settings.json"),
        r#"{"theme":"dark\ud800","extensions":["ext\udc00"],"shellPath":"/bin/s\ud801"}"#,
    )
    .unwrap();
    let mut manager = SettingsManager::create(
        &cwd.to_str().unwrap().into(),
        Some(&agent_dir.to_str().unwrap().into()),
        SettingsManagerCreateOptions::default(),
    )
    .unwrap();

    assert_eq!(manager.get_theme_setting(), Some(&raw("dark", 0xd800)));
    assert_eq!(manager.get_extension_paths(), vec![raw("ext", 0xdc00)]);
    assert_eq!(manager.get_shell_path(), Some(raw("/bin/s", 0xd801)));

    manager.set_theme(&raw("light", 0xdfff));
    manager.flush();
    let written = std::fs::read_to_string(agent_dir.join("settings.json")).unwrap();
    assert!(written.contains(r#""theme": "light\udfff""#), "{written}");
    assert!(written.contains(r#""ext\udc00""#), "{written}");
}

/// settings-manager.ts:282 reads with `readFileSync(path, "utf-8")`: an invalid
/// byte becomes U+FFFD and every other setting in that scope still loads.
#[test]
fn invalid_utf8_bytes_decode_like_node_without_dropping_the_scope() {
    let directory = tempfile::tempdir().unwrap();
    let agent_dir = directory.path().join("agent");
    let cwd = directory.path().join("project");
    std::fs::create_dir_all(&agent_dir).unwrap();
    std::fs::create_dir_all(cwd.join(".pi")).unwrap();
    std::fs::write(
        agent_dir.join("settings.json"),
        b"{\"theme\":\"light\",\"defaultModel\":\"kept\",\"externalEditor\":\"editor caf\xe9\"}",
    )
    .unwrap();
    std::fs::write(cwd.join(".pi/settings.json"), b"{\"shellPath\":\"/bin/s\xe9\"}").unwrap();
    let mut manager = SettingsManager::create(
        &cwd.to_str().unwrap().into(),
        Some(&agent_dir.to_str().unwrap().into()),
        SettingsManagerCreateOptions::default(),
    )
    .unwrap();
    assert!(manager.drain_errors().is_empty());
    assert_eq!(manager.get_theme_setting(), Some(&JsString::from("light")));
    assert_eq!(manager.get_default_model(), Some(&JsString::from("kept")));
    assert_eq!(
        manager.get_external_editor_command(),
        JsString::from("editor caf\u{fffd}")
    );
    assert_eq!(manager.get_shell_path(), Some(JsString::from("/bin/s\u{fffd}")));
}
