use crate::shared_oracle as oracle;
use pi_js_compat::js_json::{JsString, parse_lossless};
use pi_tui::keybindings::{KeybindingDefinition, KeybindingsManager};
use serde_json::json;
#[test]
fn raw_registry_keys_descriptions_conflicts_and_config_match_source() {
    let mut oracle = oracle::TsOracle::spawn("raw-tui-oracle.ts", "packages/tui/src");
    for raw in [
        JsString::from_utf16(vec![0xd800]),
        JsString::from_utf16(vec![0xdc00]),
        JsString::from("😀"),
        JsString::from_utf16(vec![97, 0xd800, 0x301, 98, 0xdc00]),
    ] {
        let expected = oracle.request(&json!({"operation":"keybindings_native","units":raw.units()}));
        assert_eq!(expected["ok"], true, "{expected}");
        let first = raw.clone() + "first";
        let second = raw.clone() + "second";
        let key = JsString::from("ctrl+") + &raw;
        let manager = KeybindingsManager::new(
            vec![
                (
                    first.clone(),
                    KeybindingDefinition {
                        default_keys: vec![key.clone(), "enter".into()],
                        description: Some(raw.clone()),
                    },
                ),
                (
                    second.clone(),
                    KeybindingDefinition {
                        default_keys: vec!["escape".into()],
                        description: Some(raw.clone()),
                    },
                ),
            ],
            vec![
                (first.clone(), vec![key.clone(), key.clone()]),
                (second.clone(), vec![key.clone()]),
            ],
        );
        let definition = manager.get_definition(&first).unwrap();
        let conflicts: Vec<_> = manager
            .get_conflicts()
            .into_iter()
            .map(|conflict| (conflict.key, conflict.keybindings))
            .collect();
        let actual = serde_json::to_string(&(
            manager.get_keys(&first),
            (&definition.default_keys, &definition.description),
            conflicts,
            manager.get_resolved_bindings(),
            manager.get_user_bindings(),
            manager.matches(&raw, &first),
        ))
        .unwrap();
        assert_eq!(
            parse_lossless(actual).unwrap(),
            parse_lossless(expected["json"].as_str().unwrap()).unwrap(),
            "raw={raw:?}"
        );
    }
}
