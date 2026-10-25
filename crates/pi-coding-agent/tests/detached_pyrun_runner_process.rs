//! `extensions/pyrun/src/detached-runner.ts` and `detached-runner-entry.ts`
//! behavior at the real process boundary: the runner owns the evaluation, writes
//! the JSONL artifact, settles the control-db job, and answers control commands.

#[path = "support/executable_script.rs"]
mod executable_script;

use indexmap::IndexMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use pi_coding_agent::bundled_extensions::pyrun::detached_bridge::{
    EnqueueDetachedPyrunBridgeResponseInput, enqueue_detached_pyrun_bridge_response,
    parse_detached_pyrun_bridge_request, validate_detached_pyrun_bridge_request,
};
use pi_coding_agent::bundled_extensions::pyrun::detached_runner::{
    DETACHED_PYRUN_RUNNER_MODE, DetachedPyrunLaunchManifestData, run_detached_pyrun_runner,
    write_detached_pyrun_activation, write_detached_pyrun_launch_manifest,
};
use pi_coding_agent::bundled_extensions::pyrun::runner::{CanonicalPyrunEvalParams, PyrunRunnerOptions};
use pi_coding_agent::core::detached_job_control::{
    DetachedJobRuntimeCommand, EnqueueDetachedJobStatusRequestInput, claim_detached_job_runtime_commands,
    enqueue_detached_job_status_request,
};
use pi_coding_agent::core::detached_job_runner::{
    DetachedJobAgentType, DetachedJobLifecycleController, DetachedJobLifecycleControllerOptions, DetachedJobOwnership,
    RegisterDetachedJobInput,
};
use pi_coding_agent::core::lifecycle_coordinator::{LifecycleCoordinator, LifecycleCoordinatorOptions};
use pi_coding_agent::core::multi_agent_store::{AgentLifecycle, MultiAgentStore, MultiAgentStoreOptions};
use pi_coding_agent::core::runtime_process::{ProcessIdentity, read_process_identity};
use pi_coding_agent::core::session_control_db::{
    EnqueueRuntimeMailboxMessageInput, RuntimeMailboxAddress, RuntimeMailboxMessageKind, RuntimeMailboxStoreRef,
    enqueue_runtime_mailbox_message, list_runtime_mailbox_messages, read_multi_agent_agent,
    register_runtime_mailbox_listener, upsert_multi_agent_mailbox_message,
};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::encode_node_utf8;
use pi_js_compat::js_json::{self, JsString, JsValue as Value};
use tempfile::TempDir;

const TEST_RUNNER_ENV: &str = "PI_TEST_PYRUN_RUNNER_MANIFEST";

fn test_process_identity(name: &str) -> ProcessIdentity {
    let mut value: u32 = 0;
    for character in name.chars() {
        value = value.wrapping_mul(31).wrapping_add(u32::from(character));
    }
    ProcessIdentity {
        pid: 1_000_000_000 + (value % 1_000_000_000),
        start_time_ticks: u64::from(value) + 1,
        incarnation: None,
    }
}

fn environment() -> IndexMap<JsString, JsString> {
    let mut env = IndexMap::new();
    if let Some(path) = std::env::var_os("PATH") {
        env.insert("PATH".into(), JsString::from(path.to_str().expect("utf-8 test path")));
    }
    env
}

struct Fixture {
    directory: TempDir,
    control_db_path: String,
    session_path: String,
    controller: DetachedJobLifecycleController,
}

fn fixture(now: &'static str) -> Fixture {
    let directory = tempfile::tempdir().expect("temp dir");
    let root = directory.path().to_str().expect("utf-8 test path").to_owned();
    let control_db_path = directory
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let session_path = directory
        .path()
        .join("session.jsonl")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default()));
    let coordinator = Arc::new(LifecycleCoordinator::new(LifecycleCoordinatorOptions {
        control_db_path: control_db_path.clone().into(),
        create_agent_id: Arc::new(|| Ok("unused".into())),
        now: Arc::new(move || now.into()),
        process_identity: test_process_identity("runner-1"),
        session_path: session_path.clone().into(),
    }));
    let controller = DetachedJobLifecycleController::new(DetachedJobLifecycleControllerOptions {
        artifact_root: root.into(),
        control_db_path: control_db_path.clone().into(),
        coordinator,
        owner_agent_id: None,
        owner_session_id: "main".into(),
        session_path: session_path.clone().into(),
        store: Arc::clone(&store),
        write_bash_launch_manifest: None,
    });
    Fixture {
        directory,
        control_db_path,
        session_path,
        controller,
    }
}

