//! Differential coverage for the bwrap backend and filesystem worker.
//!
//! - Backend invocation/environment is compared with the pinned TypeScript
//!   `backend.ts`.
//! - Worker operations are compared with the real Node `fs-worker.cjs` on the
//!   same fixtures.

#[path = "support/oracle.rs"]
mod oracle;

use std::path::Path;
use std::process::Command;

use indexmap::IndexMap;
use pi_coding_agent::bundled_extensions::bwrap::backend::{
    BwrapInvocationOptions, BwrapSandboxProfile, build_bwrap_invocation, create_bwrap_runner_environment,
    resolve_bwrap_sandbox_profile,
};
use pi_coding_agent::bundled_extensions::bwrap::worker::run_operation_json;
use pi_coding_agent::core::permissions::presets::SandboxProfileName;
use pi_js_compat::js_json::{JsString, JsValue as Value};
use pi_js_compat::{js_json, js_json as json};

fn oracle() -> oracle::TsOracle {
    oracle::TsOracle::spawn(
        "coding-agent-bwrap-oracle.ts",
        "packages/coding-agent/extensions/bwrap/src/backend.ts",
    )
}

fn temp_dir(name: &str) -> String {
    let dir = std::env::temp_dir().join(format!(
        "pi-bwrap-diff-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.to_str().expect("utf-8 test path").to_owned()
}

fn environment_value(environment: IndexMap<JsString, JsString>) -> Value {
    Value::Object(
        environment
            .into_iter()
            .map(|(key, value)| (key, Value::String(value)))
            .collect(),
    )
}

fn source_worker() -> String {
    let lock: Value = js_json::parse_lossless(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/porting/source-lock.json"
    )))
    .unwrap();
    format!(
        "{}/packages/coding-agent/extensions/bwrap/src/fs-worker.cjs",
        lock["worktree"]
            .as_string()
            .unwrap()
            .to_utf8()
            .expect("UTF-8 source-lock path")
    )
}

#[test]
fn matches_typescript_bwrap_profiles_and_runner_environment() {
    let mut oracle = oracle();
    let profiles = ["full-access", "read-only", "workspace-write"];
    let output = oracle.request(&json!({"id": 1, "operation": "profile", "cases": profiles}));
    let actual: Vec<Value> = profiles
        .iter()
        .map(|profile| {
            let profile = SandboxProfileName::parse(profile).unwrap();
            js_json::to_value(&resolve_bwrap_sandbox_profile(profile).map(|profile| profile.as_str())).unwrap()
        })
        .collect();
    assert_eq!(Value::Array(actual), output["output"]);

    let env_cases = [
        json!({"hostEnv": {"LANG": "C.UTF-8", "PATH": "/bin", "TERM": "xterm", "USER": "tester"}}),
        json!({"hostEnv": {"LANG": "C.UTF-8", "PATH": "/bin", "USER": "tester"}, "pythonPath": "/py"}),
        json!({"hostEnv": {"LANG": "", "PATH": "", "TERM": "", "USER": ""}, "pythonPath": ""}),
    ];
    let output = oracle.request(&json!({"id": 2, "operation": "runner-env", "cases": env_cases}));
    let actual: Vec<Value> = env_cases
        .iter()
        .map(|case| {
            let host: IndexMap<JsString, JsString> = case["hostEnv"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(key, value)| (key.clone(), value.as_string().unwrap().clone()))
                .collect();
            environment_value(create_bwrap_runner_environment(
                &host,
                case.get("pythonPath").and_then(Value::as_string),
            ))
        })
        .collect();
    assert_eq!(Value::Array(actual), output["output"]);
}

#[test]
fn matches_typescript_bwrap_invocation_arguments() {
    let workspace = temp_dir("workspace");
    std::fs::write(Path::new(&workspace).join("file.txt"), "hello").unwrap();
    std::fs::create_dir(Path::new(&workspace).join("sub")).unwrap();
    let extra = Path::new(&workspace)
        .join("file.txt")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let cases = [
        json!({"bwrapCommand":"bwrap", "command":["/bin/sh"], "cwd":workspace, "profile":"read-only", "env":{"PYTHONPATH":"", "TERM":"", "USER":""}}),
        json!({
            "bwrapCommand": "bwrap",
            "command": ["/bin/sh", "-lc", "echo ok"],
            "cwd": workspace,
            "profile": "workspace-write"
        }),
        json!({
            "bwrapCommand": "bwrap",
            "command": ["/bin/sh", "-lc", "echo ok"],
            "cwd": workspace,
            "env": {"LANG": "C.UTF-8", "PATH": "/custom/bin", "PYTHONPATH": "/py", "TERM": "xterm", "USER": "tester"},
            "extraReadOnlyPaths": [extra],
            "profile": "read-only"
        }),
    ];
    let mut oracle = oracle();
    let output = oracle.request(&json!({"id": 3, "operation": "invocation", "cases": cases}));
    let actual: Vec<Value> = cases
        .iter()
        .map(|case| {
            let env: Option<IndexMap<JsString, JsString>> = case.get("env").and_then(Value::as_object).map(|env| {
                env.iter()
                    .map(|(key, value)| (key.clone(), value.as_string().unwrap().clone()))
                    .collect()
            });
            let profile = SandboxProfileName::parse(
                &case["profile"]
                    .as_string()
                    .unwrap()
                    .to_utf8()
                    .expect("ASCII fixture profile"),
            )
            .unwrap();
            let invocation = build_bwrap_invocation(&BwrapInvocationOptions {
                bwrap_command: case["bwrapCommand"].as_string().unwrap().to_owned().into(),
                command: case["command"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|value| value.as_string().unwrap().clone())
                    .collect(),
                cwd: case["cwd"].as_string().unwrap().to_owned().into(),
                env,
                extra_read_only_paths: case
                    .get("extraReadOnlyPaths")
                    .and_then(Value::as_array)
                    .map(|paths| paths.iter().map(|path| path.as_string().unwrap().clone()).collect()),
                home_dir: None,
                profile: BwrapSandboxProfile(profile),
            })
            .unwrap();
            json!({"argv": invocation.argv, "command": invocation.command, "env": environment_value(invocation.env)})
        })
        .collect();
    assert_eq!(Value::Array(actual), output["output"]);
}

