//! Differential tests for `evals/src/vitest-evals/harness-table.ts`.

#[path = "support/oracle.rs"]
mod support;

use pi_coding_agent::utils::js_json;
use pi_evals::error::PiEvalError;
use pi_evals::harness::{Harness, HarnessRun, NormalizedSession, TranscriptEvent, TranscriptRole};
use pi_evals::vitest_evals::harness_table::{
    EvalHarnessTableCandidatesOptions, EvalHarnessTableOptions, EvalHarnessTablePairOptions, derive_eval_group_key,
    eval_harness_table, parse_eval_harness_iteration_artifact,
};
use serde_json::{Map, Value, json};

const SOURCE_MODULE: &str = "packages/evals/src/vitest-evals/harness-table.ts";

fn fake_harness(name: &str) -> Harness<Value> {
    let harness_name = name.to_owned();
    Harness::new(name, move |input, _context| {
        let harness_name = harness_name.clone();
        Box::pin(async move {
            Ok(HarnessRun {
                output: json!({ "harness": harness_name, "input": input }),
                session: NormalizedSession {
                    events: vec![TranscriptEvent::Message {
                        role: TranscriptRole::User,
                        content: Some(Value::String(harness_name.clone())),
                        metadata: None,
                    }],
                },
                usage: Default::default(),
                timings: None,
                artifacts: Some(Map::new()),
                errors: Vec::new(),
            })
        })
    })
}

fn group_key_fixtures() -> Vec<(Value, f64)> {
    vec![
        (json!({"id": " input-1 ", "prompt": "hello"}), 2.0),
        (json!({"first": 1, "second": [true, "value"]}), 1.0),
        (json!({"second": [true, "value"], "first": 1}), 1.0),
        (json!({"first": 1}), 1.0),
        (json!({"first": 2}), 1.0),
        (json!({"first": 1}), 2.0),
        (json!(["first", "second"]), 1.0),
        (json!(["second", "first"]), 1.0),
        (json!("plain string"), 1.0),
        (json!(42), 1.0),
        (json!(1.5), 3.0),
        (json!(true), 1.0),
        (Value::Null, 1.0),
        (json!({"id": 5, "value": "number id"}), 1.0),
        (json!({"id": "   ", "value": "blank id"}), 1.0),
        (json!({"id": "", "value": "empty id"}), 1.0),
        (json!({"nested": {"b": [1, 2, {"d": 4, "c": 3}], "a": null}}), 7.0),
        (json!({"10": "ten", "2": "two", "a": "letter"}), 1.0),
        (json!({"Z": 1, "\u{FFFF}": 2, "\u{1F600}": 3, "a": 4}), 1.0),
        (json!({"exponent": 1e21, "negativeZero": -0.0, "fraction": 0.1}), 1.0),
    ]
}

fn artifact_fixtures() -> Vec<Option<Value>> {
    vec![
        Some(json!({
            "schemaVersion": 1,
            "evalSet": "set",
            "groupKey": "[\"input\",1]",
            "harness": "candidate",
            "baseline": "baseline",
            "candidates": ["candidate"],
            "repetition": 1,
        })),
        Some(json!({
            "schemaVersion": 1,
            "evalSet": "set",
            "groupKey": "[\"input\",1]",
            "harness": "candidate",
            "baseline": "baseline",
            "candidates": ["candidate", "second"],
            "repetition": 2.5,
        })),
        Some(json!({"schemaVersion": 2})),
        Some(json!({"schemaVersion": "1"})),
        Some(json!({"schemaVersion": 1, "candidates": [1]})),
        Some(json!({"schemaVersion": 1, "candidates": []})),
        Some(json!({"schemaVersion": 1, "repetition": "1"})),
        Some(Value::Array(vec![json!(1)])),
        Some(Value::Null),
        None,
    ]
}