fn register(fixture: &Fixture, job_id: &JsString, identity: ProcessIdentity) -> DetachedJobOwnership {
    fixture
        .controller
        .register(&RegisterDetachedJobInput {
            agent_type: DetachedJobAgentType::Pyrun,
            cwd: fixture
                .directory
                .path()
                .to_str()
                .expect("utf-8 test path")
                .to_owned()
                .into(),
            display_name: "Pyrun evaluation".into(),
            job_id: job_id.into(),
            process_identity: identity,
            worker_handle_id: "runner-pending".into(),
            tool_call_id: Some("test-pyrun-tool-call".into()),
            detached: Some(true),
            file_refs: None,
        })
        .expect("register")
}

fn manifest(
    fixture: &Fixture,
    job_id: &JsString,
    activation_path: &str,
    runner_command: &str,
) -> DetachedPyrunLaunchManifestData {
    let artifacts = fixture
        .controller
        .create_artifacts(&JsString::from(job_id))
        .expect("artifacts");
    let directory = Path::new(&encode_node_utf8(&artifacts.directory)).to_path_buf();
    DetachedPyrunLaunchManifestData {
        activation_path: activation_path.into(),
        artifacts,
        bridge_request_path: directory
            .join("bridge-requests.jsonl")
            .to_str()
            .expect("utf-8 test path")
            .to_owned()
            .into(),
        bridge_response_path: directory
            .join("bridge-responses.jsonl")
            .to_str()
            .expect("utf-8 test path")
            .to_owned()
            .into(),
        control_db_path: fixture.control_db_path.clone().into(),
        foreground_completion_path: directory
            .join("foreground-completed")
            .to_str()
            .expect("utf-8 test path")
            .to_owned()
            .into(),
        params: CanonicalPyrunEvalParams {
            code: "6 * 7".into(),
            ..Default::default()
        },
        runner_address: RuntimeMailboxAddress {
            agent_id: Some(job_id.into()),
            session_id: "main".into(),
        },
        runner_options: PyrunRunnerOptions {
            args: Some(Vec::new()),
            command: Some(runner_command.into()),
            detached: Some(false),
            env: Some(environment()),
            inherit_env: Some(true),
            inherited_process_group_id: None,
        },
        runner_process_identity: test_process_identity("runner-2"),
        session_path: fixture.session_path.clone().into(),
        started_at: 1_700_000_000_000.0,
        supervisor_process_identity: test_process_identity("supervisor"),
        tool_call_id: Some("test-pyrun-tool-call".into()),
    }
}

fn write_runner(dir: &Path, body: &str) -> String {
    let path = dir.join("fake-pyrun");
    executable_script::write_executable_script(&path, &format!("#!/usr/bin/env python3\n{body}"));
    path.to_str().expect("utf-8 test path").to_owned()
}

