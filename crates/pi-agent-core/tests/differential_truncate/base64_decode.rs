use pi_js_compat::base64::buffer_from_base64;
use serde_json::{Value, json};

use super::{oracle::TsOracle, support::ok_response};

const REQUEST_ID: u64 = 13_100;

/// Base64 payloads covering the alphabet, padding, length remainders, and
/// characters `Buffer.from(value, "base64")` ignores or stops at.
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
];

pub(crate) fn compare(oracle: &mut TsOracle) {
    let expected = oracle.request(&request());
    let actual = response();
    assert_eq!(actual, expected, "base64 decode mismatch");
}

fn request() -> Value {
    json!({"id": REQUEST_ID, "operation": "base64_decode", "values": VALUES})
}

fn response() -> Value {
    let results = VALUES
        .iter()
        .map(|value| hex(&buffer_from_base64(value)))
        .collect::<Vec<_>>();
    ok_response(REQUEST_ID, json!({"results": results}))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
