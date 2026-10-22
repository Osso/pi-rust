#[path = "support/oracle.rs"]
mod oracle;
use pi_coding_agent::core::footer_data_provider::FooterDataProvider;
use pi_js_compat::js_json::{JsString, parse_lossless};
use serde_json::json;
#[tokio::test]
async fn footer_status_replacement_removal_and_map_order_match_frozen_source() {
    let mut oracle = oracle::TsOracle::spawn("raw-footer-statuses-oracle.ts", "packages/coding-agent/src");
    for raw in [
        JsString::default(),
        JsString::from_utf16(vec![0xd800]),
        JsString::from_utf16(vec![0xdc00]),
        JsString::from("😀"),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let expected = oracle.request(&json!({"directory":directory.path(),"units":raw.units()}));
        assert_eq!(expected["ok"], true, "{expected}");
        let z = JsString::from("z") + &raw;
        let a = JsString::from("a") + &raw;
        let provider = FooterDataProvider::new(directory.path());
        provider.set_extension_status(&z, Some(&(JsString::from("first") + &raw)));
        provider.set_extension_status(&a, Some(&(JsString::from("second") + &raw)));
        provider.set_extension_status(&z, Some(&(JsString::from("updated") + &raw)));
        let mut snapshots = vec![provider.get_extension_statuses().into_iter().collect::<Vec<_>>()];
        provider.set_extension_status(&z, None);
        snapshots.push(provider.get_extension_statuses().into_iter().collect::<Vec<_>>());
        provider.clear_extension_statuses();
        snapshots.push(provider.get_extension_statuses().into_iter().collect::<Vec<_>>());
        provider.dispose();
        let actual = serde_json::to_string(&snapshots).unwrap();
        assert_eq!(
            parse_lossless(actual).unwrap(),
            parse_lossless(expected["json"].as_str().unwrap()).unwrap(),
            "raw={raw:?}"
        );
    }
}

#[test]
fn footer_status_trimming_does_not_treat_next_line_control_as_js_whitespace() {
    assert_eq!(
        pi_coding_agent::utils::footer_status::sanitize_footer_status_text("\u{85}"),
        "\u{85}"
    );
}

#[test]
fn native_footer_status_text_and_sorting_match_source() {
    let mut oracle = oracle::TsOracle::spawn("raw-footer-statuses-oracle.ts", "packages/coding-agent/src");
    for raw in [
        JsString::from_utf16(vec![0xd800]),
        JsString::from_utf16(vec![0xdc00]),
        JsString::from("😀"),
    ] {
        let statuses = indexmap::IndexMap::from([
            (JsString::from("z") + &raw, raw.clone() + "z\r\n\t  x"),
            (JsString::from("a") + &raw, raw.clone() + "\u{85}"),
            (JsString::from("goal"), JsString::from(" \t") + &raw + "\u{feff}"),
        ]);
        let expected = oracle.request(&json!({"operation":"helpers","units":raw.units()}));
        assert_eq!(expected["ok"], true, "{expected}");
        let actual = serde_json::to_string(
            &pi_coding_agent::utils::footer_status::format_extension_status_text_lines(&statuses),
        )
        .unwrap();
        assert_eq!(
            parse_lossless(actual).unwrap(),
            parse_lossless(expected["json"].as_str().unwrap()).unwrap(),
            "raw={raw:?}"
        );
    }
}
