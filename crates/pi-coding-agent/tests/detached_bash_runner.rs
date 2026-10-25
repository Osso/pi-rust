//! `core/detached-bash-runner.ts` behavioral tests at the real process boundary.

use pi_js_compat::js_json::{self, JsObject, JsString, JsValue, encode_node_utf8};
use std::fs;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use pi_coding_agent::core::detached_bash_runner::{
    DetachedBashLaunchManifestData, DetachedFinalizeError, finalize_detached_job_with_retry,
    finalize_detached_job_with_retry_delay, read_detached_bash_activation, run_detached_bash_runner,
    write_detached_bash_activation, write_detached_bash_launch_manifest,
};
use pi_coding_agent::core::detached_job_control::{DetachedJobRuntimeCommand, enqueue_detached_job_status_request};
use pi_coding_agent::core::detached_job_runner::{
    DetachedJobAgentType, DetachedJobLifecycleController, DetachedJobLifecycleControllerOptions, DetachedJobOutcome,
    DetachedJobOwnership, RegisterDetachedJobInput, create_detached_job_terminal_input,
};
use pi_coding_agent::core::lifecycle_coordinator::{
    LifecycleCommandResult, LifecycleCoordinator, LifecycleCoordinatorOptions,
};
use pi_coding_agent::core::multi_agent_store::{AgentLifecycle, MultiAgentStore, MultiAgentStoreOptions};
use pi_coding_agent::core::runtime_process::{ProcessIdentity, read_process_identity};
use pi_coding_agent::core::session_control_db::{
    EnqueueRuntimeMailboxMessageInput, RuntimeMailboxAddress, RuntimeMailboxMessageKind, RuntimeMailboxStoreRef,
    enqueue_runtime_mailbox_message, read_multi_agent_agent, register_runtime_mailbox_listener,
    upsert_multi_agent_mailbox_message,
};
use tempfile::TempDir;

const TEST_RUNNER_ENV: &str = "PI_TEST_DETACHED_RUNNER_MANIFEST";

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

fn environment() -> JsObject {
    let mut env = JsObject::new();
    if let Some(path) = std::env::var_os("PATH") {
        env.insert(
            "PATH".into(),
            JsValue::String(JsString::from(path.to_str().expect("utf-8 test path"))),
        );
    }
    env
}

struct Fixture {
    directory: TempDir,
    control_db_path: String,
    session_path: String,
    store: Arc<MultiAgentStore>,
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
        store,
        controller,
    }
}

fn register(fixture: &Fixture, job_id: &JsString, identity: ProcessIdentity) -> DetachedJobOwnership {
    fixture
        .controller
        .register(&RegisterDetachedJobInput {
            agent_type: DetachedJobAgentType::Bash,
            cwd: fixture
                .directory
                .path()
                .to_str()
                .expect("utf-8 test path")
                .to_owned()
                .into(),
            display_name: "Bash command".into(),
            job_id: job_id.into(),
            process_identity: identity,
            worker_handle_id: "runner-pending".into(),
            tool_call_id: None,
            detached: None,
            file_refs: None,
        })
        .expect("register")
}

fn manifest(
    fixture: &Fixture,
    job_id: &JsString,
    activation_path: &JsString,
    foreground_completion_path: &JsString,
    args: Vec<JsString>,
) -> DetachedBashLaunchManifestData {
    DetachedBashLaunchManifestData {
        activation_path: activation_path.into(),
        args,
        artifacts: fixture.controller.create_artifacts(job_id).expect("artifacts"),
        command: "/bin/sh".into(),
        control_db_path: fixture.control_db_path.clone().into(),
        cwd: fixture
            .directory
            .path()
            .to_str()
            .expect("utf-8 test path")
            .to_owned()
            .into(),
        env: environment(),
        foreground_completion_path: foreground_completion_path.into(),
        runner_address: RuntimeMailboxAddress {
            agent_id: Some(job_id.into()),
            session_id: "main".into(),
        },
        session_path: fixture.session_path.clone().into(),
        timeout_ms: None,
        tool_call_id: None,
    }
}

