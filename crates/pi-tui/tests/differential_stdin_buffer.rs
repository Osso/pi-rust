use crate::shared_oracle as oracle;

use std::time::{Duration, Instant};

use pi_tui::stdin_buffer::{StdinBuffer, StdinBufferEvent, StdinBufferOptions};
use serde_json::{Value, json};

const SOURCE_DIRECTORY: &str = "packages/tui/src";

fn cases() -> Vec<Vec<String>> {
    let long_sequence = format!("\x1b[{}H", "1;".repeat(50));
    let raw: Vec<Vec<&str>> = vec![
        vec!["a"],
        vec!["abc"],
        vec!["hello 世界"],
        vec!["\x1b[<35;20;5m"],
        vec!["\x1b[A"],
        vec!["\x1b[11~"],
        vec!["\x1ba"],
        vec!["\x1bOA"],
        vec!["\x1b", "[<35", ";20;5m"],
        vec!["\x1b[", "1;", "5H"],
        vec!["\x1b", "[", "<", "3", "5", ";", "2", "0", ";", "5", "m"],
        vec!["\x1b[<35"],
        vec!["\x1b"],
        vec!["abc\x1b[A"],
        vec!["\x1b[Aabc"],
        vec!["\x1b[A\x1b[B\x1b[C"],
        vec!["abc\x1b[<35", ";20;5m"],
        vec!["\x1b[97u"],
        vec!["\x1b[97;1:3u"],
        vec!["\x1b[97u\x1b[97;1:3u"],
        vec!["\x1b[97u\x1b[97;1:3u\x1b[98u\x1b[98;1:3u"],
        vec!["\x1b[1;1:1A"],
        vec!["\x1b[3;1:3~"],
        vec!["\x1b\x1b[27;129:3u"],
        vec!["\x1b\x1b[27;1:3u"],
        vec!["\x1b\x1b"],
        vec!["\x1b[224uà"],
        vec!["\x1b[64u", "@"],
        vec!["\x1b[97ub"],
        vec!["\x1b[64;3u@"],
        vec!["\x1b[<0;10;5M"],
        vec!["\x1b[<0;10;5m"],
        vec!["\x1b[<35;20;5m"],
        vec!["\x1b[<3", "5;1", "5;", "10m"],
        vec!["\x1b[<35;1;1m\x1b[<35;2;2m\x1b[<35;3;3m"],
        vec!["\x1b[M abc"],
        vec!["\x1b[M", " a", "b"],
        vec![""],
        vec!["\x1b[200~hello world\x1b[201~"],
        vec!["\x1b[200~", "hello ", "world\x1b[201~"],
        vec!["a", "\x1b[200~pasted\x1b[201~", "b"],
        vec!["\x1b[200~line1\nline2\nline3\x1b[201~"],
        vec!["\x1b[200~Hello 世界 🎉\x1b[201~"],
    ];
    let mut result: Vec<Vec<String>> = raw
        .into_iter()
        .map(|chunks| chunks.into_iter().map(str::to_string).collect())
        .collect();
    result.push(vec![long_sequence]);
    result
}

fn options() -> StdinBufferOptions {
    StdinBufferOptions {
        timeout: Duration::from_millis(10),
    }
}

fn event_json(event: &StdinBufferEvent) -> Value {
    match event {
        StdinBufferEvent::Data(value) => json!({ "kind": "data", "value": value }),
        StdinBufferEvent::Paste(value) => json!({ "kind": "paste", "value": value }),
    }
}

#[test]
fn stdin_buffer_matches_the_pinned_typescript_source() {
    let cases = cases();
    let mut oracle = oracle::TsOracle::spawn("tui-leaves-oracle.ts", SOURCE_DIRECTORY);
    let request: Vec<Value> = cases.iter().map(|chunks| json!({ "chunks": chunks })).collect();
    let response = oracle.request(&json!({
        "id": "stdin-buffer",
        "operation": "stdin_buffer",
        "cases": request,
    }));
    assert_eq!(response["ok"], true, "oracle failed: {response}");
    let results = response["output"]["results"].as_array().unwrap();

    for (chunks, expected) in cases.iter().zip(results) {
        let mut buffer = StdinBuffer::with_options(options());
        let mut events = Vec::new();
        for chunk in chunks {
            events.extend(buffer.process(chunk));
        }
        assert_eq!(
            buffer.buffer(),
            expected["bufferAfterProcess"].as_str().unwrap(),
            "buffer after process mismatch for chunks {chunks:?}"
        );

        events.extend(buffer.poll(Instant::now() + Duration::from_secs(1)));
        let expected_events: Vec<Value> = events.iter().map(event_json).collect();
        assert_eq!(
            Value::Array(expected_events),
            expected["events"],
            "events mismatch for chunks {chunks:?}"
        );
        assert_eq!(
            buffer.buffer(),
            expected["bufferAfterTimeout"].as_str().unwrap(),
            "buffer after timeout mismatch for chunks {chunks:?}"
        );
    }
}

#[test]
fn frozen_timeout_applies_to_escape_and_partial_sequences() {
    let mut oracle = oracle::TsOracle::spawn("tui-leaves-oracle.ts", "packages/tui/src");
    let cases = json!([
        {"chunks": ["\u{1b}[" ]},
        {"chunks": ["\u{1b}"], "timeout": 100},
    ]);
    let response = oracle.request(&json!({"id": "timeouts", "operation": "stdin_buffer", "cases": cases}));
    assert_eq!(response["ok"], true, "{response}");
    for (index, (chunk, custom)) in [("\x1b[", None), ("\x1b", Some(100))].into_iter().enumerate() {
        let mut buffer = custom.map_or_else(StdinBuffer::new, |millis| {
            StdinBuffer::with_options(StdinBufferOptions {
                timeout: Duration::from_millis(millis),
            })
        });
        assert!(buffer.process(chunk).is_empty());
        let events = buffer.poll(Instant::now() + Duration::from_millis(30));
        let expected = &response["output"]["results"][index];
        assert_eq!(buffer.buffer(), expected["bufferAfterTimeout"].as_str().unwrap());
        assert_eq!(events.len(), expected["events"].as_array().unwrap().len());
    }
}
