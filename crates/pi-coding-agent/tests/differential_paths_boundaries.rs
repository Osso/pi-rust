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
        "file://///a/../b",
        "file:////a/../../b",
        "file:////a/..//b",
        "file://///",
        "file:///C:/..//",
        "file:///C|/../../x",
        "file://C:/x",
        "file:////C:/../../x",
        "file:////%2e%2e/a",
        "file://localhost//a",
        "file://localhost///a/../b",
        "file:///\\\\a\\\\b",
        "file:////a\t/b\n",
        "file:////a?//query",
        "file:////a#//fragment",
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
fn file_url_adversarial_host_path_and_percent_matrix() {
    let hosts = [
        "",
        "localhost",
        "LOCALHOST",
        "local%68ost",
        "localhost.",
        "127.0.0.1",
        "0x7f000001",
        "[::1]",
        "[invalid]",
        "server",
        "user@localhost",
        "localhost:80",
        "%",
        "%00",
        "%2f",
        "a b",
        "a^b",
        "a|b",
        "a<b",
        "é",
        "\u{200d}",
        "\u{ff0f}",
        "C:",
        "C|",
    ];
    let paths = [
        "",
        "/",
        "//",
        "///",
        "/a",
        "//a",
        "///a",
        "/a//b",
        "/a/../b",
        "//a/../../b",
        "/./",
        "/../",
        "//../",
        "/%2e/x",
        "/.%2E/x",
        "/%2e./x",
        "/%2E%2e/x",
        "/a/%252e/x",
        "/C:",
        "/C|",
        "/C:/",
        "/C|/",
        "/C:/..//",
        "//C:/../../x",
        "/x/C:/../../x",
        "/%43:/..",
        "/C%3A/..",
        "/C%7C/..",
        "/a%3Ab%7Cc",
        "/%253A/%257C",
        "/%2f",
        "/%2F/%",
        "/%5c",
        "/%",
        "/%0",
        "/%GG",
        "/%C0%AF",
        "/%ED%A0%80",
        "/%F4%90%80%80",
        "/%E2%82%AC",
        "/%00x",
        "/é/中文",
        "/a\0b",
        "/a ",
        "/a\tb\rc\nd",
        "\\a\\..\\b",
        "//a?query/../x",
        "//a#fragment/../x",
        "?query",
        "#fragment",
    ];
    let mut inputs: Vec<JsString> = hosts
        .iter()
        .flat_map(|host| {
            paths
                .iter()
                .map(move |path| JsString::from(format!("file://{host}{path}")))
        })
        .collect();
    inputs.extend([
        JsString::from("file:///raw/") + JsString::from_utf16(vec![0xd800]),
        JsString::from("file://") + JsString::from_utf16(vec![0xdfff]) + "/x",
    ]);
    compare_normalized_urls(&inputs);
}

fn compare_normalized_urls(inputs: &[JsString]) {
    let mut source = oracle::TsOracle::spawn("paths-boundaries-oracle.ts", "packages/coding-agent/src/utils/paths.ts");
    let mut mismatches = Vec::new();
    for batch in inputs.chunks(100) {
        let cases = batch
            .iter()
            .map(|input| pi_js_compat::js_json!({"kind":"normalize","input":input}))
            .collect();
        let expected = source.request(&pi_js_compat::js_json!({"id":1,"cases":JsValue::Array(cases)}));
        for (input, expected) in batch.iter().zip(expected["output"]["results"].as_array().unwrap()) {
            let actual = match normalize_path(input, &PathInputOptions::default()) {
                Ok(value) => pi_js_compat::js_json!({"ok":true,"value":value}),
                Err(error) => {
                    pi_js_compat::js_json!({"ok":false,"error":error.native_error().expect("Node error identity").value()})
                }
            };
            if &actual != expected {
                mismatches.push(format!(
                    "{input:?}: native={}, Node={}",
                    pi_js_compat::js_json::stringify_lossless(&actual),
                    pi_js_compat::js_json::stringify_lossless(expected)
                ));
            }
        }
    }
    assert!(
        mismatches.is_empty(),
        "{} mismatches / {} cases:\n{}",
        mismatches.len(),
        inputs.len(),
        mismatches.join("\n")
    );
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