#[test]
fn launch_manifest_enumerates_numeric_environment_names_like_javascript() {
    let fixture = fixture("2026-07-11T22:00:00.000Z");
    let mut data = manifest(
        &fixture,
        &JsString::from("job"),
        &JsString::from("activation"),
        &JsString::from("done"),
        Vec::new(),
    );
    data.env = [
        ("2".into(), pi_js_compat::js_json!("two")),
        ("1".into(), pi_js_compat::js_json!("one")),
        ("10".into(), pi_js_compat::js_json!("ten")),
    ]
    .into();
    let path = fixture.directory.path().join("launch.json");
    write_detached_bash_launch_manifest(&JsString::from(path.to_str().expect("utf-8 test path")), &data).unwrap();
    let bytes = fs::read_to_string(path).unwrap();
    assert!(bytes.contains(r#""env":{"1":"one","2":"two","10":"ten"}"#), "{bytes}");
}

#[tokio::test]
async fn owns_payload_exit_and_commits_one_exact_terminal_input() {
    let fixture = fixture("2026-07-11T22:00:00.000Z");
    let job_id = fixture.controller.allocate_job_id(DetachedJobAgentType::Bash).unwrap();
    let activation_path = fixture
        .directory
        .path()
        .join("activation.json")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let foreground_completion_path = fixture
        .directory
        .path()
        .join("foreground-completed")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let data = manifest(
        &fixture,
        &job_id,
        &JsString::from(&activation_path),
        &JsString::from(&foreground_completion_path),
        vec!["-c".into(), "printf done".into()],
    );
    let ownership = register(&fixture, &job_id, test_process_identity("runner"));
    write_detached_bash_activation(&JsString::from(&activation_path), &ownership.identity).expect("activation");
    let manifest_path = fixture
        .directory
        .path()
        .join("launch.json")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    write_detached_bash_launch_manifest(&JsString::from(&manifest_path), &data).expect("manifest");

    let result = run_detached_bash_runner(&JsString::from(&manifest_path), None)
        .await
        .expect("run");
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.terminal_revision, 2);
    assert_eq!(
        fs::read_to_string(&encode_node_utf8(&data.artifacts.output_path)).expect("output"),
        "done"
    );
    let agent = read_multi_agent_agent(&fixture.control_db_path, &fixture.session_path, &job_id)
        .expect("read")
        .expect("agent");
    assert_eq!(agent.lifecycle, AgentLifecycle::Completed);
    assert_eq!(agent.revision, 2);
    let result_details = agent.result.expect("result");
    assert_eq!(
        result_details.summary.as_ref(),
        Some(&JsString::from("Process exited successfully."))
    );
    assert_eq!(
        result_details.file_refs.expect("file refs")[0].path,
        data.artifacts.output_path
    );
}

#[tokio::test]
async fn completes_a_foreground_run_without_registering_a_background_job() {
    let fixture = fixture("2026-07-11T22:00:00.000Z");
    let job_id = fixture.controller.allocate_job_id(DetachedJobAgentType::Bash).unwrap();
    let activation_path = fixture
        .directory
        .path()
        .join("activation.json")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let foreground_completion_path = fixture
        .directory
        .path()
        .join("foreground-completed")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let data = manifest(
        &fixture,
        &job_id,
        &JsString::from(&activation_path),
        &JsString::from(&foreground_completion_path),
        vec!["-c".into(), "printf foreground".into()],
    );
    let manifest_path = fixture
        .directory
        .path()
        .join("launch.json")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    write_detached_bash_launch_manifest(&JsString::from(&manifest_path), &data).expect("manifest");

    let result = run_detached_bash_runner(&JsString::from(&manifest_path), None)
        .await
        .expect("run");
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.terminal_revision, 0);
    assert_eq!(
        fs::read_to_string(&encode_node_utf8(&data.artifacts.output_path)).expect("output"),
        "foreground"
    );
    let completion: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&foreground_completion_path).expect("completion"))
            .expect("completion json");
    assert_eq!(completion["exitCode"], 0);
    assert_eq!(completion["timedOut"], false);
    assert_eq!(fixture.store.list_agents().len(), 0);
}

