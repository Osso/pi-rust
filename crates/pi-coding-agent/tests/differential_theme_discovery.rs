use crate::shared_oracle as oracle;
use pi_coding_agent::modes::interactive::theme::{self, ColorMode, REQUIRED_COLORS, Theme};
use pi_js_compat::{
    js_json,
    js_json::{JsString, JsValue},
    process_env,
};
use std::sync::Arc;
#[test]
fn custom_theme_discovery_order_truthiness_and_precedence_match_frozen() {
    let root = tempfile::tempdir().unwrap();
    let themes = root.path().join("themes");
    std::fs::create_dir(&themes).unwrap();
    for (file, name, index) in [
        ("z.json", JsString::from("duplicate"), 1),
        ("a.json", JsString::from("duplicate"), 2),
        ("empty.json", JsString::default(), 3),
        ("dark-custom.json", JsString::from("dark"), 4),
        ("raw.json", JsString::from_utf16(vec![0xd800]), 5),
        ("skip.txt", JsString::from("not-json-extension"), 6),
    ] {
        let fields = REQUIRED_COLORS
            .iter()
            .map(|key| format!("\"{key}\":{index}"))
            .collect::<Vec<_>>()
            .join(",");
        let name = pi_js_compat::js_json::stringify_lossless(&JsValue::String(name));
        let content = format!("{{\"name\":{name},\"colors\":{{{fields},\"10\":10,\"2\":2}}}}");
        std::fs::write(themes.join(file), content).unwrap();
    }
    std::fs::write(themes.join("invalid.json"), "{").unwrap();
    std::fs::create_dir(themes.join("directory.json")).unwrap();
    process_env::set("PI_CODING_AGENT_DIR", root.path().to_str().unwrap());
    let duplicate = Theme::new(
        &[],
        &[],
        ColorMode::Color256,
        Some("duplicate".into()),
        Some("/registered.json".into()),
    )
    .unwrap();
    let registered = Theme::new(&[], &[], ColorMode::Color256, Some("registered".into()), None).unwrap();
    let empty_path = Theme::new(
        &[],
        &[],
        ColorMode::Color256,
        Some("empty-path".into()),
        Some("".into()),
    )
    .unwrap();
    let file_export = Theme::new(
        &[],
        &[],
        ColorMode::Color256,
        Some("file-export".into()),
        Some(themes.join("a.json").to_str().unwrap().into()),
    )
    .unwrap();
    theme::set_registered_themes(vec![
        Arc::new(duplicate),
        Arc::new(registered),
        Arc::new(empty_path),
        Arc::new(file_export),
    ])
    .unwrap();
    let actual: Vec<_> = theme::get_available_themes_with_paths()
        .unwrap()
        .into_iter()
        .map(|info| {
            let file = info.path.map(|path| path.split('/').last().unwrap());
            js_json!({"name":info.name,"file":file})
        })
        .collect();
    let mut source = oracle::TsOracle::spawn_with_environment(
        "theme-discovery-oracle.mts",
        "packages/coding-agent/src/modes/interactive/theme/theme.ts",
        &[("PI_CODING_AGENT_DIR", root.path().as_os_str().to_owned())],
    );
    let expected: JsValue = source.request(&js_json!({"id":1,"exportPath":themes.join("a.json").to_str().unwrap()}));
    let exports: Vec<_> = ["registered", "empty-path"]
        .iter()
        .map(|name| {
            pi_js_compat::js_format!(
                "Error: {}",
                theme::get_resolved_theme_colors(Some(&(*name).into()))
                    .unwrap_err()
                    .message()
            )
        })
        .collect();
    let color_keys: Vec<_> = theme::get_resolved_theme_colors(Some(&"file-export".into()))
        .unwrap()
        .into_iter()
        .map(|(key, _)| key)
        .collect();
    assert_eq!(expected["ok"], JsValue::Bool(true));
    assert_eq!(
        js_json!({"themes":JsValue::Array(actual),"exports":exports,"colorKeys":color_keys}),
        expected["output"]
    );
    process_env::clear();
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
