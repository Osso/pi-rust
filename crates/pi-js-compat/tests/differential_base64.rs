//! Differential proof for Base64 `atob`/`btoa` and Node `Buffer`.

use crate::shared_oracle_2 as oracle;

use pi_js_compat::base64::{atob, btoa, buffer_from_base64, buffer_to_base64};
use serde_json::json;

const VALUES: &[&str] = &[
    "",
    "A",
    "AA",
    "AAA",
    "AAAA",
    "AAAAA",
    "AAAAAA",
    "QQ==",
    "QUI=",
    "QUJD",
    "AA==AAAA",
    "AAA=AAAA",
    "A A A A",
    "!!!!",
    "A-_A",
    "-_+/AQ==",
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFBQIAX8jx0gAAAABJRU5ErkJggg==",
    "/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAA==",
    "QUJD\nREVG",
    "QUJD\r\nREVG",
    "QUJDRA",
    "=",
    "AA=",
    "AA=A",
    "Q Q = =",
    "AA",
    "QUI",
    "A===",
    "A=",
    "=AAA",
    "QUJDRA==",
    "SGVsbG8sIHdvcmxkIQ==",
    "8J+YgA==",
];

const BYTES: &[&str] = &[
    "",
    "00",
    "41",
    "4142",
    "414243",
    "000000",
    "03efc0",
    "fbffbf01",
    "48656c6c6f2c20776f726c6421",
    "f09f9880",
    "ff00ff00",
];

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
fn matches_node_base64_semantics() {
    let mut oracle = oracle::TsOracle::spawn("js-compat-oracle.ts", "packages/coding-agent/src/index.ts");
    let response = oracle.request(&json!({
        "id": 11,
        "operation": "base64",
        "values": VALUES,
        "bytes": BYTES,
    }));
    assert_eq!(response["ok"], true, "oracle failed: {response}");

    let expected_atob: Vec<String> = response["output"]["atob"]
        .as_array()
        .expect("atob array")
        .iter()
        .map(|value| value.as_str().expect("string").to_owned())
        .collect();
    let actual_atob: Vec<String> = VALUES
        .iter()
        .map(|value| match atob(&(*value).into()) {
            Ok(bytes) => hex(&bytes),
            Err(_) => "ERR".to_owned(),
        })
        .collect();
    assert_eq!(actual_atob, expected_atob, "atob");

    let expected_buffer: Vec<String> = response["output"]["buffer"]
        .as_array()
        .expect("buffer array")
        .iter()
        .map(|value| value.as_str().expect("string").to_owned())
        .collect();
    let actual_buffer: Vec<String> = VALUES.iter().map(|value| hex(&buffer_from_base64(*value))).collect();
    assert_eq!(actual_buffer, expected_buffer, "Buffer.from(value, 'base64')");

    let expected_btoa: Vec<String> = response["output"]["btoa"]
        .as_array()
        .expect("btoa array")
        .iter()
        .map(|value| value.as_str().expect("string").to_owned())
        .collect();
    let actual_btoa: Vec<String> = BYTES
        .iter()
        .map(|value| {
            let bytes = decode_hex(value);
            btoa(&bytes)
        })
        .collect();
    assert_eq!(actual_btoa, expected_btoa, "btoa");

    let actual_buffer_to: Vec<String> = BYTES.iter().map(|value| buffer_to_base64(&decode_hex(value))).collect();
    assert_eq!(actual_buffer_to, expected_btoa, "Buffer#toString('base64')");
}

fn decode_hex(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .chunks(2)
        .map(|pair| {
            let text = std::str::from_utf8(pair).expect("hex");
            u8::from_str_radix(text, 16).expect("hex")
        })
        .collect()
}