// Frozen detached-bash-runner.ts:171,183-185 adds signed milliseconds to Date.now().
#[tokio::test]
async fn negative_manifest_timeout_expires_before_the_payload_can_complete() {
    let fixture = fixture("2026-10-24T00:00:00.000Z");
    let job_id = fixture.controller.allocate_job_id(DetachedJobAgentType::Bash).unwrap();
    let root = fixture.directory.path();
    let activation: JsString = root.join("activation.json").to_str().unwrap().into();
    let completion_path = root.join("completion.json");
    let completion: JsString = completion_path.to_str().unwrap().into();
    let marker = root.join("survived");
    let mut data = manifest(
        &fixture,
        &job_id,
        &activation,
        &completion,
        vec![
            "-c".into(),
            format!("sleep .25; printf survived > '{}'", marker.display()).into(),
        ],
    );
    data.timeout_ms = js_json::from_value(&pi_js_compat::js_json!(-5000)).unwrap();
    let path: JsString = root.join("launch.json").to_str().unwrap().into();
    write_detached_bash_launch_manifest(&path, &data).unwrap();
    let result = run_detached_bash_runner(&path, None).await.unwrap();
    let completion = js_json::parse_lossless(&fs::read_to_string(completion_path).unwrap()).unwrap();
    assert_eq!(completion["timedOut"], pi_js_compat::js_json!(true));
    assert_eq!(result.exit_code, None);
    assert!(
        !marker.exists(),
        "a past deadline must not become abs(timeoutMs) in the future"
    );
}

