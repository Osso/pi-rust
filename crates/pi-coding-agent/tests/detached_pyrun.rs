//! Behavioral tests for the detached Pyrun launch manifest and activation
//! protocol.

use pi_coding_agent::bundled_extensions::pyrun::detached_runner::{
    DETACHED_PYRUN_RUNNER_MODE, DetachedPyrunLaunchManifestData, DetachedPyrunRunnerInvocation,
    DetachedPyrunRunnerInvocationOptions, get_detached_pyrun_runner_invocation, read_detached_pyrun_activation,
    read_detached_pyrun_launch_manifest, write_detached_pyrun_activation, write_detached_pyrun_launch_manifest,
};
use pi_coding_agent::bundled_extensions::pyrun::runner::{CanonicalPyrunEvalParams, PyrunRunnerOptions};
use pi_coding_agent::core::detached_job_runner::DetachedJobArtifacts;
use pi_coding_agent::core::runtime_process::ProcessIdentity;
use pi_coding_agent::core::session_control_db::{DetachedJobOwnershipIdentity, MultiAgentRuntimeOwnerIdentity};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString};

fn temp_dir(name: &str) -> String {
    let dir = std::env::temp_dir().join(format!(
        "pi-pyrun-detached-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.to_str().expect("utf-8 test path").to_owned()
}

fn identity() -> DetachedJobOwnershipIdentity {
    DetachedJobOwnershipIdentity {
        job_id: "pyrun-1".into(),
        owner: MultiAgentRuntimeOwnerIdentity {
            session_id: "session-1".into(),
            agent_id: None,
        },
        process_identity: ProcessIdentity {
            pid: 42,
            start_time_ticks: 100,
            incarnation: None,
        },
        output_label: "Pyrun output".into(),
    }
}

fn manifest(dir: &str) -> DetachedPyrunLaunchManifestData {
    DetachedPyrunLaunchManifestData {
        activation_path: format!("{dir}/activation.json").into(),
        artifacts: DetachedJobArtifacts {
            directory: dir.into(),
            output_path: format!("{dir}/output.jsonl").into(),
        },
        bridge_request_path: format!("{dir}/requests.jsonl").into(),
        bridge_response_path: format!("{dir}/responses.jsonl").into(),
        control_db_path: format!("{dir}/control.sqlite").into(),
        foreground_completion_path: format!("{dir}/completed").into(),
        params: CanonicalPyrunEvalParams {
            code: "1 + 1".into(),
            pi: None,
            pi_bridge: Some(false),
            session_id: None,
            stream_console: Some(true),
        },
        runner_address: pi_coding_agent::core::session_control_db::RuntimeMailboxAddress {
            agent_id: Some("pyrun-1".into()),
            session_id: "session-1".into(),
        },
        runner_options: PyrunRunnerOptions {
            command: Some("pyrun-jsonl".into()),
            args: Some(Vec::new()),
            detached: Some(false),
            env: Some([("PATH".into(), "/bin".into())].into()),
            inherit_env: Some(false),
            inherited_process_group_id: None,
        },
        runner_process_identity: ProcessIdentity {
            pid: 43,
            start_time_ticks: 101,
            incarnation: None,
        },
        session_path: format!("{dir}/session.jsonl").into(),
        started_at: 1_700_000_000_000.0,
        supervisor_process_identity: ProcessIdentity {
            pid: 42,
            start_time_ticks: 100,
            incarnation: None,
        },
        tool_call_id: Some("call-1".into()),
    }
}

#[test]
fn writes_and_reads_a_checksummed_launch_manifest() {
    let dir = temp_dir("manifest");
    let path = format!("{dir}/launch.json");
    let data = manifest(&dir);
    write_detached_pyrun_launch_manifest(&JsString::from(&path), &data).unwrap();
    let parsed = read_detached_pyrun_launch_manifest(&JsString::from(&path)).unwrap();
    assert_eq!(
        js_json::to_value(&parsed.data).unwrap(),
        js_json::to_value(&data).unwrap()
    );
    assert_eq!(parsed.version, 3.0);
    assert_eq!(parsed.checksum.len(), 64);

    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.ends_with('\n'));
    let tampered = content.replace("1 + 1", "2 + 2");
    std::fs::write(&path, tampered).unwrap();
    assert_eq!(
        read_detached_pyrun_launch_manifest(&JsString::from(&path)).unwrap_err(),
        format!("Invalid detached Pyrun launch manifest: {path}")
    );

    let mut version = js_json::parse_lossless(&content).unwrap();
    version["version"] = json!(2);
    std::fs::write(&path, js_json::stringify_lossless(&version)).unwrap();
    assert!(read_detached_pyrun_launch_manifest(&JsString::from(&path)).is_err());
}

#[test]
fn writes_and_reads_the_activation_identity() {
    let dir = temp_dir("activation");
    let path = format!("{dir}/activation.json");
    assert!(
        read_detached_pyrun_activation(&JsString::from(&path))
            .unwrap()
            .is_none()
    );
    let identity = identity();
    write_detached_pyrun_activation(&JsString::from(&path), &identity).unwrap();
    assert_eq!(
        read_detached_pyrun_activation(&JsString::from(&path)).unwrap(),
        Some(identity)
    );
}

#[test]
fn uses_the_pi_internal_runner_mode() {
    let invocation = get_detached_pyrun_runner_invocation(
        &JsString::from("/tmp/pyrun/launch.json"),
        &DetachedPyrunRunnerInvocationOptions {
            compiled: true,
            executable_path: Some("/tmp/pi".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        invocation,
        DetachedPyrunRunnerInvocation {
            args: vec![DETACHED_PYRUN_RUNNER_MODE.into(), "/tmp/pyrun/launch.json".into()],
            executable: "/tmp/pi".into(),
        }
    );
}

#[test]
fn keeps_the_source_entry_invocation_for_node() {
    let script = get_detached_pyrun_runner_invocation(
        &JsString::from("/tmp/pyrun/launch.json"),
        &DetachedPyrunRunnerInvocationOptions {
            compiled: false,
            entry_path: Some("/repo/detached-runner-entry.mjs".into()),
            executable_path: Some("/usr/bin/node".into()),
        },
    )
    .unwrap();
    assert_eq!(
        script,
        DetachedPyrunRunnerInvocation {
            args: vec![
                "/repo/detached-runner-entry.mjs".into(),
                "/tmp/pyrun/launch.json".into(),
            ],
            executable: "/usr/bin/node".into(),
        }
    );

    let typescript = get_detached_pyrun_runner_invocation(
        &JsString::from("/tmp/pyrun/launch.json"),
        &DetachedPyrunRunnerInvocationOptions {
            compiled: false,
            entry_path: Some("/repo/detached-runner-entry.ts".into()),
            executable_path: Some("/usr/bin/node".into()),
        },
    )
    .unwrap();
    assert_eq!(
        typescript.args,
        vec![
            JsString::from("--experimental-strip-types"),
            JsString::from("/repo/detached-runner-entry.ts"),
            JsString::from("/tmp/pyrun/launch.json"),
        ]
    );
}

#[test]
fn requires_an_entry_path_when_not_compiled() {
    let error = get_detached_pyrun_runner_invocation(
        &JsString::from("/tmp/pyrun/launch.json"),
        &DetachedPyrunRunnerInvocationOptions::default(),
    )
    .unwrap_err();
    assert_eq!(error, "Detached Pyrun runner entry path is required");
}
