use crate::shared_oracle as oracle;

use pi_agent_core::harness::utils::{
    TruncationOptions, format_size, sanitize_binary_output, truncate_head, truncate_line, truncate_tail,
};
use pi_js_compat::js_json::{JsString, parse_lossless, to_value};

#[test]
fn numeric_limits_preserve_the_full_frozen_number_domain() {
    let mut oracle = oracle::TsOracle::spawn(
        "agent-core-utils-oracle.ts",
        "packages/agent-core/src/harness/utils/truncate.ts",
    );
    for token in [
        "1.5",
        "-0.5",
        "-1",
        "NaN",
        "Infinity",
        "-Infinity",
        "-0",
        "1023.9",
        "1280",
        "1048575",
        "1e21",
    ] {
        let number: f64 = token.parse().unwrap();
        let options = TruncationOptions {
            max_lines: number,
            max_bytes: number,
        };
        let content = JsString::from("a\nb\nc");
        let line = truncate_line(&content, Some(number));
        let actual = pi_js_compat::js_json!({
            "head": to_value(&truncate_head(&content, options)).unwrap(),
            "tail": to_value(&truncate_tail(&content, options)).unwrap(),
            "line": {"text": line.text, "wasTruncated": line.was_truncated},
            "sanitized": sanitize_binary_output(&content),
            "formattedSize": format_size(number),
        });
        let expected = oracle.request(&serde_json::json!({"units": content.units(), "maxLines": token, "maxBytes": token, "maxChars": token, "formatBytes": token}));
        let actual_wire = pi_js_compat::js_json::stringify_lossless(&actual);
        assert_eq!(
            parse_lossless(&actual_wire).unwrap(),
            parse_lossless(expected["wire"].as_str().unwrap()).unwrap(),
            "number {token}"
        );
    }
}

#[test]
fn native_units_and_empty_line_metadata_match_frozen() {
    let mut oracle = oracle::TsOracle::spawn(
        "agent-core-utils-oracle.ts",
        "packages/agent-core/src/harness/utils/truncate.ts",
    );
    let values = [
        JsString::default(),
        JsString::from("a\n"),
        JsString::from("😀tail"),
        JsString::from_utf16(vec![0xd800, 0xa, 0xdc00, 0x1, 0xd83d, 0xde00, 0xfff9]),
        JsString::from_utf16(vec![0x61, 0xdfff]),
    ];
    for content in values {
        for (max_lines, max_bytes, max_chars) in [(0, 0, 0), (1, 3, 1), (2, 5, 2), (2000, 10240, 500)] {
            let options = TruncationOptions {
                max_lines: max_lines as f64,
                max_bytes: max_bytes as f64,
            };
            let line = truncate_line(&content, Some(max_chars as f64));
            let actual = pi_js_compat::js_json!({
                "head": to_value(&truncate_head(&content, options)).unwrap(),
                "tail": to_value(&truncate_tail(&content, options)).unwrap(),
                "line": {"text": line.text, "wasTruncated": line.was_truncated},
                "sanitized": sanitize_binary_output(&content),
            });
            let expected = oracle.request(&serde_json::json!({
                "units": content.units(), "maxLines": max_lines, "maxBytes": max_bytes, "maxChars": max_chars,
            }));
            assert_eq!(
                actual,
                parse_lossless(expected["wire"].as_str().unwrap()).unwrap(),
                "input {content:?}, options {options:?}"
            );
        }
    }
}
