#[path = "support/corpus.rs"]
mod corpus;
#[path = "support/oracle.rs"]
mod oracle;

use corpus::Corpus;
use pi_coding_agent::core::tools::output_accumulator::{OutputAccumulator, OutputAccumulatorOptions, SnapshotOptions};
use pi_coding_agent::core::tools::truncate::{TruncatedBy, TruncationResult};
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};

struct Case {
    name: String,
    max_lines: usize,
    max_bytes: usize,
    chunks: Vec<Vec<u8>>,
    finish: bool,
    persist_if_truncated: bool,
    append_after_finish: bool,
}

#[test]
fn coding_agent_output_accumulator_matches_the_pinned_typescript_source() {
    let temp = tempfile::tempdir().expect("temp dir");
    let cases = cases();
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-output-accumulator-oracle.ts",
        "packages/coding-agent/src/core/tools/output-accumulator.ts",
    );
    let expected = oracle.request(&json!({
        "id": 27_003,
        "operation": "coding_agent_output_accumulator",
        "cases": cases.iter().map(request_case).collect::<Vec<_>>(),
    }));
    let actual = json!({"id": 27_003, "ok": true, "output": cases.iter().map(|case| actual_case(case, temp.path())).collect::<Vec<_>>()});

    assert_eq!(expected["ok"], Value::Bool(true), "{expected}");
    let expected_results = expected["output"].as_array().expect("oracle output array");
    let actual_results = actual["output"].as_array().expect("actual output array");
    assert_eq!(expected_results.len(), cases.len());
    for (index, (actual, expected)) in actual_results.iter().zip(expected_results).enumerate() {
        assert_eq!(actual, expected, "case {index} ({})", cases[index].name);
    }
}

fn request_case(case: &Case) -> Value {
    json!({
        "name": case.name,
        "maxLines": case.max_lines,
        "maxBytes": case.max_bytes,
        "chunks": case.chunks,
        "finish": case.finish,
        "persistIfTruncated": case.persist_if_truncated,
        "appendAfterFinish": case.append_after_finish,
    })
}

