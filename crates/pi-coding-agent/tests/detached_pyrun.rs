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
use serde_json::json;

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
    dir.to_string_lossy().into_owned()
}

fn identity() -> DetachedJobOwnershipIdentity {
    DetachedJobOwnershipIdentity {
        job_id: "pyrun-1".to_owned(),
        owner: MultiAgentRuntimeOwnerIdentity {
            session_id: "session-1".to_owned(),
            agent_id: None,
        },
        process_identity: ProcessIdentity {
            pid: 42,
            start_time_ticks: 100,
            incarnation: None,
        },
        output_label: "Pyrun output".to_owned(),
    }
}

fn manifest(dir: &str) -> DetachedPyrunLaunchManifestData {
    DetachedPyrunLaunchManifestData {
        activation_path: format!("{dir}/activation.json"),
        artifacts: DetachedJobArtifacts {
            directory: dir.to_owned(),
            output_path: format!("{dir}/output.jsonl"),
        },
        bridge_request_path: format!("{dir}/requests.jsonl"),
        bridge_response_path: format!("{dir}/responses.jsonl"),
        control_db_path: format!("{dir}/control.sqlite"),
        foreground_completion_path: format!("{dir}/completed"),
        params: CanonicalPyrunEvalParams {
            code: "1 + 1".to_owned(),
            pi: None,
            pi_bridge: Some(false),
            session_id: None,
            stream_console: Some(true),
        },
        runner_address: pi_coding_agent::core::session_control_db::RuntimeMailboxAddress {
            agent_id: Some("pyrun-1".to_owned()),
            session_id: "session-1".to_owned(),
        },
        runner_options: PyrunRunnerOptions {
            command: Some("pyrun-jsonl".to_owned()),
            args: Some(Vec::new()),
            detached: Some(false),
            env: Some(vec![("PATH".to_owned(), "/bin".to_owned())]),
            inherit_env: Some(false),
        },
        runner_process_identity: ProcessIdentity {
            pid: 43,
            start_time_ticks: 101,
            incarnation: None,
        },
        session_path: format!("{dir}/session.jsonl"),
        started_at: 1_700_000_000_000.0,
        supervisor_process_identity: ProcessIdentity {
            pid: 42,
            start_time_ticks: 100,
            incarnation: None,
        },
        tool_call_id: Some("call-1".to_owned()),
    }
}

#[test]
fn writes_and_reads_a_checksummed_launch_manifest() {
    let dir = temp_dir("manifest");
    let path = format!("{dir}/launch.json");
    let data = manifest(&dir);
    write_detached_pyrun_launch_manifest(&path, &data).unwrap();
    let parsed = read_detached_pyrun_launch_manifest(&path).unwrap();
    assert_eq!(
        serde_json::to_value(&parsed.data).unwrap(),
        serde_json::to_value(&data).unwrap()
    );
    assert_eq!(parsed.version, 3.0);
    assert_eq!(parsed.checksum.len(), 64);

    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.ends_with('\n'));
    let tampered = content.replace("1 + 1", "2 + 2");
    std::fs::write(&path, tampered).unwrap();
    assert_eq!(
        read_detached_pyrun_launch_manifest(&path).unwrap_err(),
        format!("Invalid detached Pyrun launch manifest: {path}")
    );

    let mut version = serde_json::from_str::<serde_json::Value>(&content).unwrap();
    version["version"] = json!(2);
    std::fs::write(&path, version.to_string()).unwrap();
    assert!(read_detached_pyrun_launch_manifest(&path).is_err());
}

#[test]
fn writes_and_reads_the_activation_identity() {
    let dir = temp_dir("activation");
    let path = format!("{dir}/activation.json");
    assert!(read_detached_pyrun_activation(&path).unwrap().is_none());
    let identity = identity();
    write_detached_pyrun_activation(&path, &identity).unwrap();
    assert_eq!(read_detached_pyrun_activation(&path).unwrap(), Some(identity));
}

#[test]
fn uses_the_pi_internal_runner_mode() {
    let invocation = get_detached_pyrun_runner_invocation(
        "/tmp/pyrun/launch.json",
        &DetachedPyrunRunnerInvocationOptions {
            compiled: true,
            executable_path: Some("/tmp/pi".to_owned()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        invocation,
        DetachedPyrunRunnerInvocation {
            args: vec![
                DETACHED_PYRUN_RUNNER_MODE.to_owned(),
                "/tmp/pyrun/launch.json".to_owned()
            ],
            executable: "/tmp/pi".to_owned(),
        }
    );
}

#[test]
fn keeps_the_source_entry_invocation_for_node() {
    let script = get_detached_pyrun_runner_invocation(
        "/tmp/pyrun/launch.json",
        &DetachedPyrunRunnerInvocationOptions {
            compiled: false,
            entry_path: Some("/repo/detached-runner-entry.mjs".to_owned()),
            executable_path: Some("/usr/bin/node".to_owned()),
        },
    )
    .unwrap();
    assert_eq!(
        script,
        DetachedPyrunRunnerInvocation {
            args: vec![
                "/repo/detached-runner-entry.mjs".to_owned(),
                "/tmp/pyrun/launch.json".to_owned(),
            ],
            executable: "/usr/bin/node".to_owned(),
        }
    );

    let typescript = get_detached_pyrun_runner_invocation(
        "/tmp/pyrun/launch.json",
        &DetachedPyrunRunnerInvocationOptions {
            compiled: false,
            entry_path: Some("/repo/detached-runner-entry.ts".to_owned()),
            executable_path: Some("/usr/bin/node".to_owned()),
        },
    )
    .unwrap();
    assert_eq!(
        typescript.args,
        vec![
            "--experimental-strip-types".to_owned(),
            "/repo/detached-runner-entry.ts".to_owned(),
            "/tmp/pyrun/launch.json".to_owned(),
        ]
    );
}

#[test]
fn requires_an_entry_path_when_not_compiled() {
    let error = get_detached_pyrun_runner_invocation(
        "/tmp/pyrun/launch.json",
        &DetachedPyrunRunnerInvocationOptions::default(),
    )
    .unwrap_err();
    assert_eq!(error, "Detached Pyrun runner entry path is required");
}
