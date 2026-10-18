#[path = "support/oracle.rs"]
mod oracle;

use std::collections::BTreeMap;

use oracle::TsOracle;
use pi_tui::terminal::{
    KeyboardProtocolNegotiationSequence, TerminalRawModeError, is_terminal_raw_mode_failure,
    normalize_apple_terminal_input, normalize_native_shift_enter_input, parse_keyboard_protocol_negotiation_sequence,
    resolve_escape_timeout_ms,
};
use serde_json::{Value, json};

fn rust_terminal(request: &Value) -> Value {
    let env: BTreeMap<String, String> = request["env"]
        .as_object()
        .map(|entries| {
            entries
                .iter()
                .map(|(key, value)| (key.clone(), value.as_str().unwrap_or("").to_string()))
                .collect()
        })
        .unwrap_or_default();
    let escape_timeout = resolve_escape_timeout_ms(&|name| env.get(name).cloned());
    let sequences: Vec<Value> = request["sequences"]
        .as_array()
        .map(|cases| {
            cases
                .iter()
                .map(
                    |case| match parse_keyboard_protocol_negotiation_sequence(case.as_str().unwrap()) {
                        Some(KeyboardProtocolNegotiationSequence::KittyFlags { flags }) => {
                            json!({"type": "kitty-flags", "flags": flags})
                        }
                        Some(KeyboardProtocolNegotiationSequence::DeviceAttributes) => {
                            json!({"type": "device-attributes"})
                        }
                        None => Value::Null,
                    },
                )
                .collect()
        })
        .unwrap_or_default();
    let normalize: Vec<Value> = request["normalize"]
        .as_array()
        .map(|cases| {
            cases
                .iter()
                .map(|case| {
                    let data = case["data"].as_str().unwrap();
                    let flag = case["flag"].as_bool().unwrap();
                    let shift = case["shift"].as_bool().unwrap();
                    let output = if case["kind"].as_str() == Some("native") {
                        normalize_native_shift_enter_input(data, flag, shift)
                    } else {
                        normalize_apple_terminal_input(data, flag, shift)
                    };
                    json!(output)
                })
                .collect()
        })
        .unwrap_or_default();
    let raw_mode: Vec<Value> = request["rawModeErrors"]
        .as_array()
        .map(|cases| {
            cases
                .iter()
                .map(|case| {
                    let typed =
                        TerminalRawModeError::new(case["flag"].as_bool().unwrap(), case["message"].as_str().unwrap());
                    let plain = std::io::Error::other(case["message"].as_str().unwrap());
                    json!({
                        "message": typed.to_string(),
                        "typed": is_terminal_raw_mode_failure(&typed),
                        "plain": is_terminal_raw_mode_failure(&plain),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let escape_timeout_value = if escape_timeout.fract() == 0.0 {
        json!(escape_timeout as u64)
    } else {
        json!(escape_timeout)
    };
    json!({
        "escapeTimeout": escape_timeout_value,
        "sequences": sequences,
        "normalize": normalize,
        "rawMode": raw_mode,
    })
}

#[test]
fn terminal_helpers_match_typescript() {
    let mut oracle = TsOracle::spawn("tui-core-oracle.ts", "packages/tui/src");
    let env_cases = vec![
        json!({}),
        json!({"PI_TUI_ESC_TIMEOUT": "80"}),
        json!({"PI_TUI_ESC_TIMEOUT": "80", "SSH_TTY": "/dev/pts/1"}),
        json!({"PI_TUI_ESC_TIMEOUT": "abc"}),
        json!({"PI_TUI_ESC_TIMEOUT": "0"}),
        json!({"PI_TUI_ESC_TIMEOUT": "-5"}),
        json!({"PI_TUI_ESC_TIMEOUT": ""}),
        json!({"PI_TUI_ESC_TIMEOUT": "1e2"}),
        json!({"SSH_CONNECTION": "10.0.0.1 22"}),
        json!({"SSH_TTY": "/dev/pts/1"}),
    ];
    for env in env_cases {
        let request = json!({
            "id": "terminal",
            "operation": "terminal",
            "env": env,
            "sequences": ["\u{1b}[?7u", "\u{1b}[?62;4;52c", "\u{1b}[?u", "\u{1b}[?0u", "\u{1b}[7u", "\u{1b}[?12;3c"],
            "normalize": [
                {"kind": "native", "data": "\r", "flag": true, "shift": true},
                {"kind": "native", "data": "\r", "flag": false, "shift": true},
                {"kind": "native", "data": "\r", "flag": true, "shift": false},
                {"kind": "native", "data": "a", "flag": true, "shift": true},
                {"kind": "apple", "data": "\r", "flag": true, "shift": true},
                {"kind": "apple", "data": "\u{1b}[13;2u", "flag": true, "shift": true},
            ],
            "rawModeErrors": [
                {"flag": true, "message": "setRawMode failed with errno: 5"},
                {"flag": false, "message": "boom"},
                {"flag": true, "message": "other failure"},
            ],
        });
        let response = oracle.request(&request);
        assert_eq!(response["ok"], Value::Bool(true), "oracle failed: {response}");
        assert_eq!(
            rust_terminal(&request),
            response["output"],
            "terminal helper output differs for env {env}"
        );
    }
}
