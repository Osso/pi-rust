use crate::shared_oracle as oracle;
use pi_js_compat::js_json::{JsString, parse_lossless};
use pi_tui::stdin_buffer::{StdinBuffer, StdinBufferEvent};
use serde_json::json;
#[test]
fn stdin_astral_plain_text_emits_source_code_units_not_scalar_characters() {
    let mut oracle = oracle::TsOracle::spawn("raw-tui-oracle.ts", "packages/tui/src");
    let expected = oracle.request(&json!({"operation":"stdin_native","chunks":[JsString::from("😀").units()]}));
    assert_eq!(expected["ok"], true, "{expected}");
    let mut buffer = StdinBuffer::new();
    let events: Vec<_> = buffer
        .process("😀")
        .into_iter()
        .map(|event| match event {
            StdinBufferEvent::Data(value) => ("data", value),
            StdinBufferEvent::Paste(value) => ("paste", value),
        })
        .collect();
    let actual = serde_json::to_string(&(vec![(events, JsString::from(buffer.buffer()))], buffer.flush())).unwrap();
    assert_eq!(
        parse_lossless(actual).unwrap(),
        parse_lossless(expected["json"].as_str().unwrap()).unwrap()
    );
}

#[test]
fn raw_chunk_boundaries_meta_mouse_osc_paste_and_kitty_duplicates_match_source() {
    let mut oracle = oracle::TsOracle::spawn("raw-tui-oracle.ts", "packages/tui/src");
    for raw in [
        JsString::from_utf16(vec![0xd800]),
        JsString::from_utf16(vec![0xdc00]),
        JsString::from("😀"),
        JsString::from_utf16(vec![97, 0xd800, 0x301, 98, 0xdc00]),
    ] {
        for sequence in [
            raw.clone(),
            JsString::from("\x1b") + &raw,
            JsString::from("\x1b[M") + &raw + "ab",
            JsString::from("\x1b]0;") + &raw + "\x07",
            JsString::from("\x1bP") + &raw + "\x1b\\",
            JsString::from("\x1b_") + &raw + "\x1b\\",
            JsString::from("a\x1b[200~") + &raw + "\x1b[201~" + &raw,
            JsString::from("\x1b[55296u") + &raw + "\x1b[56320u" + &raw,
            JsString::from("\x1b\x1b[27u") + &raw + "\x1b[<35;20;5m",
        ] {
            for split_units in [false, true] {
                let chunks = if split_units {
                    sequence
                        .units()
                        .iter()
                        .map(|unit| JsString::from_utf16(vec![*unit]))
                        .collect::<Vec<_>>()
                } else {
                    vec![sequence.clone()]
                };
                let expected=oracle.request(&json!({"operation":"stdin_native","chunks":chunks.iter().map(JsString::units).collect::<Vec<_>>()}));
                assert_eq!(expected["ok"], true, "{expected}");
                let mut buffer = StdinBuffer::new();
                let mut states = Vec::new();
                for chunk in &chunks {
                    let events: Vec<_> = buffer
                        .process(chunk)
                        .into_iter()
                        .map(|event| match event {
                            StdinBufferEvent::Data(value) => ("data", value),
                            StdinBufferEvent::Paste(value) => ("paste", value),
                        })
                        .collect();
                    states.push((events, buffer.buffer().clone()));
                }
                let actual = serde_json::to_string(&(states, buffer.flush())).unwrap();
                assert_eq!(
                    parse_lossless(actual).unwrap(),
                    parse_lossless(expected["json"].as_str().unwrap()).unwrap(),
                    "raw={raw:?} sequence={sequence:?} split={split_units}"
                );
            }
        }
    }
}

#[test]
fn process_terminal_routes_native_stdin_units_and_paste_without_encoding() {
    use pi_tui::terminal::{ProcessTerminal, TerminalEvent};
    let mut terminal = ProcessTerminal::with_output(Box::new(std::io::sink()));
    terminal.begin_keyboard_protocol_negotiation();
    terminal.feed_stdin(JsString::from_utf16(vec![0xd800, 0xdc00]));
    terminal.feed_stdin(JsString::from("\x1b[200~") + JsString::from_utf16(vec![0xd800]) + "\x1b[201~");
    let inputs: Vec<_> = terminal
        .poll_at(std::time::Instant::now())
        .into_iter()
        .filter_map(|event| match event {
            TerminalEvent::Input(value) => Some(value),
            _ => None,
        })
        .collect();
    assert_eq!(
        inputs,
        vec![
            JsString::from_utf16(vec![0xd800]),
            JsString::from_utf16(vec![0xdc00]),
            JsString::from("\x1b[200~") + JsString::from_utf16(vec![0xd800]) + "\x1b[201~"
        ]
    );
}
