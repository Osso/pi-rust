use std::sync::{Arc, Mutex};
use std::time::Duration;

use pi_coding_agent::modes::rpc::{attach_jsonl_line_reader, serialize_json_line};
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;

type CollectedLines = Arc<Mutex<Vec<String>>>;

fn collector() -> (
    CollectedLines,
    impl FnMut(String) -> Result<(), pi_ai::request::CallbackError> + Send + 'static,
) {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&lines);
    (lines, move |line| {
        sink.lock().unwrap().push(line);
        Ok(())
    })
}

async fn read_all(input: &'static [u8]) -> Vec<String> {
    let (lines, on_line) = collector();
    let reader = attach_jsonl_line_reader(input, on_line);
    reader.finished().await.expect("in-memory reads cannot fail");
    Arc::try_unwrap(lines).unwrap().into_inner().unwrap()
}

#[tokio::test]
async fn splits_on_lf_only_and_preserves_unicode_separators_inside_payloads() {
    let record = serialize_json_line(&json!({"text": "a\u{2028}b\u{2029}c"})).unwrap();
    let input: &'static [u8] = Box::leak(record.into_bytes().into_boxed_slice());

    let lines = read_all(input).await;

    assert_eq!(lines.len(), 1);
    let parsed: Value = serde_json::from_str(&lines[0]).unwrap();
    assert_eq!(parsed, json!({"text": "a\u{2028}b\u{2029}c"}));
}

#[tokio::test]
async fn handles_crlf_delimited_input() {
    assert_eq!(
        read_all(b"{\"a\":1}\r\n{\"b\":2}\r\n").await,
        ["{\"a\":1}", "{\"b\":2}"]
    );
}

#[tokio::test]
async fn emits_a_final_line_without_trailing_lf() {
    assert_eq!(read_all(b"{\"a\":1}").await, ["{\"a\":1}"]);
}

#[tokio::test]
async fn decodes_utf8_split_across_reads() {
    let (mut writer, reader) = tokio::io::duplex(64);
    let (lines, on_line) = collector();
    let handle = attach_jsonl_line_reader(reader, on_line);

    writer.write_all(&[b'x', 0xF0, 0x9F]).await.unwrap();
    writer.flush().await.unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    writer.write_all(&[0x98, 0x80, b'\n', 0xE2]).await.unwrap();
    drop(writer);
    handle.finished().await.unwrap();

    assert_eq!(*lines.lock().unwrap(), ["x😀", "\u{fffd}"]);
}

#[tokio::test]
async fn detach_stops_delivering_later_chunks_and_the_final_line() {
    let (mut writer, reader) = tokio::io::duplex(64);
    let (lines, on_line) = collector();
    let handle = attach_jsonl_line_reader(reader, on_line);

    writer.write_all(b"one\npartial").await.unwrap();
    for _ in 0..50 {
        if !lines.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    handle.detach();
    let _ = writer.write_all(b"\ntwo\n").await;
    drop(writer);
    handle.finished().await.unwrap();

    assert_eq!(*lines.lock().unwrap(), ["one"]);
}
