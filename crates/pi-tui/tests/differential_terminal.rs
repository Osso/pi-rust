use crate::shared_oracle as oracle;

use std::io::Write;
use std::sync::{Arc, Mutex};

use oracle::TsOracle;
use pi_tui::terminal::{
    KeyboardProtocolNegotiationSequence, ProcessTerminal, Terminal, TerminalRawModeError, is_terminal_raw_mode_failure,
    normalize_apple_terminal_input, parse_keyboard_protocol_negotiation_sequence,
};
use serde_json::{Value, json};

struct Capture(Arc<Mutex<Vec<u8>>>);
impl Write for Capture {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(data);
        Ok(data.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn rust_terminal(request: &Value) -> Value {
    let sequences: Vec<Value> = request["sequences"]
        .as_array()
        .map(|cases| {
            cases
                .iter()
                .map(
                    |case| match parse_keyboard_protocol_negotiation_sequence(case.as_str().unwrap()) {
                        Some(KeyboardProtocolNegotiationSequence::KittyFlags { flags }) => {
                            let number = if flags < u64::MAX as f64 {
                                json!(flags as u64)
                            } else {
                                json!(flags)
                            };
                            json!({"type": "kitty-flags", "flags": number})
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
                    let output = normalize_apple_terminal_input(data, flag, shift);
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
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let mut terminal = ProcessTerminal::with_output(Box::new(Capture(Arc::clone(&bytes))));
    terminal.set_progress(true);
    terminal.set_progress(false);
    let progress = String::from_utf8(bytes.lock().unwrap().clone()).unwrap();
    json!({
        "progress": progress,
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
            "sequences": ["\u{1b}[?7u", "\u{1b}[?62;4;52c", "\u{1b}[?u", "\u{1b}[?0u", "\u{1b}[7u", "\u{1b}[?12;3c", "\u{1b}[?4294967296u", "\u{1b}[?18446744073709551616u"],
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
