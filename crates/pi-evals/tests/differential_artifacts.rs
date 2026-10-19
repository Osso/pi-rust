//! Differential tests for `evals/src/vitest-evals/artifacts.ts` persistence.

#[path = "support/oracle.rs"]
mod support;

use std::fs;

use pi_coding_agent::utils::js_json;
use pi_evals::vitest_evals::artifacts::{TestArtifact, persist_eval_artifact_references};
use serde_json::{Value, json};

const SOURCE_MODULE: &str = "packages/evals/src/vitest-evals/artifacts.ts";

fn artifact_fixtures() -> Vec<(&'static str, Vec<Value>, &'static str)> {
    vec![
        (
            "matching run",
            vec![
                json!({
                    "type": "@earendil-works/pi-evals:session",
                    "runId": "run-1",
                    "attachments": [{
                        "name": "session.jsonl",
                        "contentType": "application/jsonl",
                        "body": "{\"type\":\"session\"}\n",
                        "bodyEncoding": "utf-8",
                    }],
                }),
                json!({
                    "type": "@earendil-works/pi-evals:session",
                    "runId": "run-2",
                    "attachments": [],
                }),
                json!({
                    "type": "@earendil-works/pi-evals:source",
                    "runId": "run-1",
                    "attachments": [{
                        "name": "hello.ts",
                        "contentType": "text/typescript",
                        "body": "export default function () {}\n",
                        "bodyEncoding": "utf-8",
                    }],
                }),
                json!({"type": "internal:annotation", "annotation": {"message": "other", "type": "info"}}),
            ],
            "run-1",
        ),
        (
            "multiple attachments",
            vec![json!({
                "type": "@earendil-works/pi-evals:source",
                "runId": "run-3",
                "attachments": [
                    {
                        "name": "first.ts",
                        "contentType": "text/typescript",
                        "body": "first\n",
                        "bodyEncoding": "utf-8",
                    },
                    {
                        "name": "second.ts",
                        "contentType": "text/typescript",
                        "body": "second\n",
                        "bodyEncoding": "utf-8",
                    },
                ],
            })],
            "run-3",
        ),
        ("no matching artifacts", vec![], "run-4"),
        (
            "unicode run id",
            vec![json!({
                "type": "@earendil-works/pi-evals:session",
                "runId": "run-\u{00e9}\u{1F600}",
                "attachments": [{
                    "name": "session.jsonl",
                    "contentType": "application/jsonl",
                    "body": "body\n",
                    "bodyEncoding": "utf-8",
                }],
            })],
            "run-\u{00e9}\u{1F600}",
        ),
    ]
}

#[test]
fn persisted_artifact_references_match_the_typescript_module() {
    let mut oracle = support::TsOracle::spawn("evals-artifacts-oracle.ts", SOURCE_MODULE);
    for (index, (name, artifacts, run_id)) in artifact_fixtures().into_iter().enumerate() {
        let request = json!({
            "id": index,
            "operation": "persist",
            "artifacts": artifacts,
            "runId": run_id,
        });
        let response = oracle.request(&request);
        assert_eq!(response["ok"], true, "{name}: {response}");
        let expected_references = &response["output"]["references"];
        let expected_contents = response["output"]["contents"]
            .as_array()
            .expect("contents array")
            .clone();

        let directory = tempfile::tempdir().expect("artifact directory");
        let artifacts: Vec<TestArtifact> = artifacts
            .into_iter()
            .map(|value| serde_json::from_value(value).expect("artifact fixture"))
            .collect();
        let references = persist_eval_artifact_references(&artifacts, run_id, &directory.path().display().to_string())
            .expect("persistence must succeed");
        assert_eq!(
            js_json::stringify(&serde_json::to_value(&references).expect("references serialize")),
            js_json::stringify(expected_references),
            "{name}: references"
        );
        for expected in expected_contents {
            let path = expected["path"].as_str().expect("artifact path");
            let body = fs::read_to_string(directory.path().join(path)).expect("persisted artifact body");
            assert_eq!(
                body,
                expected["body"].as_str().expect("artifact body"),
                "{name}: {path}"
            );
        }
    }
}

#[test]
fn invalid_artifact_names_match_the_typescript_error() {
    let mut oracle = support::TsOracle::spawn("evals-artifacts-oracle.ts", SOURCE_MODULE);
    let artifacts = vec![json!({
        "type": "@earendil-works/pi-evals:source",
        "runId": "run-1",
        "attachments": [{
            "name": "nested/hello.ts",
            "contentType": "text/typescript",
            "body": "body\n",
            "bodyEncoding": "utf-8",
        }],
    })];
    let request = json!({
        "id": "invalid",
        "operation": "persist",
        "artifacts": artifacts,
        "runId": "run-1",
    });
    let response = oracle.request(&request);
    assert_eq!(response["ok"], false, "{response}");
    assert_eq!(response["error"]["name"], "TypeError");
    assert_eq!(
        response["error"]["message"],
        "Invalid eval artifact name: nested/hello.ts"
    );

    let directory = tempfile::tempdir().expect("artifact directory");
    let artifacts: Vec<TestArtifact> = artifacts
        .into_iter()
        .map(|value| serde_json::from_value(value).expect("artifact fixture"))
        .collect();
    let error = persist_eval_artifact_references(&artifacts, "run-1", &directory.path().display().to_string())
        .expect_err("nested names must be rejected");
    assert_eq!(error.message(), "Invalid eval artifact name: nested/hello.ts");
    assert_eq!(
        error.to_string(),
        response["error"]["message"].as_str().expect("error message")
    );
}