#[tokio::test]
async fn adopts_the_cancelling_revision_terminates_the_payload_group_and_finalizes_aborted() {
    let fixture = fixture("2026-07-11T22:00:00.000Z");
    let job_id = "agent_1";
    let activation_path = fixture
        .directory
        .path()
        .join("activation.json")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let foreground_completion_path = fixture
        .directory
        .path()
        .join("foreground-completed")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let data = manifest(
        &fixture,
        &JsString::from(job_id),
        &JsString::from(&activation_path),
        &JsString::from(&foreground_completion_path),
        vec!["-c".into(), "while true; do printf running; sleep 0.05; done".into()],
    );
    let ownership = register(
        &fixture,
        &JsString::from(job_id),
        read_process_identity(std::process::id()).expect("process identity"),
    );
    write_detached_bash_activation(&JsString::from(&activation_path), &ownership.identity).expect("activation");
    let manifest_path = fixture
        .directory
        .path()
        .join("launch.json")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    write_detached_bash_launch_manifest(&JsString::from(&manifest_path), &data).expect("manifest");

    let runner = tokio::spawn({
        let manifest_path = manifest_path.clone();
        async move { run_detached_bash_runner(&JsString::from(&manifest_path), None).await }
    });
    let identity_path = std::path::Path::new(&encode_node_utf8(&data.artifacts.directory)).join("payload.json");
    wait_for(|| identity_path.exists(), "payload identity").await;

    let coordinator = LifecycleCoordinator::new(LifecycleCoordinatorOptions {
        control_db_path: fixture.control_db_path.clone().into(),
        create_agent_id: Arc::new(|| Ok("unused".into())),
        now: Arc::new(|| "2026-07-11T22:00:00.000Z".into()),
        process_identity: test_process_identity("runner-1"),
        session_path: fixture.session_path.clone().into(),
    });
    let cancelling = coordinator
        .request_detached_cancellation(
            &pi_coding_agent::core::lifecycle_coordinator::DetachedCancellationCommandInput {
                agent: ownership.agent.clone(),
                output_label: data.artifacts.output_path.clone(),
                ownership: ownership.control_ownership.clone(),
                reason: None,
                suppress_terminal_notification: None,
            },
        )
        .expect("cancelling");
    let LifecycleCommandResult::Ok {
        agent: cancelling_agent,
    } = cancelling
    else {
        panic!("expected cancelling state");
    };

    let cancel_body = js_json::stringify_lossless(&pi_js_compat::js_json!({
        "command": "cancel",
        "identity": ownership.identity,
        "reason": "test cancel",
    }));
    upsert_multi_agent_mailbox_message(
        &fixture.control_db_path,
        &fixture.session_path,
        "message_1",
        &pi_js_compat::js_json!({
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

    let result = runner.await.expect("join").expect("run");
    assert_eq!(result.terminal_revision, cancelling_agent.revision + 1);
    let agent = read_multi_agent_agent(&fixture.control_db_path, &fixture.session_path, job_id)
        .expect("read")
        .expect("agent");
    assert_eq!(agent.lifecycle, AgentLifecycle::Aborted);
    assert_eq!(agent.revision, cancelling_agent.revision + 1);
}

#[tokio::test]
async fn answers_status_requests_while_the_payload_runs() {
    let fixture = fixture("2026-07-11T22:00:00.000Z");
    let job_id = "agent_status";
    let activation_path = fixture
        .directory
        .path()
        .join("activation.json")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let foreground_completion_path = fixture
        .directory
        .path()
        .join("foreground-completed")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let data = manifest(
        &fixture,
        &JsString::from(job_id),
        &JsString::from(&activation_path),
        &JsString::from(&foreground_completion_path),
        vec!["-c".into(), "sleep 0.4; printf done".into()],
    );
    let ownership = register(
        &fixture,
        &JsString::from(job_id),
        read_process_identity(std::process::id()).expect("process identity"),
    );
    write_detached_bash_activation(&JsString::from(&activation_path), &ownership.identity).expect("activation");
    let manifest_path = fixture
        .directory
        .path()
        .join("launch.json")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    write_detached_bash_launch_manifest(&JsString::from(&manifest_path), &data).expect("manifest");

    let runner = tokio::spawn({
        let manifest_path = manifest_path.clone();
        async move { run_detached_bash_runner(&JsString::from(&manifest_path), None).await }
    });
    let identity_path = std::path::Path::new(&encode_node_utf8(&data.artifacts.directory)).join("payload.json");
    wait_for(|| identity_path.exists(), "payload identity").await;

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
    enqueue_detached_job_status_request(
        &pi_coding_agent::core::detached_job_control::EnqueueDetachedJobStatusRequestInput {
            control_db_path: fixture.control_db_path.clone().into(),
            identity: ownership.identity.clone(),
            requester_address: requester.clone(),
            request_id: "status-1".into(),
            runner_address: data.runner_address.clone(),
            session_path: fixture.session_path.clone().into(),
        },
    )
    .expect("status request");

    let result = runner.await.expect("join").expect("run");
    assert_eq!(result.exit_code, Some(0));
    let responses = wait_for_status_response(&fixture, &requester, &ownership).await;
    let DetachedJobRuntimeCommand::Response(response) = &responses[0] else {
        panic!("expected response");
    };
    let status = response.result.as_ref().expect("status");
    assert_eq!(status["state"], pi_js_compat::js_json!("running"));
    assert_eq!(status["outputPath"], pi_js_compat::js_json!(data.artifacts.output_path));
}

async fn wait_for_status_response(
    fixture: &Fixture,
    requester: &RuntimeMailboxAddress,
    ownership: &DetachedJobOwnership,
) -> Vec<DetachedJobRuntimeCommand> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let commands = pi_coding_agent::core::detached_job_control::claim_detached_job_runtime_commands(
            &JsString::from(&fixture.control_db_path),
            requester,
            &ownership.identity,
        )
        .expect("claim");
        if !commands.is_empty() {
            return commands;
        }
        assert!(Instant::now() < deadline, "timed out waiting for status response");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
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
    runtime
        .block_on(run_detached_bash_runner(&JsString::from(&manifest_path), None))
        .expect("runner process");
}

#[tokio::test]
async fn launches_an_independent_runner_that_remains_the_payload_parent() {
    let fixture = fixture("2026-07-11T22:00:00.000Z");
    let job_id = "agent_launch";
    let activation_path = fixture
        .directory
        .path()
        .join("activation.json")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let foreground_completion_path = fixture
        .directory
        .path()
        .join("foreground-completed")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    // The payload blocks until the test has inspected its parent, so a loaded
    // machine cannot let it exit before `/proc/<pid>/status` is read.
    let release_path = fixture.directory.path().join("release-payload");
    let data = manifest(
        &fixture,
        &JsString::from(job_id),
        &JsString::from(&activation_path),
        &JsString::from(&foreground_completion_path),
        vec![
            "-c".into(),
            pi_js_compat::js_format!(
                "while [ ! -e '{}' ]; do sleep 0.01; done; printf independent",
                release_path.to_str().expect("utf-8 test path")
            ),
        ],
    );
    let manifest_path = fixture
        .directory
        .path()
        .join("launch.json")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    write_detached_bash_launch_manifest(&JsString::from(&manifest_path), &data).expect("manifest");
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
    write_detached_bash_activation(&JsString::from(&activation_path), &ownership.identity).expect("activation");
    let identity_path = std::path::Path::new(&encode_node_utf8(&data.artifacts.directory)).join("payload.json");
    wait_for(|| identity_path.exists(), "payload identity").await;
    let payload_identity: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&identity_path).expect("payload identity"))
            .expect("payload identity json");
    let payload_pid = payload_identity["pid"].as_u64().expect("payload pid");
    let status = fs::read_to_string(format!("/proc/{payload_pid}/status")).expect("status");
    assert!(
        status.contains(&format!("PPid:\t{runner_pid}")),
        "payload must remain a child of the runner: {status}"
    );
    fs::write(&release_path, "").expect("release payload");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let agent = read_multi_agent_agent(&fixture.control_db_path, &fixture.session_path, job_id)
            .expect("read")
            .expect("agent");
        if agent.lifecycle == AgentLifecycle::Completed {
            break;
        }
        assert!(Instant::now() < deadline, "timed out waiting for completion");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        fs::read_to_string(&encode_node_utf8(&data.artifacts.output_path))
            .expect("output")
            .contains("independent")
    );
    runner.wait().expect("reap runner");
}