async fn wait_for(predicate: impl Fn() -> bool, what: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !predicate() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn read_agent(fixture: &Fixture, job_id: &JsString) -> pi_coding_agent::core::multi_agent_store::AgentSnapshot {
    read_multi_agent_agent(&fixture.control_db_path, &fixture.session_path, job_id)
        .expect("read agent")
        .expect("agent")
}

#[tokio::test]
async fn owns_evaluation_output_and_commits_one_exact_terminal_input() {
    let fixture = fixture("2026-07-11T22:00:00.000Z");
    let runner_command = write_runner(
        fixture.directory.path(),
        r#"
import json, sys
request = json.loads(sys.stdin.readline())
print(json.dumps({"type": "progress", "message": "working"}), flush=True)
print(json.dumps({"type": "pi_request", "method": "models.scoped", "params": None}), flush=True)
response = json.loads(sys.stdin.readline())
print(json.dumps({"executed": request["code"], "type": "completed", "value": response["result"]}), flush=True)
"#,
    );
    let job_id = fixture.controller.allocate_job_id(DetachedJobAgentType::Pyrun).unwrap();
    let mut data = manifest(
        &fixture,
        &job_id,
        &fixture
            .directory
            .path()
            .join("activation.json")
            .to_str()
            .expect("utf-8 test path"),
        &runner_command,
    );
    let (_, runner) = launch_registered_runner(&fixture, &job_id, &mut data);

    let supervisor_address = RuntimeMailboxAddress {
        agent_id: None,
        session_id: "main".into(),
    };
    let mut request = None;
    let deadline = Instant::now() + Duration::from_secs(10);
    while request.is_none() {
        let messages = list_runtime_mailbox_messages(&fixture.control_db_path).expect("messages");
        request = messages
            .iter()
            .find(|message| {
                message.recipient == supervisor_address
                    && message.status == pi_coding_agent::core::session_control_db::RuntimeMailboxMessageStatus::Pending
            })
            .map(|message| {
                let request = parse_detached_pyrun_bridge_request(message).expect("bridge request");
                assert!(
                    validate_detached_pyrun_bridge_request(
                        pi_coding_agent::bundled_extensions::pyrun::detached_bridge::ValidateDetachedPyrunBridgeRequestInput {
                            control_db_path: &JsString::from(&fixture.control_db_path),
                            message,
                            request: &request,
                            session_path: &JsString::from(&fixture.session_path),
                            supervisor_session_id: &JsString::from("main"),
                        }
                    )
                    .expect("validate")
                );
                let mut stale = request.clone();
                stale.identity.process_identity = test_process_identity("stale-runner");
                assert!(
                    !validate_detached_pyrun_bridge_request(
                        pi_coding_agent::bundled_extensions::pyrun::detached_bridge::ValidateDetachedPyrunBridgeRequestInput {
                            control_db_path: &JsString::from(&fixture.control_db_path),
                            message,
                            request: &stale,
                            session_path: &JsString::from(&fixture.session_path),
                            supervisor_session_id: &JsString::from("main"),
                        }
                    )
                    .expect("validate")
                );
                request
            });
        assert!(Instant::now() < deadline, "timed out waiting for the bridge request");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    enqueue_detached_pyrun_bridge_response(EnqueueDetachedPyrunBridgeResponseInput {
        control_db_path: fixture.control_db_path.clone().into(),
        error: None,
        request: request.expect("request"),
        result: Some(json!([{"id": "model-1"}])),
        session_path: fixture.session_path.clone().into(),
        supervisor_address: supervisor_address.clone(),
    })
    .expect("bridge response");

    let revision = runner.await.expect("join").expect("run");
    assert_eq!(revision, Some(2));
    let output = fs::read_to_string(&encode_node_utf8(&data.artifacts.output_path)).expect("output");
    assert_eq!(
        output,
        concat!(
            r#"{"kind":"progress","update":{"type":"progress","message":"working"}}"#,
            "\n",
            r#"{"kind":"result","result":{"executed":"6 * 7","type":"completed","value":[{"id":"model-1"}]}}"#,
            "\n",
        )
    );
    let agent = read_agent(&fixture, &job_id);
    assert_eq!(agent.lifecycle, AgentLifecycle::Completed);
    assert_eq!(agent.revision, 2);
    assert_eq!(
        agent.result.expect("result").summary.as_ref(),
        Some(&JsString::from("Pyrun evaluation completed."))
    );
}

#[tokio::test]
async fn answers_status_requests_while_the_evaluation_runs() {
    let fixture = fixture("2026-07-11T22:00:00.000Z");
    let runner_command = write_runner(
        fixture.directory.path(),
        r#"
import json, pathlib, sys, time
request = json.loads(sys.stdin.readline())
print(json.dumps({"type": "progress", "message": "working"}), flush=True)
release = pathlib.Path(__file__).with_name("status-requests-finished")
deadline = time.monotonic() + 30
while not release.exists():
    if time.monotonic() >= deadline:
        raise TimeoutError("test did not release the running evaluation")
    time.sleep(0.01)
print(json.dumps({"executed": request["code"], "type": "completed", "value": 42}), flush=True)
"#,
    );
    let job_id = fixture.controller.allocate_job_id(DetachedJobAgentType::Pyrun).unwrap();
    let mut data = manifest(
        &fixture,
        &job_id,
        &fixture
            .directory
            .path()
            .join("activation.json")
            .to_str()
            .expect("utf-8 test path"),
        &runner_command,
    );
    let (ownership, runner) = launch_registered_runner(&fixture, &job_id, &mut data);
    wait_for(
        || Path::new(&encode_node_utf8(&data.artifacts.output_path)).exists(),
        "output",
    )
    .await;

    let requester = RuntimeMailboxAddress {
        agent_id: None,
        session_id: "main".into(),
    };
    register_runtime_mailbox_listener(
        &fixture.control_db_path,
        &requester,
        std::process::id(),
        Some(&JsString::from(&fixture.session_path)),
        Default::default(),
    )
    .expect("requester listener");
    for request_id in ["status-1", "status-2"] {
        enqueue_detached_job_status_request(&EnqueueDetachedJobStatusRequestInput {
            control_db_path: fixture.control_db_path.clone().into(),
            identity: ownership.identity.clone(),
            requester_address: requester.clone(),
            request_id: request_id.into(),
            runner_address: data.runner_address.clone(),
            session_path: fixture.session_path.clone().into(),
        })
        .expect("status request");
        let response = wait_for_status_response(&fixture, &requester, &ownership, request_id).await;
        assert_eq!(response["state"], json!("running"));
        assert_eq!(response["outputPath"], json!(data.artifacts.output_path));
        assert_eq!(response["pendingRequestCount"], json!(0));
    }
    fs::write(fixture.directory.path().join("status-requests-finished"), "done").expect("release evaluation");
    let revision = runner.await.expect("join").expect("run");
    assert_eq!(revision, Some(2));
}

async fn wait_for_status_response(
    fixture: &Fixture,
    requester: &RuntimeMailboxAddress,
    ownership: &DetachedJobOwnership,
    request_id: &str,
) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let commands = claim_detached_job_runtime_commands(
            &JsString::from(&fixture.control_db_path),
            requester,
            &ownership.identity,
        )
        .expect("claim");
        for command in commands {
            if let DetachedJobRuntimeCommand::Response(response) = command
                && response.request_id == request_id
            {
                return response.result.expect("status result");
            }
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for the {request_id} response"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn adopts_the_cancelling_revision_and_finalizes_aborted() {
    let fixture = fixture("2026-07-11T22:00:00.000Z");
    let runner_command = write_runner(
        fixture.directory.path(),
        r#"
import json, sys, time
json.loads(sys.stdin.readline())
print(json.dumps({"type": "progress", "message": "working"}), flush=True)
while True:
    time.sleep(0.05)
"#,
    );
    let job_id = "agent_1";
    let mut data = manifest(
        &fixture,
        &JsString::from(job_id),
        &fixture
            .directory
            .path()
            .join("activation.json")
            .to_str()
            .expect("utf-8 test path"),
        &runner_command,
    );
    let (ownership, runner) = launch_registered_runner(&fixture, &JsString::from(job_id), &mut data);
    wait_for(
        || {
            Path::new(&encode_node_utf8(&data.artifacts.output_path)).exists()
                && fs::read_to_string(&encode_node_utf8(&data.artifacts.output_path))
                    .is_ok_and(|output| output.contains("working"))
        },
        "progress",
    )
    .await;

    let cancelling = fixture.controller.cancel(&ownership, None).expect("cancelling");
    let cancelling_agent = match cancelling {
        pi_coding_agent::core::lifecycle_coordinator::LifecycleCommandResult::Ok { agent } => agent,
        other => panic!("expected cancelling state: {other:?}"),
    };
    let cancel_body = js_json::stringify_lossless(&json!({
        "command": "cancel",
        "identity": ownership.identity,
        "reason": "test cancel",
    }));
    upsert_multi_agent_mailbox_message(
        &fixture.control_db_path,
        &fixture.session_path,
        "message_1",
        &json!({
            "body": cancel_body,
            "fromAgentId": "main",
            "id": "message_1",
            "kind": "system",
            "status": "pending",
            "toAgentId": job_id,
        }),
    )
    .expect("upsert");
    enqueue_runtime_mailbox_message(
        &fixture.control_db_path,
        &EnqueueRuntimeMailboxMessageInput {
            recipient: data.runner_address.clone(),
            recipient_host: None,
            sender: RuntimeMailboxAddress {
                agent_id: None,
                session_id: "main".into(),
            },
            kind: RuntimeMailboxMessageKind::System,
            store_ref: RuntimeMailboxStoreRef {
                message_id: "message_1".into(),
                session_path: fixture.session_path.clone().into(),
            },
        },
    )
    .expect("enqueue");

    let revision = runner.await.expect("join").expect("run");
    assert_eq!(revision, Some(cancelling_agent.revision + 1));
    let agent = read_agent(&fixture, &JsString::from(job_id));
    assert_eq!(agent.lifecycle, AgentLifecycle::Aborted);
    assert_eq!(agent.revision, cancelling_agent.revision + 1);
}

fn launch_registered_runner(
    fixture: &Fixture,
    job_id: &JsString,
    data: &mut DetachedPyrunLaunchManifestData,
) -> (
    DetachedJobOwnership,
    tokio::task::JoinHandle<Result<Option<i64>, JsString>>,
) {
    let manifest_path = Path::new(&encode_node_utf8(&data.artifacts.directory)).join("launch.json");
    let child = tokio::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "detached_runner_process_entry", "--nocapture"])
        .env(TEST_RUNNER_ENV, &manifest_path)
        .process_group(0)
        .kill_on_drop(true)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let identity = read_process_identity(child.id().unwrap()).unwrap();
    let ownership = register(fixture, job_id, identity.clone());
    data.runner_process_identity = identity;
    write_detached_pyrun_activation(&data.activation_path, &ownership.identity).unwrap();
    write_detached_pyrun_launch_manifest(&manifest_path.to_str().unwrap().into(), data).unwrap();
    (ownership, tokio::spawn(wait_runner_child(child)))
}

