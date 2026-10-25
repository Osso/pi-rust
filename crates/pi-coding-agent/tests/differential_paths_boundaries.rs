#[path = "support/oracle.rs"]
mod oracle;

use pi_coding_agent::utils::paths::{PathInputOptions, normalize_path};
use pi_js_compat::js_json::{JsString, JsValue};

#[test]
fn file_url_errors_preserve_frozen_names_messages_and_properties() {
    // Frozen utils/paths.ts:76-77 passes file URLs directly to Node fileURLToPath.
    let inputs = [
        "file:///tmp/a%2Fb",
        "file://host/tmp/x",
        "file://[invalid]/x",
        "file:///%",
        "file:///%C0%AF",
        "file:///%ED%A0%80",
        "file:///%00",
        "file:///a%5Cb",
        "file:///tmp/a?query#fragment",
        "file://localhost/tmp/x",
        "file:///tmp/../x",
        "file://user:password@host/x",
        "file://host:80/x",
        "file:////host/x",
    ];
    let mut source = oracle::TsOracle::spawn("paths-boundaries-oracle.ts", "packages/coding-agent/src/utils/paths.ts");
    let cases = inputs
        .iter()
        .map(|input| pi_js_compat::js_json!({"kind":"normalize","input":input}))
        .collect();
    let expected = source.request(&pi_js_compat::js_json!({"id":1,"cases":JsValue::Array(cases)}));
    let actual: Vec<_> = inputs
        .iter()
        .map(|input| match normalize_path(*input, &PathInputOptions::default()) {
            Ok(value) => pi_js_compat::js_json!({"ok":true,"value":value}),
            Err(error) => {
                pi_js_compat::js_json!({"ok":false,"error":error.native_error().expect("Node error identity").value()})
            }
        })
        .collect();
    for ((input, actual), expected) in inputs
        .iter()
        .zip(actual)
        .zip(expected["output"]["results"].as_array().unwrap())
    {
        assert_eq!(&actual, expected, "URL {input}");
    }
}

#[test]
fn cloud_sync_marking_does_not_suppress_synchronous_argument_errors() {
    // Frozen paths.ts:114-115 / child-process.ts:31-34: spawnSync validation throws.
    let input = JsString::from("bad\0path");
    let mut source = oracle::TsOracle::spawn("paths-boundaries-oracle.ts", "packages/coding-agent/src/utils/paths.ts");
    let expected = source.request(&pi_js_compat::js_json!({"id":1,"cases":[{"kind":"mark","input":input}]}));
    assert_eq!(
        expected["output"]["results"].as_array().unwrap()[0]["ok"],
        JsValue::Bool(false)
    );
    let error = pi_coding_agent::utils::paths::mark_path_ignored_by_cloud_sync(&input).unwrap_err();
    let actual = pi_js_compat::js_json!({"ok":false,"error":error.value()});
    assert_eq!(actual, expected["output"]["results"].as_array().unwrap()[0]);
}
