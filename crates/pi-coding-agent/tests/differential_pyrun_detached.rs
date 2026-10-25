//! Differential coverage for the detached Pyrun launch manifest and activation
//! formats against the pinned TypeScript module: byte-identical files, read-back
//! serialization, and identical validation errors.

use crate::shared_oracle as oracle;

use std::path::Path;

use pi_coding_agent::bundled_extensions::pyrun::detached_runner::{
    DetachedPyrunLaunchManifestData, read_detached_pyrun_activation, read_detached_pyrun_launch_manifest,
    write_detached_pyrun_activation, write_detached_pyrun_launch_manifest,
};
use pi_coding_agent::core::detached_job_runner::DetachedJobArtifacts;
use pi_coding_agent::core::runtime_process::ProcessIdentity;
use pi_coding_agent::core::session_control_db::{
    DetachedJobOwnershipIdentity, MultiAgentRuntimeOwnerIdentity, RuntimeMailboxAddress,
};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue as Value, encode_node_utf8};
use tempfile::TempDir;

fn oracle() -> oracle::TsOracle {
    oracle::TsOracle::spawn(
        "coding-agent-pyrun-detached-oracle.ts",
        "packages/coding-agent/extensions/pyrun/src/detached-runner.ts",
    )
}

fn identity(pid: u32, incarnation: Option<&str>) -> DetachedJobOwnershipIdentity {
    DetachedJobOwnershipIdentity {
        job_id: "pyrun_1".into(),
        owner: MultiAgentRuntimeOwnerIdentity {
            session_id: "main".into(),
            agent_id: None,
        },
        process_identity: ProcessIdentity {
            pid,
            start_time_ticks: 1_700,
            incarnation: incarnation.map(JsString::from),
        },
        output_label: "Pyrun output".into(),
    }
}