fn actual_case(case: &Case, temp_dir: &std::path::Path) -> Value {
    let path = temp_dir.join(format!("{}.log", case.name));
    let mut accumulator = OutputAccumulator::new_with_temp_file_path(
        OutputAccumulatorOptions {
            max_lines: case.max_lines,
            max_bytes: case.max_bytes,
            ..OutputAccumulatorOptions::default()
        },
        JsString::from(path.to_str().expect("UTF-8 temp path")),
    );
    let mut last_line_bytes = Vec::new();
    for chunk in &case.chunks {
        accumulator.append(chunk);
        last_line_bytes.push(accumulator.last_line_bytes());
    }
    if case.finish {
        accumulator.finish();
        last_line_bytes.push(accumulator.last_line_bytes());
    }
    let append_error = case.append_after_finish.then(|| {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| accumulator.append(b"a")))
            .err()
            .map(panic_message)
            .unwrap_or_default()
    });
    let snapshot = accumulator.snapshot(SnapshotOptions {
        persist_if_truncated: case.persist_if_truncated,
    });
    let temp_file_hex = snapshot.full_output_path.as_ref().map(|_| {
        accumulator.close_temp_file().expect("temp file must close");
        std::fs::read(&path).map(|bytes| hex(&bytes)).unwrap_or_default()
    });

    json!({
        "name": case.name,
        "content": snapshot.content,
        "truncation": truncation_json(snapshot.truncation),
        "hasFullOutputPath": snapshot.full_output_path.is_some(),
        "tempFileHex": temp_file_hex,
        "lastLineBytes": last_line_bytes,
        "appendError": append_error,
    })
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    match payload.downcast_ref::<String>() {
        Some(message) => message.clone(),
        None => match payload.downcast_ref::<&str>() {
            Some(message) => (*message).to_owned(),
            None => String::new(),
        },
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn truncation_json(result: TruncationResult) -> Value {
    let truncated_by = result.truncated_by.map(|value| match value {
        TruncatedBy::Lines => "lines",
        TruncatedBy::Bytes => "bytes",
    });
    json!({
        "content": result.content,
        "truncated": result.truncated,
        "truncatedBy": truncated_by,
        "totalLines": result.total_lines,
        "totalBytes": result.total_bytes,
        "outputLines": result.output_lines,
        "outputBytes": result.output_bytes,
        "lastLinePartial": result.last_line_partial,
        "firstLineExceedsLimit": result.first_line_exceeds_limit,
        "maxLines": result.max_lines,
        "maxBytes": result.max_bytes,
    })
}

fn cases() -> Vec<Case> {
    let mut cases = targeted_cases();
    let mut corpus = Corpus::new(0x6f75_7470);
    let byte_alphabet = [
        vec![b'a'],
        vec![b'\n'],
        vec![b'\r'],
        vec![0xE2, 0x82, 0xAC],
        vec![0xF0, 0x9F, 0x98, 0x80],
        vec![0xFF],
        vec![0xE2],
        vec![0x80],
        vec![0xEF, 0xBB, 0xBF],
        vec![b' '],
    ];
    for index in 0..120 {
        let chunk_count = corpus.below(5) + 1;
        let chunks = (0..chunk_count)
            .map(|_| {
                let mut chunk = Vec::new();
                for _ in 0..corpus.below(6) {
                    chunk.extend_from_slice(&byte_alphabet[corpus.below(byte_alphabet.len())]);
                }
                chunk
            })
            .collect();
        cases.push(Case {
            name: format!("corpus-{index}"),
            max_lines: corpus.below(4) + 1,
            max_bytes: corpus.below(12) + 1,
            chunks,
            finish: true,
            persist_if_truncated: corpus.below(2) == 0,
            append_after_finish: false,
        });
    }
    cases
}

fn targeted_cases() -> Vec<Case> {
    let case = |name: &str, chunks: Vec<Vec<u8>>| Case {
        name: name.to_owned(),
        max_lines: 2_000,
        max_bytes: 10_240,
        chunks,
        finish: true,
        persist_if_truncated: false,
        append_after_finish: false,
    };
    vec![
        case("empty", vec![]),
        case("ascii", vec![b"one\ntwo\nthree".to_vec()]),
        case("split-euro", vec![vec![b'a', 0xE2, 0x82], vec![0xAC, b'b']]),
        case("bom-whole", vec![vec![0xEF, 0xBB, 0xBF, b'a']]),
        case("bom-split", vec![vec![0xEF], vec![0xBB], vec![0xBF]]),
        case("bom-later", vec![vec![b'A', 0xEF, 0xBB, 0xBF]]),
        case("invalid-lead", vec![vec![0xFF, b'a']]),
        case("invalid-second", vec![vec![0xE2, 0x28, 0xA1]]),
        case("incomplete-end", vec![vec![0xE2, 0x82]]),
        case("overlong", vec![vec![0xC0, 0x80]]),
        case("surrogate", vec![vec![0xED, 0xA0, 0x80]]),
        case("out-of-range", vec![vec![0xF4, 0x90, 0x80, 0x80]]),
        case("empty-chunk", vec![vec![], vec![b'a'], vec![]]),
        case("crlf", vec![b"a\r\nb\r\n".to_vec()]),
        Case {
            name: "truncated-lines".to_owned(),
            max_lines: 2,
            max_bytes: 1_024,
            chunks: vec![b"a\nb\nc\n".to_vec()],
            finish: true,
            persist_if_truncated: true,
            append_after_finish: false,
        },
        Case {
            name: "truncated-bytes".to_owned(),
            max_lines: 100,
            max_bytes: 4,
            chunks: vec![b"abcdef\ngh".to_vec(), b"ij".to_vec()],
            finish: true,
            persist_if_truncated: true,
            append_after_finish: false,
        },
        Case {
            name: "rolling-tail".to_owned(),
            max_lines: 3,
            max_bytes: 16,
            chunks: vec![b"line-0\nline-1\nline-2\nline-3\nline-4\nline-5".to_vec()],
            finish: true,
            persist_if_truncated: false,
            append_after_finish: false,
        },
        Case {
            name: "append-after-finish".to_owned(),
            max_lines: 2_000,
            max_bytes: 10_240,
            chunks: vec![b"a\n".to_vec()],
            finish: true,
            persist_if_truncated: false,
            append_after_finish: true,
        },
        Case {
            name: "no-finish".to_owned(),
            max_lines: 2_000,
            max_bytes: 10_240,
            chunks: vec![b"a\nb".to_vec(), vec![0xE2]],
            finish: false,
            persist_if_truncated: false,
            append_after_finish: false,
        },
    ]
}
