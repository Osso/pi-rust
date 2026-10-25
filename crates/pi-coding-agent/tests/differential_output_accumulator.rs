use crate::shared_corpus as corpus;
use crate::shared_oracle as oracle;

use corpus::Corpus;
use pi_coding_agent::core::tools::output_accumulator::{OutputAccumulator, OutputAccumulatorOptions, SnapshotOptions};
use pi_coding_agent::core::tools::truncate::{TruncatedBy, TruncationResult};
use pi_js_compat::js_json::{JsString, JsValue};
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
        assert_eq!(
            JsValue::from(actual.clone()),
            JsValue::from(expected.clone()),
            "case {index} ({})",
            cases[index].name
        );
    }
}

#[test]
fn frozen_accepts_binary64_accumulator_limits() {
    let cases = numeric_requests();
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-output-accumulator-oracle.ts",
        "packages/coding-agent/src/core/tools/output-accumulator.ts",
    );
    let expected = oracle.request(&json!({"id": 27004, "cases": cases}));
    assert_eq!(expected["ok"], true, "{expected}");
    let outputs = expected["output"].as_array().unwrap();
    assert_eq!(outputs.len(), cases.len());
    for (input, output) in cases.iter().zip(outputs) {
        assert_eq!(
            output["limits"],
            json!({"maxLines": input["maxLines"], "maxBytes": input["maxBytes"]}),
            "{output}"
        );
        assert_eq!(
            output["tempFileHex"],
            if output["hasFullOutputPath"] == true {
                json!(hex("a€\nb😀c\nlast".as_bytes()))
            } else {
                Value::Null
            }
        );
    }
}

#[test]
fn native_binary64_limits_match_frozen_snapshots_and_spill_bytes() {
    let cases = numeric_requests();
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-output-accumulator-oracle.ts",
        "packages/coding-agent/src/core/tools/output-accumulator.ts",
    );
    let expected = oracle.request(&json!({"id": 27005, "cases": cases}));
    assert_eq!(expected["ok"], true, "{expected}");
    let directory = tempfile::tempdir().unwrap();
    let mut mismatches = Vec::new();
    for (input, expected) in cases.iter().zip(expected["output"].as_array().unwrap()) {
        let path = directory.path().join(input["name"].as_str().unwrap());
        let mut output = OutputAccumulator::new_with_temp_file_path(
            OutputAccumulatorOptions {
                max_lines: decode_limit(&input["maxLines"]),
                max_bytes: decode_limit(&input["maxBytes"]),
                ..OutputAccumulatorOptions::default()
            },
            path.to_str().unwrap(),
        );
        output.append("a€\nb😀c\nlast".as_bytes()).unwrap();
        let first_line_bytes = output.last_line_bytes();
        output.finish().unwrap();
        let last_line_bytes = output.last_line_bytes();
        let snapshot = output
            .snapshot(SnapshotOptions {
                persist_if_truncated: true,
            })
            .unwrap();
        let limits = json!({"maxLines": encode_limit(snapshot.truncation.max_lines), "maxBytes": encode_limit(snapshot.truncation.max_bytes)});
        let bytes =
            pi_js_compat::js_json::stringify_lossless(&pi_js_compat::js_json::to_value(&snapshot.truncation).unwrap());
        output.close_temp_file().unwrap();
        let actual = json!({
            "name": input["name"], "content": snapshot.content,
            "truncation": truncation_json(snapshot.truncation), "truncationJson": bytes,
            "limits": limits, "hasFullOutputPath": snapshot.full_output_path.is_some(),
            "tempFileHex": snapshot.full_output_path.map(|_| hex(&std::fs::read(&path).unwrap())),
            "lastLineBytes": [first_line_bytes, last_line_bytes], "appendError": null,
        });
        if JsValue::from(actual.clone()) != JsValue::from(expected.clone()) {
            mismatches.push(json!({"name": input["name"], "actual": actual, "expected": expected}));
        }
    }
    assert!(mismatches.is_empty(), "{}", json!(mismatches));
}

#[test]
fn default_and_custom_artifact_names_match_the_frozen_crypto_hex_format() {
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-output-accumulator-oracle.ts",
        "packages/coding-agent/src/core/tools/output-accumulator.ts",
    );
    for prefix in [None, Some("output-π")] {
        let mut case = json!({"name": "artifact-format", "maxBytes": 0, "chunks": [[120]], "finish": true, "persistIfTruncated": true, "includeFullPath": true});
        if let Some(prefix) = prefix {
            case["tempFilePrefix"] = json!(prefix);
        }
        let expected = oracle.request(&json!({"id": 27007, "cases": [case]}));
        assert_eq!(expected["ok"], true, "{expected}");
        let mut options = OutputAccumulatorOptions {
            max_bytes: 0.0,
            ..Default::default()
        };
        if let Some(prefix) = prefix {
            options.temp_file_prefix = prefix.into();
        }
        let mut output = OutputAccumulator::new(options);
        output.append(b"x").unwrap();
        output.finish().unwrap();
        let snapshot = output.snapshot(SnapshotOptions::default()).unwrap();
        output.close_temp_file().unwrap();
        let path = snapshot.full_output_path.unwrap();
        let scalar = path.to_utf8().unwrap();
        for path in [
            scalar.as_str(),
            expected["output"][0]["fullOutputPath"].as_str().unwrap(),
        ] {
            let filename = path.rsplit('/').next().unwrap();
            let suffix = filename
                .strip_prefix(&format!("{}-", prefix.unwrap_or("pi-output")))
                .unwrap()
                .strip_suffix(".log")
                .unwrap();
            assert_eq!(suffix.len(), 16);
            assert!(suffix.bytes().all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f')));
        }
        assert_eq!(std::fs::read(&scalar).unwrap(), b"x");
        assert_eq!(expected["output"][0]["tempFileHex"], "78");
        std::fs::remove_file(&scalar).unwrap();
    }
}

