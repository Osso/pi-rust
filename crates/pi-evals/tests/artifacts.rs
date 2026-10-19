//! Behavioral tests for `evals/src/vitest-evals/artifacts.ts` and `setup.ts`.

use pi_evals::harness::{HarnessRun, NormalizedSession, UsageSummary};
use pi_evals::vitest_evals::artifacts::{
    PI_SESSION_SNAPSHOT_ARTIFACT, TestArtifact, TestAttachment, record_eval_session_artifact,
    record_eval_source_artifact,
};
use pi_evals::vitest_evals::reporter::{EvalTestCase, HarnessMeta, TestState};
use pi_evals::vitest_evals::setup::record_eval_session_artifact_after_run;
use serde_json::{Map, Value, json};

fn run_with_artifacts(artifacts: Map<String, Value>) -> HarnessRun {
    HarnessRun {
        output: Value::Null,
        session: NormalizedSession::default(),
        usage: UsageSummary::default(),
        timings: None,
        artifacts: Some(artifacts),
        errors: Vec::new(),
    }
}

fn test_case(run: HarnessRun) -> EvalTestCase {
    EvalTestCase {
        id: "test-1".to_owned(),
        file: "src/extensions.eval.ts".to_owned(),
        name: "creates an extension".to_owned(),
        full_name: "Pi extension authoring > creates an extension".to_owned(),
        state: TestState::Passed,
        artifacts: Vec::new(),
        harness: Some(HarnessMeta {
            name: "default-system-prompt".to_owned(),
            run,
        }),
        eval: None,
    }
}

#[test]
fn records_session_and_source_artifacts() {
    let run = run_with_artifacts(Map::from_iter([
        ("runId".to_owned(), json!("run-1")),
        (
            PI_SESSION_SNAPSHOT_ARTIFACT.to_owned(),
            json!("{\"type\":\"session\"}\n"),
        ),
    ]));
    let mut artifacts: Vec<TestArtifact> = Vec::new();
    record_eval_session_artifact(&mut artifacts, &run).expect("session artifact");
    record_eval_source_artifact(
        &mut artifacts,
        "run-1",
        TestAttachment::new("hello.ts", "text/typescript", "export default function () {}\n"),
    );

    assert_eq!(artifacts.len(), 2);
    assert_eq!(artifacts[0].artifact_type, "@earendil-works/pi-evals:session");
    assert_eq!(artifacts[0].run_id.as_deref(), Some("run-1"));
    let attachment = &artifacts[0].attachments.as_ref().expect("attachments")[0];
    assert_eq!(attachment.name, "session.jsonl");
    assert_eq!(attachment.content_type, "application/jsonl");
    assert_eq!(attachment.body, "{\"type\":\"session\"}\n");
    assert_eq!(attachment.body_encoding, "utf-8");

    assert_eq!(artifacts[1].artifact_type, "@earendil-works/pi-evals:source");
    assert_eq!(artifacts[1].run_id.as_deref(), Some("run-1"));
    let attachment = &artifacts[1].attachments.as_ref().expect("attachments")[0];
    assert_eq!(attachment.name, "hello.ts");
    assert_eq!(attachment.content_type, "text/typescript");
    assert_eq!(attachment.body, "export default function () {}\n");
}

#[test]
fn skips_recording_without_a_session_snapshot() {
    let mut artifacts: Vec<TestArtifact> = Vec::new();
    record_eval_session_artifact(&mut artifacts, &run_with_artifacts(Map::new())).expect("no-op");
    record_eval_session_artifact(
        &mut artifacts,
        &run_with_artifacts(Map::from_iter([("runId".to_owned(), json!("run-1"))])),
    )
    .expect("no-op");
    assert!(artifacts.is_empty());
}

#[test]
fn rejects_invalid_session_artifact_metadata() {
    let mut artifacts: Vec<TestArtifact> = Vec::new();
    let run = run_with_artifacts(Map::from_iter([
        ("runId".to_owned(), json!(5)),
        (PI_SESSION_SNAPSHOT_ARTIFACT.to_owned(), json!("snapshot")),
    ]));
    let error = record_eval_session_artifact(&mut artifacts, &run).expect_err("invalid metadata");
    assert_eq!(error.name(), "TypeError");
    assert_eq!(error.message(), "Pi eval session artifact metadata is invalid.");
}

#[test]
fn setup_records_the_harness_session_snapshot() {
    let run = run_with_artifacts(Map::from_iter([
        ("runId".to_owned(), json!("run-2")),
        (
            PI_SESSION_SNAPSHOT_ARTIFACT.to_owned(),
            json!("{\"type\":\"session\"}\n"),
        ),
    ]));
    let mut test = test_case(run);
    record_eval_session_artifact_after_run(&mut test).expect("session artifact");
    assert_eq!(test.artifacts.len(), 1);
    assert_eq!(test.artifacts[0].run_id.as_deref(), Some("run-2"));

    let mut test = test_case(run_with_artifacts(Map::new()));
    test.harness = None;
    record_eval_session_artifact_after_run(&mut test).expect("no harness");
    assert!(test.artifacts.is_empty());
}
