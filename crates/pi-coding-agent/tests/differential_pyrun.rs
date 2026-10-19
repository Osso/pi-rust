//! Differential coverage for the Pyrun runner against the real `pyrun-jsonl`
//! binary: the pinned TypeScript `PyrunRunnerClient` and the Rust client run
//! identical inputs and must return identical result JSON.

#[path = "support/oracle.rs"]
mod oracle;

use pi_coding_agent::bundled_extensions::pyrun::runner::{
    CanonicalPyrunEvalParams, PyrunRunnerClient, PyrunRunnerOptions,
};
use serde_json::{Value, json};

fn oracle() -> oracle::TsOracle {
    oracle::TsOracle::spawn(
        "coding-agent-pyrun-oracle.ts",
        "packages/coding-agent/extensions/pyrun/src/runner.ts",
    )
}

fn has_real_runner() -> bool {
    std::process::Command::new("pyrun-jsonl")
        .arg("--help")
        .output()
        .is_ok_and(|output| output.status.success())
}

#[tokio::test]
async fn matches_typescript_runner_results_against_real_pyrun_jsonl() {
    if !has_real_runner() {
        eprintln!("pyrun-jsonl is not installed; skipping real-runner differential");
        return;
    }
    let cases = [
        json!({"code": "1 + 1", "session_id": "diff-int"}),
        json!({"code": "\"a\" + \"b\"", "session_id": "diff-str"}),
        json!({"code": "print('hello')\n2 * 3", "session_id": "diff-console"}),
        json!({"code": "import sys\nprint('out')\nprint('err', file=sys.stderr)\n[1, {'a': True}]", "session_id": "diff-streams"}),
        json!({"code": "raise ValueError('boom')", "session_id": "diff-error"}),
        json!({"code": "def (", "session_id": "diff-syntax"}),
        json!({"code": "ctx", "session_id": "diff-ctx"}),
    ];
    let oracle_cases: Vec<Value> = cases
        .iter()
        .map(|case| {
            let mut params = case.clone();
            params["pi_bridge"] = json!(false);
            params["stream_console"] = json!(true);
            json!({
                "params": params,
                "options": {"command": "pyrun-jsonl", "args": []}
            })
        })
        .collect();
    let mut oracle = oracle();
    let output = oracle.request(&json!({"id": 1, "operation": "evaluate", "cases": oracle_cases}));
    let expected = output["output"].as_array().expect("oracle output array");

    let runner = PyrunRunnerClient::new(PyrunRunnerOptions {
        command: Some("pyrun-jsonl".to_owned()),
        args: Some(Vec::new()),
        ..Default::default()
    });
    for (index, case) in cases.iter().enumerate() {
        let params = CanonicalPyrunEvalParams {
            code: case["code"].as_str().unwrap().to_owned(),
            pi: None,
            pi_bridge: Some(false),
            session_id: case["session_id"].as_str().map(str::to_owned),
            stream_console: Some(true),
        };
        let actual = runner
            .evaluate(params, None, None, None)
            .await
            .unwrap_or_else(|error| panic!("case {index} failed: {error}"));
        let actual = serde_json::to_value(&actual).unwrap();
        assert_eq!(&actual, &expected[index], "case {index}: {}", case["code"]);
    }
    runner.dispose();
}
