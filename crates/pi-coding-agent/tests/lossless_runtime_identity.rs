use pi_coding_agent::core::runtime_process::ProcessIdentity;
use pi_js_compat::js_json::{JsString, from_value, parse_lossless, stringify_lossless, to_value};

#[test]
fn process_error_keeps_raw_message_until_display_boundary() {
    let raw = JsString::from("process-") + JsString::from_utf16(vec![0xd800]);
    let error = pi_coding_agent::core::runtime_process::ProcessIdentityError { message: raw.clone() };
    assert_eq!(error.message, raw);
    assert_eq!(error.to_string(), "process-�");
}

#[test]
fn runtime_incarnation_survives_lossless_json_and_node_roundtrip() {
    let wire = r#"{"pid":1,"startTimeTicks":2,"incarnation":"run-\ud800\udfff\ud801"}"#;
    let value = parse_lossless(wire).unwrap();
    let identity: ProcessIdentity = from_value(&value).unwrap();
    let incarnation = identity.incarnation.clone().unwrap();
    assert_eq!(incarnation, value["incarnation"].as_string().unwrap().clone());
    let output = std::process::Command::new("node")
        .args([
            "-e",
            "process.stdout.write(JSON.stringify(JSON.parse(process.argv[1])))",
            wire,
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        stringify_lossless(&to_value(&identity).unwrap()),
        String::from_utf8(output.stdout).unwrap()
    );
}
