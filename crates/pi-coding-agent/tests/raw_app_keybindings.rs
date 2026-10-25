use crate::shared_oracle as oracle;
use pi_coding_agent::core::keybindings::KeybindingsManager;
use pi_js_compat::js_json::{JsString, parse_lossless};
use serde_json::json;
#[test]
fn app_keybinding_file_preserves_raw_json_overrides() {
    let mut oracle = oracle::TsOracle::spawn("raw-coding-tui-oracle.ts", "packages/coding-agent/src");
    for raw in [
        JsString::from_utf16(vec![0xd800]),
        JsString::from_utf16(vec![0xdc00]),
        JsString::from("😀"),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let content = format!("{{\"app.interrupt\":{}}}", serde_json::to_string(&raw).unwrap());
        std::fs::write(directory.path().join("keybindings.json"), content).unwrap();
        let expected = oracle.request(&json!({"directory":directory.path()}));
        assert_eq!(expected["ok"], true, "{expected}");
        let manager = KeybindingsManager::create(Some(&JsString::from(
            (directory.path()).to_str().expect("fixture path must be UTF-8"),
        )))
        .unwrap();
        let actual = serde_json::to_string(&(manager.get_keys("app.interrupt"), manager.get_user_bindings())).unwrap();
        // Source stores overrides as an object; preserve raw key/value bytes while normalizing only object-vs-ordered-pairs shape.
        let expected = parse_lossless(expected["json"].as_str().unwrap()).unwrap();
        let expected = expected.as_array().unwrap();
        let actual = parse_lossless(actual).unwrap();
        let actual = actual.as_array().unwrap();
        assert_eq!(&actual[0], &expected[0], "raw={raw:?}");
        let source_keys = expected[1].get("app.interrupt").unwrap().as_string().unwrap();
        assert_eq!(
            actual[1].as_array().unwrap()[0].as_array().unwrap()[1]
                .as_array()
                .unwrap()[0]
                .as_string()
                .unwrap(),
            source_keys
        );
    }
}
