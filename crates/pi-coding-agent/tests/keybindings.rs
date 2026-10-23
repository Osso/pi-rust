use pi_js_compat::js_json::JsString;
use std::fs;

use pi_coding_agent::core::keybindings::KeybindingsManager;
use tempfile::TempDir;

#[test]
fn loads_legacy_key_names_before_the_file_is_rewritten() {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("keybindings.json"),
        r#"{"selectConfirm":"enter","interrupt":"ctrl+x"}"#,
    )
    .unwrap();

    let keybindings = KeybindingsManager::create(Some(&JsString::from(
        (temp.path()).to_str().expect("fixture path must be UTF-8"),
    )))
    .unwrap();

    assert_eq!(
        keybindings.get_user_bindings(),
        vec![
            (JsString::from("tui.select.confirm"), vec![JsString::from("enter")]),
            (JsString::from("app.interrupt"), vec![JsString::from("ctrl+x")]),
        ]
    );
    assert_eq!(keybindings.get_keys("tui.select.confirm"), vec!["enter"]);
    assert_eq!(keybindings.get_keys("app.interrupt"), vec!["ctrl+x"]);
}

#[test]
fn defaults_bash_backgrounding_and_agent_slots() {
    let keybindings = KeybindingsManager::new(Vec::new(), None);

    assert_eq!(keybindings.get_keys("app.tool.background"), vec!["ctrl+b"]);
    let slots = [
        ("app.agent.select", "alt+0"),
        ("app.agent.slot1", "alt+1"),
        ("app.agent.slot2", "alt+2"),
        ("app.agent.slot3", "alt+3"),
        ("app.agent.slot4", "alt+4"),
        ("app.agent.slot5", "alt+5"),
        ("app.agent.slot6", "alt+6"),
        ("app.agent.slot7", "alt+7"),
        ("app.agent.slot8", "alt+8"),
        ("app.agent.slot9", "alt+9"),
    ];
    for (binding, key) in slots {
        assert_eq!(keybindings.get_keys(binding), vec![key]);
    }
}

#[test]
fn reload_rereads_the_keybindings_file() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("keybindings.json");
    fs::write(&path, r#"{"app.tools.expand":"ctrl+x"}"#).unwrap();
    let mut keybindings = KeybindingsManager::create(Some(&JsString::from(
        (temp.path()).to_str().expect("fixture path must be UTF-8"),
    )))
    .unwrap();
    assert_eq!(keybindings.get_keys("app.tools.expand"), vec!["ctrl+x"]);

    fs::write(&path, r#"{"app.tools.expand":"ctrl+y"}"#).unwrap();
    keybindings.reload();
    assert_eq!(keybindings.get_keys("app.tools.expand"), vec!["ctrl+y"]);
}