async fn wait_runner_child(child: tokio::process::Child) -> Result<Option<i64>, JsString> {
    let output = tokio::time::timeout(Duration::from_secs(15), child.wait_with_output())
        .await
        .map_err(|error| JsString::from(error.to_string()))?
        .map_err(|error| JsString::from(error.to_string()))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let revision = stdout
        .lines()
        .find_map(|line| line.strip_prefix("PyrunRevision:"))
        .ok_or_else(|| JsString::from("runner child omitted terminal revision"))?;
    serde_json::from_str(revision).map_err(|error| JsString::from(error.to_string()))
}

#[test]
fn detached_runner_process_entry() {
    let Ok(manifest_path) = std::env::var(TEST_RUNNER_ENV) else {
        return;
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    match runtime.block_on(run_detached_pyrun_runner(&JsString::from(&manifest_path))) {
        Ok(revision) => println!("PyrunRevision:{}", serde_json::to_string(&revision).unwrap()),
        Err(error) => {
            eprintln!("PyrunError:{}", encode_node_utf8(&error));
            std::process::exit(1);
        }
    }
}

#[tokio::test]
async fn launches_an_independent_runner_process_that_owns_the_evaluation() {
    let fixture = fixture("2026-07-11T22:00:00.000Z");
    let runner_command = write_runner(
        fixture.directory.path(),
        r#"
import json, sys
request = json.loads(sys.stdin.readline())
print(json.dumps({"executed": request["code"], "type": "completed", "value": 42}), flush=True)
"#,
    );
    let job_id = "agent_process";
    let data = manifest(
        &fixture,
        &JsString::from(job_id),
        &fixture
            .directory
            .path()
            .join("activation.json")
            .to_str()
            .expect("utf-8 test path"),
        &runner_command,
    );
    let manifest_path = Path::new(&encode_node_utf8(&data.artifacts.directory))
        .join("launch.json")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut runner = std::process::Command::new(std::env::current_exe().expect("test binary"))
        .args(["--exact", "detached_runner_process_entry", "--nocapture"])
        .env(TEST_RUNNER_ENV, &manifest_path)
        .current_dir(fixture.directory.path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("runner process");
    let runner_pid = runner.id();
    let ownership = register(
        &fixture,
        &JsString::from(job_id),
        read_process_identity(runner_pid).expect("runner identity"),
    );
    write_detached_pyrun_activation(&data.activation_path, &ownership.identity).expect("activation");
    write_detached_pyrun_launch_manifest(&JsString::from(&manifest_path), &data).expect("manifest");

    let status = runner.wait().expect("reap runner");
    assert!(status.success(), "runner exited with {status}");
    assert!(!Path::new(&format!("{manifest_path}.runner-error")).exists());
    let output = fs::read_to_string(&encode_node_utf8(&data.artifacts.output_path)).expect("output");
    assert_eq!(
        output,
        concat!(
            r#"{"kind":"result","result":{"executed":"6 * 7","type":"completed","value":42}}"#,
            "\n",
        )
    );
    let agent = read_agent(&fixture, &JsString::from(job_id));
    assert_eq!(agent.lifecycle, AgentLifecycle::Completed);
    assert_eq!(agent.revision, 2);
}

#[tokio::test]
async fn pi_binary_dispatches_the_detached_pyrun_runner_mode() {
    let fixture = fixture("2026-07-11T22:00:00.000Z");
    let runner_command = write_runner(
        fixture.directory.path(),
        r#"
import json, sys
request = json.loads(sys.stdin.readline())
print(json.dumps({"executed": request["code"], "type": "completed", "value": 42}), flush=True)
"#,
    );
    let job_id = "agent_pi_binary";
    let data = manifest(
        &fixture,
        &JsString::from(job_id),
        &fixture
            .directory
            .path()
            .join("activation.json")
            .to_str()
            .expect("utf-8 test path"),
        &runner_command,
    );
    let manifest_path = Path::new(&encode_node_utf8(&data.artifacts.directory))
        .join("launch.json")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut runner = std::process::Command::new(env!("CARGO_BIN_EXE_pi"))
        .args([DETACHED_PYRUN_RUNNER_MODE, &manifest_path])
        .current_dir(fixture.directory.path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("pi binary");
    let ownership = register(
        &fixture,
        &JsString::from(job_id),
        read_process_identity(runner.id()).expect("runner identity"),
    );
    write_detached_pyrun_activation(&data.activation_path, &ownership.identity).expect("activation");
    write_detached_pyrun_launch_manifest(&JsString::from(&manifest_path), &data).expect("manifest");

    let status = runner.wait().expect("reap runner");
    assert!(status.success(), "pi binary exited with {status}");
    assert!(!Path::new(&format!("{manifest_path}.runner-error")).exists());
    let agent = read_agent(&fixture, &JsString::from(job_id));
    assert_eq!(agent.lifecycle, AgentLifecycle::Completed);
    assert_eq!(agent.revision, 2);
    let output = fs::read_to_string(&encode_node_utf8(&data.artifacts.output_path)).expect("output");
    assert!(output.contains(r#""value":42"#), "{output}");
}

#[tokio::test]
async fn pi_binary_reports_a_runner_failure_to_the_runner_error_file() {
    let directory = tempfile::tempdir().expect("temp dir");
    let manifest_path = directory.path().join("launch.json");
    fs::write(&manifest_path, "{\"version\":3}\n").expect("manifest");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_pi"))
        .args([
            DETACHED_PYRUN_RUNNER_MODE,
            &manifest_path.to_str().expect("utf-8 test path"),
        ])
        .current_dir(directory.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("pi binary");
    assert_eq!(output.status.code(), Some(1));
    let error_path = format!("{}.runner-error", manifest_path.to_str().expect("utf-8 test path"));
    assert_eq!(
        fs::read_to_string(&error_path).expect("runner error"),
        format!(
            "Invalid detached Pyrun launch manifest: {}\n",
            manifest_path.to_str().expect("utf-8 test path")
        )
    );
}

#[tokio::test]
async fn terminal_commit_failure_uses_the_frozen_error_tag() {
    let fixture = fixture("2026-10-25T00:00:00.000Z");
    let runner_command = write_runner(
        fixture.directory.path(),
        "import json,sys\njson.loads(sys.stdin.readline())\nprint(json.dumps({'type':'completed','value':42}),flush=True)\n",
    );
    let data = manifest(
        &fixture,
        &"missing-agent".into(),
        fixture.directory.path().join("activation.json").to_str().unwrap(),
        &runner_command,
    );
    let identity = pi_coding_agent::core::session_control_db::DetachedJobOwnershipIdentity {
        job_id: "missing-agent".into(),
        owner: pi_coding_agent::core::session_control_db::MultiAgentRuntimeOwnerIdentity {
            agent_id: None,
            session_id: "main".into(),
        },
        process_identity: test_process_identity("missing"),
        output_label: "Pyrun output".into(),
    };
    write_detached_pyrun_activation(&data.activation_path, &identity).unwrap();
    let path = Path::new(&encode_node_utf8(&data.artifacts.directory)).join("launch.json");
    write_detached_pyrun_launch_manifest(&path.to_str().unwrap().into(), &data).unwrap();
    let child = tokio::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "detached_runner_process_entry", "--nocapture"])
        .env(TEST_RUNNER_ENV, &path)
        .process_group(0)
        .kill_on_drop(true)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let error = wait_runner_child(child)
        .await
        .expect_err("missing job must fail terminal commit");
    assert!(
        error.contains("Could not finalize detached Pyrun job: agent_not_found"),
        "{error:?}"
    );
}
