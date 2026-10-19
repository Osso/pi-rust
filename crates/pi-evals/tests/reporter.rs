//! Behavioral tests for `evals/src/vitest-evals/reporter.ts`.

use std::fs;
use tokio::sync::Mutex;

use pi_evals::harness::{HarnessRun, NormalizedSession, TimingSummary, UsageSummary};
use pi_evals::vitest_evals::artifacts::PI_SESSION_SNAPSHOT_ARTIFACT;
use pi_evals::vitest_evals::harness_table::EVAL_HARNESS_ITERATION_ARTIFACT;
use pi_evals::vitest_evals::reporter::{
    EvalHarnessReporter, EvalMeta, EvalTestCase, HarnessMeta, TestModule, TestRunEndReason, TestState,
    collect_harness_observations,
};
use pi_evals::vitest_evals::setup::record_eval_session_artifact_after_run;
use pi_evals::vitest_evals::summary::HarnessObservationOutcome;
use pi_js_compat::js_json;
use pi_js_compat::process_env;
use serde_json::{Map, Value, json};

fn iteration_artifact(harness: &str, repetition: f64) -> Value {
    json!({
        "schemaVersion": 1,
        "evalSet": "set",
        "harness": harness,
        "baseline": "baseline",
        "candidates": ["candidate"],
        "repetition": repetition,
        "groupKey": "[\"input\",1]",
    })
}

fn usage(tokens: f64) -> UsageSummary {
    UsageSummary {
        provider: Some("faux".to_owned()),
        model: Some("faux-1".to_owned()),
        input_tokens: Some(tokens),
        output_tokens: Some(5.0),
        reasoning_tokens: None,
        total_tokens: Some(tokens),
        tool_calls: Some(1.0),
        retries: None,
        metadata: Some(Map::from_iter([
            ("cacheReadTokens".to_owned(), json!(0)),
            ("cacheWriteTokens".to_owned(), json!(0)),
            ("estimatedCostUsd".to_owned(), json!(0)),
        ])),
    }
}

fn test_case(harness: &str, run_id: &str, tokens: f64, score: f64) -> EvalTestCase {
    let run = HarnessRun {
        output: json!("Paris"),
        session: NormalizedSession::default(),
        usage: usage(tokens),
        timings: Some(TimingSummary {
            total_ms: Some(12.5),
            metadata: None,
        }),
        artifacts: Some(Map::from_iter([
            ("runId".to_owned(), json!(run_id)),
            (
                PI_SESSION_SNAPSHOT_ARTIFACT.to_owned(),
                json!("{\"type\":\"session\"}\n"),
            ),
            (
                EVAL_HARNESS_ITERATION_ARTIFACT.to_owned(),
                iteration_artifact(harness, 1.0),
            ),
        ])),
        errors: Vec::new(),
    };
    EvalTestCase {
        id: format!("test-{harness}"),
        file: "src/smoke.eval.ts".to_owned(),
        name: "runs".to_owned(),
        full_name: "Pi smoke > runs".to_owned(),
        state: TestState::Passed,
        artifacts: Vec::new(),
        harness: Some(HarnessMeta {
            name: harness.to_owned(),
            run,
        }),
        eval: Some(EvalMeta { avg_score: Some(score) }),
    }
}

fn expected_record(harness: &str, run_id: &str, tokens: f64, hash: &str) -> Value {
    json!({
        "schemaVersion": 1,
        "runId": run_id,
        "test": {
            "id": format!("test-{harness}"),
            "file": "src/smoke.eval.ts",
            "name": "runs",
            "fullName": "Pi smoke > runs",
            "status": "passed",
        },
        "harness": harness,
        "usage": {
            "provider": "faux",
            "model": "faux-1",
            "inputTokens": tokens,
            "outputTokens": 5,
            "totalTokens": tokens,
            "toolCalls": 1,
            "metadata": {
                "cacheReadTokens": 0,
                "cacheWriteTokens": 0,
                "estimatedCostUsd": 0,
            },
        },
        "timings": {"totalMs": 12.5},
        "artifacts": [{
            "name": "session.jsonl",
            "path": format!("sessions/{hash}/session.jsonl"),
        }],
        "metadata": {
            "vitestEvalsHarnessIteration": iteration_artifact(harness, 1.0),
        },
    })
}

async fn write_record(test: &EvalTestCase) {
    EvalHarnessReporter.on_test_case_result(test).await.expect("run record");
}

/// `PI_EVAL_ARTIFACT_DIR` is process-global; serialize the reporter tests.
static ENV_LOCK: std::sync::LazyLock<Mutex<()>> = std::sync::LazyLock::new(|| Mutex::new(()));