#[tokio::test]
async fn does_not_retry_non_database_terminal_finalization_failures() {
    let directory = tempfile::tempdir().expect("temp dir");
    let artifacts = pi_coding_agent::core::detached_job_runner::DetachedJobArtifacts {
        directory: directory.path().to_str().expect("utf-8 test path").to_owned().into(),
        output_path: directory
            .path()
            .join("output.log")
            .to_str()
            .expect("utf-8 test path")
            .to_owned()
            .into(),
    };
    fs::write(&encode_node_utf8(&artifacts.output_path), "done").expect("output");
    let identity = pi_coding_agent::core::detached_job_runner::DetachedJobOwnershipIdentity {
        job_id: "job-1".into(),
        owner: pi_coding_agent::core::session_control_db::MultiAgentRuntimeOwnerIdentity {
            agent_id: None,
            session_id: "runner".into(),
        },
        process_identity: test_process_identity("retry"),
        output_label: "Bash output".into(),
    };
    let terminal = create_detached_job_terminal_input(
        &artifacts,
        &identity,
        DetachedJobOutcome::Completed {
            exit_code: None,
            summary: Some("done".into()),
        },
        &JsString::from("2026-07-11T22:00:00.000Z"),
        None,
        None,
    )
    .expect("terminal");
    let attempts = AtomicUsize::new(0);
    let result: Result<(), FakeError> = finalize_detached_job_with_retry_delay(
        &terminal,
        |_| {
            attempts.fetch_add(1, Ordering::SeqCst);
            Err(FakeError { contention: false })
        },
        Duration::from_millis(1),
    )
    .await;
    assert!(result.is_err());
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn retries_the_same_terminal_input_after_transient_database_failures() {
    let directory = tempfile::tempdir().expect("temp dir");
    let artifacts = pi_coding_agent::core::detached_job_runner::DetachedJobArtifacts {
        directory: directory.path().to_str().expect("utf-8 test path").to_owned().into(),
        output_path: directory
            .path()
            .join("output.log")
            .to_str()
            .expect("utf-8 test path")
            .to_owned()
            .into(),
    };
    fs::write(&encode_node_utf8(&artifacts.output_path), "done").expect("output");
    let identity = pi_coding_agent::core::detached_job_runner::DetachedJobOwnershipIdentity {
        job_id: "job-1".into(),
        owner: pi_coding_agent::core::session_control_db::MultiAgentRuntimeOwnerIdentity {
            agent_id: None,
            session_id: "runner".into(),
        },
        process_identity: test_process_identity("retry"),
        output_label: "Bash output".into(),
    };
    let terminal = create_detached_job_terminal_input(
        &artifacts,
        &identity,
        DetachedJobOutcome::Completed {
            exit_code: None,
            summary: Some("done".into()),
        },
        &JsString::from("2026-07-11T22:00:00.000Z"),
        None,
        None,
    )
    .expect("terminal");
    let attempts = AtomicUsize::new(0);
    let result = finalize_detached_job_with_retry_delay(
        &terminal,
        |_| {
            let attempt = attempts.fetch_add(1, Ordering::SeqCst);
            if attempt < 2 {
                Err(FakeError { contention: true })
            } else {
                Ok(8)
            }
        },
        Duration::from_millis(0),
    )
    .await
    .expect("retried");
    assert_eq!(result, 8);
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn default_retry_delay_is_250_milliseconds() {
    let directory = tempfile::tempdir().expect("temp dir");
    let artifacts = pi_coding_agent::core::detached_job_runner::DetachedJobArtifacts {
        directory: directory.path().to_str().expect("utf-8 test path").to_owned().into(),
        output_path: directory
            .path()
            .join("output.log")
            .to_str()
            .expect("utf-8 test path")
            .to_owned()
            .into(),
    };
    fs::write(&encode_node_utf8(&artifacts.output_path), "done").expect("output");
    let identity = pi_coding_agent::core::detached_job_runner::DetachedJobOwnershipIdentity {
        job_id: "job-1".into(),
        owner: pi_coding_agent::core::session_control_db::MultiAgentRuntimeOwnerIdentity {
            agent_id: None,
            session_id: "runner".into(),
        },
        process_identity: test_process_identity("retry"),
        output_label: "Bash output".into(),
    };
    let terminal = create_detached_job_terminal_input(
        &artifacts,
        &identity,
        DetachedJobOutcome::Completed {
            exit_code: None,
            summary: None,
        },
        &JsString::from("2026-07-11T22:00:00.000Z"),
        None,
        None,
    )
    .expect("terminal");
    let attempts = AtomicUsize::new(0);
    let started = Instant::now();
    let result = finalize_detached_job_with_retry(&terminal, |_| {
        let attempt = attempts.fetch_add(1, Ordering::SeqCst);
        if attempt == 0 {
            Err(FakeError { contention: true })
        } else {
            Ok(1)
        }
    })
    .await;
    assert_eq!(result.expect("retried"), 1);
    assert!(started.elapsed() >= Duration::from_millis(250));
}

#[derive(Debug)]
struct FakeError {
    contention: bool,
}

impl DetachedFinalizeError for FakeError {
    fn is_contention(&self) -> bool {
        self.contention
    }
}

#[tokio::test]
async fn reads_activation_only_when_the_file_exists() {
    let directory = tempfile::tempdir().expect("temp dir");
    let path = directory
        .path()
        .join("activation.json")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    assert_eq!(
        read_detached_bash_activation(&JsString::from(&path)).expect("read"),
        None
    );
}

async fn wait_for(mut predicate: impl FnMut() -> bool, label: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !predicate() {
        assert!(Instant::now() < deadline, "timed out waiting for {label}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// End-to-end runner-owned detach: the real `pi` binary executes the internal
/// detached Bash runner mode against a launch manifest and finalizes the job.
#[test]
fn real_pi_binary_finalizes_a_detached_bash_job() {
    let fixture = fixture("2026-07-11T22:00:00.000Z");
    let job_id = fixture.controller.allocate_job_id(DetachedJobAgentType::Bash).unwrap();
    let activation_path = fixture
        .directory
        .path()
        .join("activation.json")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let foreground_completion_path = fixture
        .directory
        .path()
        .join("foreground-completed")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let data = manifest(
        &fixture,
        &job_id,
        &JsString::from(&activation_path),
        &JsString::from(&foreground_completion_path),
        vec!["-c".into(), "printf detached-ok".into()],
    );
    let ownership = register(&fixture, &job_id, test_process_identity("runner"));
    write_detached_bash_activation(&JsString::from(&activation_path), &ownership.identity).expect("activation");
    let manifest_path = fixture
        .directory
        .path()
        .join("launch.json")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    write_detached_bash_launch_manifest(&JsString::from(&manifest_path), &data).expect("manifest");

    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_pi"));
    command
        .arg("--internal-detached-bash-runner")
        .arg(&manifest_path)
        .env_clear()
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    if let Some(home) = std::env::var_os("HOME") {
        command.env("HOME", home);
    }
    if let Some(path) = std::env::var_os("PATH") {
        command.env("PATH", path);
    }
    let child = command.spawn().expect("spawn pi");
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut child = child;
    let status = loop {
        if let Some(status) = child.try_wait().expect("wait for pi") {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            panic!("pi detached runner timed out");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let output = child.wait_with_output().expect("runner output");
    assert!(status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(
        fs::read_to_string(&encode_node_utf8(&data.artifacts.output_path)).expect("output"),
        "detached-ok"
    );
    let agent = read_multi_agent_agent(&fixture.control_db_path, &fixture.session_path, &job_id)
        .expect("read")
        .expect("agent");
    assert_eq!(agent.lifecycle, AgentLifecycle::Completed);
    assert_eq!(agent.revision, 2);
    assert_eq!(
        agent.result.expect("result").summary.as_ref(),
        Some(&JsString::from("Process exited successfully."))
    );
}