#[tokio::test(flavor = "current_thread")]
async fn default_spill_preserves_unpaired_prefix_units_until_the_os_boundary() {
    let prefix = JsString::from("raw-") + JsString::from_utf16(vec![0xd800]);
    let mut request = JsValue::from(json!({"id": 27011, "cases": [{"name": "raw-prefix", "maxBytes": 0,
        "chunks": [[120]], "finish": true, "persistIfTruncated": true, "includeFullPath": true}]}));
    request["cases"].as_array_mut().unwrap()[0]["tempFilePrefix"] = JsValue::String(prefix.clone());
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-output-accumulator-oracle.ts",
        "packages/coding-agent/src/core/tools/output-accumulator.ts",
    );
    let expected = oracle.request(&request);
    assert_eq!(expected["ok"], JsValue::Bool(true));
    let JsValue::String(expected_path) = &expected["output"][0]["fullOutputPath"] else {
        panic!("raw path")
    };
    assert!(expected_path.units().contains(&0xd800));
    let mut output = OutputAccumulator::new(OutputAccumulatorOptions {
        max_bytes: 0.0,
        temp_file_prefix: prefix,
        ..Default::default()
    });
    output.append(b"x").unwrap();
    output.finish().unwrap();
    let path = output
        .snapshot(SnapshotOptions::default())
        .unwrap()
        .full_output_path
        .unwrap();
    output.close_temp_file().unwrap();
    assert_eq!(
        &path.units()[..path.len() - 20],
        &expected_path.units()[..expected_path.len() - 20]
    );
    assert!(path.units().contains(&0xd800));
    assert_eq!(
        std::fs::read(pi_js_compat::js_json::encode_node_utf8(&path)).unwrap(),
        b"x"
    );
    assert_eq!(expected["output"][0]["tempFileHex"], JsValue::String("78".into()));
    std::fs::remove_file(pi_js_compat::js_json::encode_node_utf8(&path)).unwrap();
}

fn decode_limit(value: &Value) -> f64 {
    match value.as_str() {
        Some("NaN") => f64::NAN,
        Some("Infinity") => f64::INFINITY,
        Some("-Infinity") => f64::NEG_INFINITY,
        Some("-0") => -0.0,
        Some(other) => panic!("unknown numeric tag {other}"),
        None => value.as_f64().unwrap(),
    }
}

fn encode_limit(value: f64) -> Value {
    if value.is_nan() {
        return json!("NaN");
    }
    if value == f64::INFINITY {
        return json!("Infinity");
    }
    if value == f64::NEG_INFINITY {
        return json!("-Infinity");
    }
    if value == 0.0 && value.is_sign_negative() {
        return json!("-0");
    }
    json!(value)
}

fn numeric_requests() -> Vec<Value> {
    [
        (json!(-1), json!(-1)),
        (json!(0), json!(0)),
        (json!("-0"), json!("-0")),
        (json!(1.25), json!(1.25)),
        (json!(2.75), json!(2.75)),
        (json!(0.25), json!(0.25)),
        (json!("NaN"), json!(1)),
        (json!(1), json!("NaN")),
        (json!("Infinity"), json!("Infinity")),
        (json!("-Infinity"), json!(1)),
        (json!(1), json!("-Infinity")),
        (json!(1e30), json!(1e30)),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (lines, bytes))| {
        json!({
            "name": format!("number-{index}"), "maxLines": lines, "maxBytes": bytes,
            "chunks": ["a€\nb😀c\nlast".as_bytes()], "finish": true, "persistIfTruncated": true
        })
    })
    .collect()
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
            max_lines: case.max_lines as f64,
            max_bytes: case.max_bytes as f64,
            ..OutputAccumulatorOptions::default()
        },
        JsString::from(path.to_str().expect("UTF-8 temp path")),
    );
    let mut last_line_bytes = Vec::new();
    for chunk in &case.chunks {
        accumulator.append(chunk).unwrap();
        last_line_bytes.push(accumulator.last_line_bytes());
    }
    if case.finish {
        accumulator.finish().unwrap();
        last_line_bytes.push(accumulator.last_line_bytes());
    }
    let append_error = case.append_after_finish.then(|| {
        accumulator
            .append(b"a")
            .err()
            .map(|error| error.message.to_utf8().unwrap())
            .unwrap_or_default()
    });
    let snapshot = accumulator
        .snapshot(SnapshotOptions {
            persist_if_truncated: case.persist_if_truncated,
        })
        .unwrap();
    let temp_file_hex = snapshot.full_output_path.as_ref().map(|_| {
        accumulator.close_temp_file().expect("temp file must close");
        std::fs::read(&path).map(|bytes| hex(&bytes)).unwrap_or_default()
    });

    let truncation_json_bytes = pi_js_compat::js_json::stringify_lossless(
        &pi_js_compat::js_json::to_value(&snapshot.truncation).expect("truncation metadata serializes"),
    );
    json!({
        "name": case.name,
        "content": snapshot.content,
        "truncation": truncation_json(snapshot.truncation),
        "truncationJson": truncation_json_bytes,
        "limits": {"maxLines": case.max_lines, "maxBytes": case.max_bytes},
        "hasFullOutputPath": snapshot.full_output_path.is_some(),
        "tempFileHex": temp_file_hex,
        "lastLineBytes": last_line_bytes,
        "appendError": append_error,
    })
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
