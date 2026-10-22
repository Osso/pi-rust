use pi_coding_agent::package_manager_cli::{handle_package_command, parse_package_command};
use pi_js_compat::js_json::JsString;
#[test]
fn package_cli_retains_raw_source_and_diagnostic_argument() {
    let raw = JsString::from_utf16(vec![0xd800]);
    let parsed = parse_package_command(&["install".into(), raw.clone()]).unwrap();
    assert_eq!(parsed.source, Some(raw.clone()));
    let invalid = JsString::from("--") + raw;
    let outcome = handle_package_command(&["install".into(), invalid.clone()]).unwrap();
    assert!(outcome.stderr.iter().any(|message| message.contains(&invalid)));
}