fn manifest(directory: &str, code: &str, tool_call_id: Option<&str>) -> DetachedPyrunLaunchManifestData {
    DetachedPyrunLaunchManifestData {
        activation_path: format!("{directory}/activation.json").into(),
        artifacts: DetachedJobArtifacts {
            directory: directory.into(),
            output_path: format!("{directory}/output.log").into(),
        },
        bridge_request_path: format!("{directory}/bridge-requests.jsonl").into(),
        bridge_response_path: format!("{directory}/bridge-responses.jsonl").into(),
        control_db_path: format!("{directory}/control.sqlite").into(),
        foreground_completion_path: format!("{directory}/foreground-completed").into(),
        params: pi_coding_agent::bundled_extensions::pyrun::runner::CanonicalPyrunEvalParams {
            code: code.into(),
            session_id: Some("session-1".into()),
            pi: Some(
                js_json::parse_lossless(r#"{"footer":{"availableProviderCount":2},"custom":{"2":2.0,"1":1.0}}"#)
                    .unwrap(),
            ),
            pi_bridge: Some(true),
            stream_console: Some(true),
        },
        runner_address: RuntimeMailboxAddress {
            agent_id: Some("pyrun_1".into()),
            session_id: "session-1".into(),
        },
        runner_options: pi_coding_agent::bundled_extensions::pyrun::runner::PyrunRunnerOptions {
            args: Some(vec!["--flag".into()]),
            command: Some("pyrun-jsonl".into()),
            detached: Some(false),
            env: Some([("PATH".into(), "/bin".into())].into()),
            inherit_env: Some(false),
            inherited_process_group_id: None,
        },
        runner_process_identity: ProcessIdentity {
            pid: 43,
            start_time_ticks: 101,
            incarnation: Some("run".into()),
        },
        session_path: format!("{directory}/session.jsonl").into(),
        started_at: 1_700_000_000_000.0,
        supervisor_process_identity: ProcessIdentity {
            pid: 42,
            start_time_ticks: 100,
            incarnation: None,
        },
        tool_call_id: tool_call_id.map(JsString::from),
    }
}

fn json_data(data: &DetachedPyrunLaunchManifestData) -> Value {
    js_json::to_value(data).expect("manifest data")
}

/// Writes the case through the TypeScript module in `directory` and returns its
/// observable output. The directory is cleared afterwards so Rust can write to
/// the same paths (the manifest checksum covers absolute paths).
fn write_via_oracle(
    oracle: &mut oracle::TsOracle,
    directory: &Path,
    data: &DetachedPyrunLaunchManifestData,
    identity: &DetachedJobOwnershipIdentity,
) -> Value {
    let _ = std::fs::remove_dir_all(directory);
    std::fs::create_dir_all(directory).expect("case dir");
    let response = oracle.request(&json!({
        "id": 1,
        "operation": "write",
        "cases": [{
            "dir": directory.to_str().expect("utf-8 test path"),
            "data": json_data(data),
            "identity": js_json::to_value(identity).expect("identity"),
        }],
    }));
    assert_eq!(response["ok"], json!(true), "{response:?}");
    let output = response["output"][0].clone();
    std::fs::remove_dir_all(directory).expect("clear oracle dir");
    std::fs::create_dir_all(directory).expect("case dir");
    output
}

#[test]
fn manifests_and_activations_match_the_typescript_bytes() {
    let directory = TempDir::new().expect("temp dir");
    let variants = [
        ("full", "6 * 7", Some("call-1"), 42, None),
        ("legacy", "1 + 1", None, 43, Some("incarnated")),
        ("unicode", "print('héllo — ✓')", Some("call-3"), 44, None),
    ];
    let mut oracle = oracle();
    for (name, code, tool_call_id, pid, incarnation) in variants {
        let case_dir = directory.path().join(name);
        let data = manifest(case_dir.to_str().expect("utf-8 test path"), code, tool_call_id);
        let case_identity = identity(pid, incarnation);
        let output = write_via_oracle(&mut oracle, &case_dir, &data, &case_identity);

        let manifest_path = case_dir.join("launch.json");
        write_detached_pyrun_launch_manifest(&JsString::from(manifest_path.to_str().expect("utf-8 test path")), &data)
            .expect("write manifest");
        assert_eq!(
            std::fs::read(&manifest_path).expect("manifest content"),
            encode_node_utf8(output["content"].as_string().expect("oracle content")).as_bytes(),
            "case {name} manifest bytes"
        );

        let activation_path = case_dir.join("activation.json");
        write_detached_pyrun_activation(
            &JsString::from(activation_path.to_str().expect("utf-8 test path")),
            &case_identity,
        )
        .expect("write activation");
        assert_eq!(
            std::fs::read(&activation_path).expect("activation content"),
            encode_node_utf8(output["activation"].as_string().expect("oracle activation")).as_bytes(),
            "case {name} activation bytes"
        );
        assert_eq!(
            JsString::from(
                serde_json::to_string(
                    &read_detached_pyrun_activation(&JsString::from(
                        activation_path.to_str().expect("utf-8 test path")
                    ))
                    .expect("read")
                )
                .expect("serialize")
            ),
            *output["readActivation"].as_string().expect("oracle read activation"),
            "case {name} activation read-back"
        );
        let read_back =
            read_detached_pyrun_launch_manifest(&JsString::from(manifest_path.to_str().expect("utf-8 test path")))
                .expect("read");
        assert_eq!(
            JsString::from(serde_json::to_string(&read_back).expect("serialize")),
            *output["readManifest"].as_string().expect("oracle read manifest"),
            "case {name} manifest read-back"
        );
    }
}

#[test]
fn invalid_manifests_throw_the_typescript_message() {
    let directory = TempDir::new().expect("temp dir");
    let case_dir = directory.path().join("invalid");
    let mut oracle = oracle();
    let data = manifest(case_dir.to_str().expect("utf-8 test path"), "6 * 7", Some("call-1"));
    let output = write_via_oracle(&mut oracle, &case_dir, &data, &identity(42, None));
    let valid = output["content"].as_string().expect("oracle manifest").clone();

    let tampered: Vec<(&str, JsString)> = vec![
        ("version", valid.replace("\"version\":3", "\"version\":2")),
        ("checksum", valid.replace("\"checksum\":\"", "\"checksum\":\"0")),
        (
            "startedAt",
            valid.replace("\"startedAt\":1700000000000", "\"startedAt\":\"nope\""),
        ),
        ("missing", "{\"version\":3}".into()),
    ];
    for (name, content) in &tampered {
        let case_dir = directory.path().join(name);
        std::fs::create_dir_all(&case_dir).expect("case dir");
        let manifest_path = case_dir.join("launch.json");
        std::fs::write(&manifest_path, encode_node_utf8(content)).expect("tampered manifest");
        let response = oracle.request(&json!({
            "id": 2,
            "operation": "readInvalid",
            "cases": [{"dir": case_dir.to_str().expect("utf-8 test path"), "content": content}],
        }));
        assert_eq!(response["ok"], json!(true), "{response:?}");
        let expected = response["output"][0]["error"]
            .as_string()
            .unwrap_or_else(|| panic!("case {name} unexpectedly read: {:?}", response["output"][0]));
        let error =
            read_detached_pyrun_launch_manifest(&JsString::from(manifest_path.to_str().expect("utf-8 test path")))
                .expect_err("invalid");
        assert_eq!(&error, expected, "case {name}");
    }
}

/// The truncated document must surface the same V8 `JSON.parse` message as TypeScript.
#[test]
fn truncated_manifests_throw_the_typescript_json_parse_error() {
    let directory = TempDir::new().expect("temp dir");
    let case_dir = directory.path().join("truncated");
    let mut oracle = oracle();
    let data = manifest(case_dir.to_str().expect("utf-8 test path"), "6 * 7", Some("call-1"));
    let output = write_via_oracle(&mut oracle, &case_dir, &data, &identity(42, None));
    let valid = output["content"].as_string().expect("oracle manifest");
    let truncated = valid.slice(0, valid.len() / 2);
    let response = oracle.request(&json!({
        "id": 2,
        "operation": "readInvalid",
        "cases": [{"dir": case_dir.to_str().expect("utf-8 test path"), "content": truncated}],
    }));
    assert_eq!(response["ok"], json!(true), "{response:?}");
    let expected = response["output"][0]["error"]
        .as_string()
        .unwrap_or_else(|| panic!("TypeScript must reject the truncated manifest: {response:?}"));

    let manifest_path = case_dir.join("launch.json");
    std::fs::write(&manifest_path, encode_node_utf8(&truncated)).expect("truncated manifest");
    let error = read_detached_pyrun_launch_manifest(&JsString::from(manifest_path.to_str().expect("utf-8 test path")))
        .expect_err("invalid");
    assert_eq!(&error, expected);
}
