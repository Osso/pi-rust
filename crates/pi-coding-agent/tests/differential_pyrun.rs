//! Differential coverage for the Pyrun runner against the real `pyrun-jsonl`
//! binary: the pinned TypeScript `PyrunRunnerClient` and the Rust client run
//! identical inputs and must return identical result JSON.

use crate::shared_oracle as oracle;

use pi_coding_agent::bundled_extensions::pyrun::runner::{
    CanonicalPyrunEvalParams, PyrunRunnerClient, PyrunRunnerOptions,
};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue as Value};

fn oracle() -> oracle::TsOracle {
    oracle::TsOracle::spawn(
        "coding-agent-pyrun-oracle.ts",
        "packages/coding-agent/extensions/pyrun/src/runner.ts",
    )
}

fn require_real_runner() {
    let output = std::process::Command::new("pyrun-jsonl")
        .arg("--help")
        .output()
        .expect("pyrun-jsonl is required: failed to launch interpreter probe");
    assert!(
        output.status.success(),
        "pyrun-jsonl probe exited with {}\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn missing_broken_and_nonexecutable_runners_fail_instead_of_skipping() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap();
    let runner = directory.path().join("pyrun-jsonl");
    for mode in ["missing", "broken", "nonexecutable"] {
        if mode != "missing" {
            std::fs::write(
                &runner,
                "#!/usr/bin/python3\nimport sys\nsys.stderr.write('entropy probe failure')\nsys.exit(42)\n",
            )
            .unwrap();
            let permissions = if mode == "broken" { 0o755 } else { 0o644 };
            std::fs::set_permissions(&runner, std::fs::Permissions::from_mode(permissions)).unwrap();
        }
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "matches_typescript_runner_results_against_real_pyrun_jsonl",
                "--nocapture",
            ])
            .env("PATH", directory.path())
            .output()
            .unwrap();
        assert!(!output.status.success(), "{mode} runner silently passed: {output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("pyrun-jsonl"), "missing runner context: {stderr}");
        if mode == "broken" {
            assert!(stderr.contains("entropy probe failure"), "missing diagnostic: {stderr}");
        }
    }
}

#[tokio::test]
async fn matches_typescript_runner_results_against_real_pyrun_jsonl() {
    require_real_runner();
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
        command: Some("pyrun-jsonl".into()),
        args: Some(Vec::new()),
        ..Default::default()
    });
    for (index, case) in cases.iter().enumerate() {
        let params = CanonicalPyrunEvalParams {
            code: case["code"].as_string().unwrap().into(),
            pi: None,
            pi_bridge: Some(false),
            session_id: case["session_id"].as_string().map(JsString::from),
            stream_console: Some(true),
        };
        let actual = runner
            .evaluate(params, None, None, None)
            .await
            .unwrap_or_else(|error| panic!("case {index} failed: {error}"));
        let actual = js_json::to_value(&actual).unwrap();
        assert_eq!(&actual, &expected[index], "case {index}: {:?}", case["code"]);
    }
    runner.dispose();
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