fn run_node_worker(operation: &str, payload: &Value, cwd: &str) -> (i32, String, String) {
    let output = Command::new("node")
        .arg(source_worker())
        .arg(operation)
        .arg(js_json::stringify_lossless(payload))
        .current_dir(cwd)
        .output()
        .expect("node fs-worker.cjs must run");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn matches_node_fs_worker_operations_on_the_same_fixtures() {
    let workspace = temp_dir("worker");
    std::fs::write(Path::new(&workspace).join("alpha.txt"), "one\ntwo\nthree\n").unwrap();
    std::fs::write(Path::new(&workspace).join("delta.txt"), "aa\nabab\n").unwrap();
    std::fs::write(Path::new(&workspace).join("beta.txt"), "Two\nfour\n").unwrap();
    std::fs::create_dir(Path::new(&workspace).join("sub")).unwrap();
    std::fs::write(Path::new(&workspace).join("sub/gamma.txt"), "two again\n").unwrap();
    std::fs::create_dir(Path::new(&workspace).join(".git")).unwrap();
    std::fs::write(Path::new(&workspace).join(".git/ignored.txt"), "two\n").unwrap();
    std::fs::create_dir(Path::new(&workspace).join("node_modules")).unwrap();
    std::fs::write(Path::new(&workspace).join("node_modules/pkg.txt"), "two\n").unwrap();

    let payloads = vec![
        ("access", json!({"path": "alpha.txt"})),
        ("exists", json!({"path": "missing.txt"})),
        ("exists", json!({"path": "alpha.txt"})),
        ("stat", json!({"path": "alpha.txt"})),
        ("stat", json!({"path": "sub"})),
        ("readFile", json!({"path": "alpha.txt"})),
        ("readRange", json!({"end": 5, "path": "alpha.txt", "start": 2})),
        ("readText", json!({"path": "alpha.txt"})),
        ("readdir", json!({"path": "."})),
        ("find", json!({"cwd": ".", "limit": 100, "pattern": "*.txt"})),
        ("find", json!({"cwd": ".", "limit": 2, "pattern": "*.txt"})),
        ("grep", json!({"limit": 100, "path": ".", "pattern": "two"})),
        (
            "grep",
            json!({"ignoreCase": true, "limit": 100, "path": ".", "pattern": "two"}),
        ),
        (
            "grep",
            json!({"glob": "*.txt", "limit": 1, "path": ".", "pattern": "two"}),
        ),
        (
            "grep",
            json!({"context": 1, "limit": 100, "path": "alpha.txt", "pattern": "two"}),
        ),
        ("grep", json!({"limit": 100, "path": ".", "pattern": "nomatch"})),
        ("grep", json!({"limit": 100, "path": "delta.txt", "pattern": "(a)\\1"})),
        ("grep", json!({"limit": 100, "path": "alpha.txt", "pattern": "t(?=wo)"})),
        (
            "grep",
            json!({"limit": 100, "path": "alpha.txt", "pattern": "(?<=t)wo"}),
        ),
        ("access", json!({"path": "../outside"})),
        ("unknown", json!({})),
    ];
    for (operation, payload) in payloads {
        let mut rust_payload = payload.clone();
        rust_payload["workspace"] = Value::String(workspace.clone().into());
        let rust = run_operation_json(operation, &rust_payload);
        let (node_code, node_stdout, node_stderr) = run_node_worker(operation, &payload, &workspace);
        match rust {
            Ok(value) => {
                assert_eq!(node_code, 0, "{operation}: node failed: {node_stderr}");
                let node_value: Value = js_json::parse_lossless(&node_stdout)
                    .unwrap_or_else(|error| panic!("{operation}: bad node JSON: {error}: {node_stdout}"));
                assert_eq!(value, json!(node_value), "{operation} payload {payload:?}");
            }
            Err(error) => {
                let message = error.message;
                assert_ne!(node_code, 0, "{operation}: rust failed but node succeeded");
                assert!(
                    !node_stderr.is_empty(),
                    "{operation}: node reported no stderr for failure"
                );
                assert!(
                    message.contains("escapes workspace") || message.starts_with("unknown op:"),
                    "{operation}: unexpected rust error: {message:?}"
                );
            }
        }
    }
}

#[test]
fn worker_write_and_mkdir_round_trip() {
    let workspace = temp_dir("worker-write");
    let payload = json!({"path": "new/dir/file.txt", "workspace": workspace});
    let mut mkdir_parent = payload.clone();
    mkdir_parent["path"] = json!("new/dir");
    assert_eq!(run_operation_json("mkdir", &mkdir_parent).unwrap(), json!({"ok": true}));
    let mut write = payload.clone();
    write["content"] = json!("content");
    assert_eq!(run_operation_json("writeFile", &write).unwrap(), json!({"ok": true}));
    assert_eq!(
        std::fs::read_to_string(Path::new(&workspace).join("new/dir/file.txt")).unwrap(),
        "content"
    );
    let mut mkdir = payload.clone();
    mkdir["path"] = json!("another");
    assert_eq!(run_operation_json("mkdir", &mkdir).unwrap(), json!({"ok": true}));
    assert!(Path::new(&workspace).join("another").is_dir());
}