#[tokio::test]
async fn reporter_writes_runs_and_the_comparison_report() {
    let _guard = ENV_LOCK.lock().await;
    let directory = tempfile::tempdir().expect("artifact directory");
    process_env::set("PI_EVAL_ARTIFACT_DIR", &directory.path().display().to_string());

    let mut baseline = test_case("baseline", "run-1", 10.0, 0.0);
    record_eval_session_artifact_after_run(&mut baseline).expect("session artifact");
    let mut candidate = test_case("candidate", "run-2", 20.0, 1.0);
    record_eval_session_artifact_after_run(&mut candidate).expect("session artifact");

    let reporter = EvalHarnessReporter;
    reporter.on_test_case_result(&baseline).await.expect("baseline report");
    reporter
        .on_test_case_result(&candidate)
        .await
        .expect("candidate report");

    let lines = fs::read_to_string(directory.path().join("runs.jsonl")).expect("runs.jsonl");
    let lines: Vec<&str> = lines.lines().collect();
    assert_eq!(lines.len(), 2);
    let baseline_hash = "4e65d3fbe8ad6535681b021b30785b12b6c0e3f8878859a4148b3f58b8835db0";
    let candidate_hash = "b9d17232a4c83d9928ee0d4e6fab81d3802cd3bf898149558e1abf8b24e1ba49";
    assert_eq!(
        lines[0],
        js_json::stringify(&expected_record("baseline", "run-1", 10.0, baseline_hash))
    );
    assert_eq!(
        lines[1],
        js_json::stringify(&expected_record("candidate", "run-2", 20.0, candidate_hash))
    );
    assert_eq!(
        fs::read_to_string(
            directory
                .path()
                .join("sessions")
                .join(baseline_hash)
                .join("session.jsonl")
        )
        .expect("persisted session"),
        "{\"type\":\"session\"}\n"
    );

    let modules = vec![TestModule {
        relative_module_id: "src/smoke.eval.ts".to_owned(),
        tests: vec![baseline, candidate],
    }];
    let observations = collect_harness_observations(&modules);
    assert_eq!(observations.len(), 2);
    assert_eq!(observations[0].base.harness, "baseline");
    assert_eq!(observations[1].base.harness, "candidate");

    let report = reporter
        .on_test_run_end(&modules, TestRunEndReason::Passed)
        .expect("comparison report")
        .expect("comparison report is present");
    assert!(report.starts_with("\nEval Comparisons"), "{report}");
    assert!(report.contains(" Baseline  baseline"), "{report}");
    assert!(report.contains("Candidate  candidate (1/1 pairs)"), "{report}");
    assert!(
        report.contains("Pass rate  +100.0 pp (candidate 100.0%, baseline 0.0%)"),
        "{report}"
    );
    assert!(
        report.contains("Tokens  +10.0 (candidate 20.0, baseline 10.0)"),
        "{report}"
    );
    assert_eq!(
        reporter
            .on_test_run_end(&modules, TestRunEndReason::Interrupted)
            .expect("interrupted report"),
        Some("\nEval comparisons unavailable: test run interrupted.".to_owned())
    );

    process_env::remove("PI_EVAL_ARTIFACT_DIR");
    write_record(&modules[0].tests[0]).await;
    assert_eq!(
        fs::read_to_string(directory.path().join("runs.jsonl"))
            .expect("runs.jsonl")
            .lines()
            .count(),
        2
    );
}

#[tokio::test]
async fn reporter_includes_run_errors_and_marks_harness_errors() {
    let _guard = ENV_LOCK.lock().await;
    let directory = tempfile::tempdir().expect("artifact directory");
    process_env::set("PI_EVAL_ARTIFACT_DIR", &directory.path().display().to_string());
    let mut test = test_case("baseline", "run-3", 10.0, 0.0);
    if let Some(harness) = test.harness.as_mut() {
        harness
            .run
            .errors
            .push(Map::from_iter([("message".to_owned(), json!("harness exploded"))]));
    }
    write_record(&test).await;
    let line = fs::read_to_string(directory.path().join("runs.jsonl")).expect("runs.jsonl");
    let record: Value = serde_json::from_str(line.trim()).expect("record JSON");
    assert_eq!(record["errors"], json!([{"message": "harness exploded"}]), "{record}");
    assert!(record["metadata"].get("vitestEvalsHarnessIteration").is_some());

    let module = TestModule {
        relative_module_id: "src/smoke.eval.ts".to_owned(),
        tests: vec![test],
    };
    let observations = collect_harness_observations(&[module]);
    assert_eq!(observations[0].outcome, HarnessObservationOutcome::Errored);
    process_env::remove("PI_EVAL_ARTIFACT_DIR");
}