#[test]
fn derive_eval_group_key_matches_the_typescript_module() {
    let mut oracle = support::TsOracle::spawn("evals-harness-table-oracle.ts", SOURCE_MODULE);
    for (index, (input, repetition)) in group_key_fixtures().into_iter().enumerate() {
        let request = json!({
            "id": index,
            "operation": "group_key",
            "input": input,
            "repetition": repetition,
        });
        let response = oracle.request(&request);
        assert_eq!(response["ok"], true, "fixture {index}: {response}");
        let expected = response["output"]["value"]
            .as_str()
            .unwrap_or_else(|| panic!("fixture {index} must return a group key"));
        let actual = derive_eval_group_key(&input, repetition).expect("group key derivation must succeed");
        assert_eq!(actual, expected, "fixture {index}");
    }
}

#[test]
fn parse_eval_harness_iteration_artifact_matches_the_typescript_module() {
    let mut oracle = support::TsOracle::spawn("evals-harness-table-oracle.ts", SOURCE_MODULE);
    for (index, value) in artifact_fixtures().into_iter().enumerate() {
        let request = match &value {
            Some(value) => json!({ "id": index, "operation": "parse_artifact", "value": value }),
            None => json!({ "id": index, "operation": "parse_artifact" }),
        };
        let response = oracle.request(&request);
        assert_eq!(response["ok"], true, "fixture {index}: {response}");
        let expected = &response["output"]["parsed"];
        let actual = parse_eval_harness_iteration_artifact(value.as_ref())
            .map(|artifact| serde_json::to_value(artifact).expect("artifacts serialize"))
            .unwrap_or(Value::Null);
        assert_eq!(
            js_json::stringify(&actual),
            js_json::stringify(expected),
            "fixture {index}"
        );
    }
}

fn plan_request(
    id: &str,
    eval_set: &str,
    baseline: &str,
    candidates: &[&str],
    repetitions: Option<u32>,
    input: Value,
) -> Value {
    let mut request = json!({
        "id": id,
        "operation": "table_plan",
        "evalSet": eval_set,
        "baseline": baseline,
        "candidates": candidates,
        "input": input,
    });
    if let Some(repetitions) = repetitions {
        request["repetitions"] = json!(repetitions);
    }
    request
}

fn plan_rows(response: &Value) -> &Vec<Value> {
    response["output"]["rows"]
        .as_array()
        .unwrap_or_else(|| panic!("plan must return rows: {response}"))
}

#[test]
fn eval_harness_table_plan_matches_the_typescript_module() {
    let mut oracle = support::TsOracle::spawn("evals-harness-table-oracle.ts", SOURCE_MODULE);
    let cases = [
        (
            "multi",
            plan_request(
                "multi",
                "local multi-harness eval",
                "withoutSkill",
                &["withSkill"],
                Some(2),
                json!({"id": "first"}),
            ),
            vec!["withoutSkill", "withSkill", "withoutSkill", "withSkill"],
            vec![1_u32, 1, 2, 2],
        ),
        (
            "singular",
            plan_request(
                "singular",
                "singular candidate",
                "baseline",
                &["candidate"],
                None,
                json!({"id": "first"}),
            ),
            vec!["baseline", "candidate"],
            vec![1_u32, 1],
        ),
        (
            "two candidates",
            plan_request(
                "two candidates",
                "two candidates",
                "first",
                &["second", "third"],
                Some(1),
                json!("plain input"),
            ),
            vec!["first", "second", "third"],
            vec![1_u32, 1, 1],
        ),
    ];
    for (name, request, expected_names, expected_repetitions) in cases {
        let response = oracle.request(&request);
        assert_eq!(response["ok"], true, "{name}: {response}");
        let rows = plan_rows(&response);
        let options = match request["candidates"].as_array().map(Vec::len) {
            Some(1) => EvalHarnessTableOptions::Pair(EvalHarnessTablePairOptions {
                baseline: fake_harness(request["baseline"].as_str().expect("baseline name")),
                candidate: fake_harness(request["candidates"][0].as_str().expect("candidate name")),
                repetitions: request["repetitions"].as_u64().map(|value| value as u32),
            }),
            _ => EvalHarnessTableOptions::Candidates(EvalHarnessTableCandidatesOptions {
                baseline: fake_harness(request["baseline"].as_str().expect("baseline name")),
                candidates: request["candidates"]
                    .as_array()
                    .expect("candidate names")
                    .iter()
                    .map(|name| fake_harness(name.as_str().expect("candidate name")))
                    .collect(),
                repetitions: request["repetitions"].as_u64().map(|value| value as u32),
            }),
        };
        let input = request["input"].clone();
        let rows_actual =
            eval_harness_table(request["evalSet"].as_str().expect("eval set"), options).expect("planning must succeed");
        assert_eq!(
            rows_actual.iter().map(|row| row.name.as_str()).collect::<Vec<_>>(),
            expected_names
        );
        assert_eq!(
            rows_actual.iter().map(|row| row.repetition).collect::<Vec<_>>(),
            expected_repetitions
        );
        assert_eq!(rows_actual.len(), rows.len(), "{name}: row count");
        for (index, (row, expected)) in rows_actual.iter().zip(rows.iter()).enumerate() {
            let context = pi_evals::harness::HarnessContext::new();
            let run = block_on_ready(row.harness.run(input.clone(), context)).expect("fake harness run must succeed");
            assert_eq!(
                js_json::stringify(&run.output),
                js_json::stringify(&expected["output"]),
                "{name} row {index}: output"
            );
            assert_eq!(
                js_json::stringify(&Value::Object(run.artifacts.clone().unwrap_or_default())),
                js_json::stringify(&expected["artifacts"]),
                "{name} row {index}: artifacts"
            );
        }
    }
}

