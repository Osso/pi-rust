use crate::shared_oracle as oracle;
use pi_coding_agent::modes::interactive::theme::{self, ColorMode, ThemeColor};
use pi_js_compat::js_json::{JsString, JsValue, encode_node_utf8, parse_lossless, stringify_lossless};
use serde_json::json;
use std::sync::Arc;

#[test]
fn native_theme_files_and_errors_match_frozen_source() {
    let mut oracle = oracle::TsOracle::spawn("raw-theme-file-oracle.ts", "packages/coding-agent/src");
    for raw in [
        JsString::from_utf16(vec![0xd800]),
        JsString::from_utf16(vec![0xdc00]),
        JsString::from("😀"),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = JsString::from(directory.path().display().to_string()) + "/" + &raw + ".json";
        let base = parse_lossless(include_str!("../theme/dark.json")).unwrap();
        for case in [
            "valid",
            "slash-name",
            "invalid-var",
            "missing-var",
            "partial-hex",
            "invalid-hex",
            "circular-var",
            "missing-file",
        ] {
            let mut value = base.clone();
            value["name"] = JsValue::String(raw.clone());
            value["vars"]
                .as_object_mut()
                .unwrap()
                .insert(raw.clone(), JsValue::String("#123456".into()));
            value["colors"]["accent"] = JsValue::String(raw.clone());
            match case {
                "slash-name" => value["name"] = JsValue::String(raw.clone() + "/x"),
                "invalid-var" => {
                    value["vars"]
                        .as_object_mut()
                        .unwrap()
                        .insert(raw.clone(), JsValue::Bool(false));
                }
                "missing-var" => value["colors"]["accent"] = JsValue::String(raw.clone() + "missing"),
                "partial-hex" => value["colors"]["accent"] = JsValue::String(JsString::from("#1") + &raw + "2345"),
                "invalid-hex" => value["colors"]["accent"] = JsValue::String(JsString::from("#") + &raw + "12345"),
                "circular-var" => {
                    value["vars"]
                        .as_object_mut()
                        .unwrap()
                        .insert(raw.clone(), JsValue::String(raw.clone()));
                }
                _ => {}
            }
            std::fs::write(encode_node_utf8(&path), stringify_lossless(&value)).unwrap();
            if case == "missing-file" {
                std::fs::remove_file(encode_node_utf8(&path)).unwrap();
            }
            let expected = oracle.request(&json!({"pathUnits":path.units(),"units":raw.units()}));
            assert_eq!(expected["ok"], true, "{expected}");
            let actual = match theme::load_theme_from_path(&path, Some(ColorMode::Truecolor)) {
                Ok(theme) => serde_json::to_string(&(
                    theme.name.clone(),
                    theme.source_path.clone(),
                    theme.fg(ThemeColor::Accent, &raw),
                ))
                .unwrap(),
                Err(error) => serde_json::to_string(&error.message()).unwrap(),
            };
            assert_eq!(
                parse_lossless(actual).unwrap(),
                parse_lossless(expected["json"].as_str().unwrap()).unwrap(),
                "case {case} raw {raw:?}"
            );
        }
    }
}

#[test]
fn theme_file_keeps_raw_name_path_and_variable_references() {
    for raw in [
        JsString::from_utf16(vec![0xd800]),
        JsString::from_utf16(vec![0xdc00]),
        JsString::from("😀"),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = JsString::from(directory.path().display().to_string()) + "/" + &raw + ".json";
        let mut json = parse_lossless(include_str!("../theme/dark.json")).unwrap();
        json["name"] = JsValue::String(raw.clone());
        json["vars"]
            .as_object_mut()
            .unwrap()
            .insert(raw.clone(), JsValue::String("#123456".into()));
        json["colors"]["accent"] = JsValue::String(raw.clone());
        std::fs::write(encode_node_utf8(&path), stringify_lossless(&json)).unwrap();
        let loaded = theme::load_theme_from_path(&path, Some(ColorMode::Truecolor)).unwrap();
        assert_eq!(loaded.name.as_ref(), Some(&raw));
        assert_eq!(loaded.source_path.as_ref(), Some(&path));
        assert_eq!(
            loaded.fg(ThemeColor::Accent, &raw),
            JsString::from("\x1b[38;2;18;52;86m") + &raw + "\x1b[39m"
        );
        let loaded = Arc::new(loaded);
        theme::set_registered_themes(vec![Arc::clone(&loaded)]).unwrap();
        assert!(Arc::ptr_eq(&theme::get_theme_by_name(&raw).unwrap(), &loaded));
        assert!(theme::set_theme(&raw, false).unwrap().success);
        json["colors"]["accent"] = JsValue::String(raw.clone() + "missing");
        std::fs::write(encode_node_utf8(&path), stringify_lossless(&json)).unwrap();
        let error = theme::load_theme_from_path(&path, Some(ColorMode::Truecolor)).unwrap_err();
        assert_eq!(
            error.message(),
            JsString::from("Variable reference not found: ") + &raw + "missing"
        );
    }
}
