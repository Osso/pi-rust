use pi_coding_agent::core::diagnostics::ResourceDiagnostic;
use pi_js_compat::js_json::{JsString, parse_lossless};
#[test]
fn resource_diagnostic_json_round_trip_keeps_all_raw_text_fields() {
    for raw in [
        JsString::from_utf16(vec![0xd800]),
        JsString::from_utf16(vec![0xdc00]),
        JsString::from("😀"),
    ] {
        let value = serde_json::to_string(&raw).unwrap();
        let input = format!(
            r#"{{"type":"collision","message":{value},"path":{value},"collision":{{"resourceType":"extension","name":{value},"winnerPath":{value},"loserPath":{value},"winnerSource":{value},"loserSource":{value}}}}}"#
        );
        let diagnostic = serde_json::from_str::<ResourceDiagnostic>(&input);
        assert!(diagnostic.is_ok(), "raw JSON diagnostic rejected: {diagnostic:?}");
        let output = serde_json::to_string(&diagnostic.unwrap()).unwrap();
        assert_eq!(parse_lossless(output).unwrap(), parse_lossless(input).unwrap());
    }
}