/// Runs one already-ready future without adding a runtime dependency to tests.
fn block_on_ready<F: std::future::Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let waker = std::task::Waker::noop();
    let mut context = std::task::Context::from_waker(waker);
    loop {
        match future.as_mut().poll(&mut context) {
            std::task::Poll::Ready(output) => return output,
            std::task::Poll::Pending => std::thread::yield_now(),
        }
    }
}

#[test]
fn eval_harness_table_rejects_invalid_options_like_typescript() {
    let mut oracle = support::TsOracle::spawn("evals-harness-table-oracle.ts", SOURCE_MODULE);
    let cases = [
        (
            "empty eval set",
            plan_request("empty", "   ", "baseline", &["candidate"], None, json!("input")),
            "evalSet must not be empty.",
        ),
        (
            "no candidates",
            plan_request("none", "set", "baseline", &[], None, json!("input")),
            "At least one candidate harness is required.",
        ),
        (
            "duplicate names",
            plan_request("duplicate", "set", "same", &["same"], None, json!("input")),
            "Harness names must be unique within an eval set.",
        ),
        (
            "zero repetitions",
            plan_request("zero", "set", "baseline", &["candidate"], Some(0), json!("input")),
            "repetitions must be a positive integer.",
        ),
    ];
    for (name, request, expected_message) in cases {
        let response = oracle.request(&request);
        assert_eq!(response["ok"], false, "{name}: {response}");
        assert_eq!(response["error"]["name"], "TypeError", "{name}");
        assert_eq!(response["error"]["message"], expected_message, "{name}");

        let candidates: Vec<&str> = request["candidates"]
            .as_array()
            .expect("candidate names")
            .iter()
            .map(|name| name.as_str().expect("candidate name"))
            .collect();
        let result = eval_harness_table(
            request["evalSet"].as_str().expect("eval set"),
            EvalHarnessTableOptions::Candidates(EvalHarnessTableCandidatesOptions {
                baseline: fake_harness(request["baseline"].as_str().expect("baseline name")),
                candidates: candidates.iter().map(|name| fake_harness(name)).collect(),
                repetitions: request["repetitions"].as_u64().map(|value| value as u32),
            }),
        );
        let error = match result {
            Ok(_) => panic!("{name}: invalid options must be rejected"),
            Err(error) => error,
        };
        assert!(matches!(error, PiEvalError::TypeError(_)), "{name}: {error:?}");
        assert_eq!(error.message(), expected_message, "{name}");
    }
}
