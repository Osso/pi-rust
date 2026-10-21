//! `core/detached-bash-runner.ts` behavioral tests at the real process boundary.

use std::collections::BTreeMap;
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

fn environment() -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    if let Some(path) = std::env::var_os("PATH") {
        env.insert("PATH".to_owned(), path.to_string_lossy().into_owned());
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
    let root = directory.path().to_string_lossy().into_owned();
    let control_db_path = directory.path().join("control.sqlite").to_string_lossy().into_owned();
    let session_path = directory.path().join("session.jsonl").to_string_lossy().into_owned();
    let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default()));
    let coordinator = Arc::new(LifecycleCoordinator::new(LifecycleCoordinatorOptions {
        control_db_path: control_db_path.clone(),
        create_agent_id: Arc::new(|| Ok("unused".to_owned())),
        now: Arc::new(move || now.to_owned()),
        process_identity: test_process_identity("runner-1"),
        session_path: session_path.clone(),
    }));
    let controller = DetachedJobLifecycleController::new(DetachedJobLifecycleControllerOptions {
        artifact_root: root,
        control_db_path: control_db_path.clone(),
        coordinator,
        owner_agent_id: None,
        owner_session_id: "main".to_owned(),
        session_path: session_path.clone(),
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

fn register(fixture: &Fixture, job_id: &str, identity: ProcessIdentity) -> DetachedJobOwnership {
    fixture
        .controller
        .register(&RegisterDetachedJobInput {
            agent_type: DetachedJobAgentType::Bash,
            cwd: fixture.directory.path().to_string_lossy().into_owned(),
            display_name: "Bash command".to_owned(),
            job_id: job_id.to_owned(),
            process_identity: identity,
            worker_handle_id: "runner-pending".to_owned(),
            tool_call_id: None,
            detached: None,
            file_refs: None,
        })
        .expect("register")
}

fn manifest(
    fixture: &Fixture,
    job_id: &str,
    activation_path: &str,
    foreground_completion_path: &str,
    args: Vec<String>,
) -> DetachedBashLaunchManifestData {
    DetachedBashLaunchManifestData {
        activation_path: activation_path.to_owned(),
        args,
        artifacts: fixture.controller.create_artifacts(job_id).expect("artifacts"),
        command: "/bin/sh".to_owned(),
        control_db_path: fixture.control_db_path.clone(),
        cwd: fixture.directory.path().to_string_lossy().into_owned(),
        env: environment(),
        foreground_completion_path: foreground_completion_path.to_owned(),
        runner_address: RuntimeMailboxAddress {
            agent_id: Some(job_id.to_owned()),
            session_id: "main".to_owned(),
        },
        session_path: fixture.session_path.clone(),
        timeout_ms: None,
        tool_call_id: None,
    }
}

#[test]
fn launch_manifest_enumerates_numeric_environment_names_like_javascript() {
    let fixture = fixture("2026-07-11T22:00:00.000Z");
    let mut data = manifest(&fixture, "job", "activation", "done", Vec::new());
    data.env = [
        ("2".into(), "two".into()),
        ("1".into(), "one".into()),
        ("10".into(), "ten".into()),
    ]
    .into();
    let path = fixture.directory.path().join("launch.json");
    write_detached_bash_launch_manifest(path.to_str().unwrap(), &data).unwrap();
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
        .to_string_lossy()
        .into_owned();
    let foreground_completion_path = fixture
        .directory
        .path()
        .join("foreground-completed")
        .to_string_lossy()
        .into_owned();
    let data = manifest(
        &fixture,
        &job_id,
        &activation_path,
        &foreground_completion_path,
        vec!["-c".to_owned(), "printf done".to_owned()],
    );
    let ownership = register(&fixture, &job_id, test_process_identity("runner"));
    write_detached_bash_activation(&activation_path, &ownership.identity).expect("activation");
    let manifest_path = fixture
        .directory
        .path()
        .join("launch.json")
        .to_string_lossy()
        .into_owned();
    write_detached_bash_launch_manifest(&manifest_path, &data).expect("manifest");

    let result = run_detached_bash_runner(&manifest_path, None).await.expect("run");
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.terminal_revision, 2);
    assert_eq!(fs::read_to_string(&data.artifacts.output_path).expect("output"), "done");
    let agent = read_multi_agent_agent(&fixture.control_db_path, &fixture.session_path, &job_id)
        .expect("read")
        .expect("agent");
    assert_eq!(agent.lifecycle, AgentLifecycle::Completed);
    assert_eq!(agent.revision, 2);
    let result_details = agent.result.expect("result");
    assert_eq!(result_details.summary.as_deref(), Some("Process exited successfully."));
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
        .to_string_lossy()
        .into_owned();
    let foreground_completion_path = fixture
        .directory
        .path()
        .join("foreground-completed")
        .to_string_lossy()
        .into_owned();
    let data = manifest(
        &fixture,
        &job_id,
        &activation_path,
        &foreground_completion_path,
        vec!["-c".to_owned(), "printf foreground".to_owned()],
    );
    let manifest_path = fixture
        .directory
        .path()
        .join("launch.json")
        .to_string_lossy()
        .into_owned();
    write_detached_bash_launch_manifest(&manifest_path, &data).expect("manifest");

    let result = run_detached_bash_runner(&manifest_path, None).await.expect("run");
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.terminal_revision, 0);
    assert_eq!(
        fs::read_to_string(&data.artifacts.output_path).expect("output"),
        "foreground"
    );
    let completion: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&foreground_completion_path).expect("completion"))
            .expect("completion json");
    assert_eq!(completion["exitCode"], 0);
    assert_eq!(completion["timedOut"], false);
    assert_eq!(fixture.store.list_agents().len(), 0);
}

#[tokio::test]
async fn adopts_the_cancelling_revision_terminates_the_payload_group_and_finalizes_aborted() {
    let fixture = fixture("2026-07-11T22:00:00.000Z");
    let job_id = "agent_1";
    let activation_path = fixture
        .directory
        .path()
        .join("activation.json")
        .to_string_lossy()
        .into_owned();
    let foreground_completion_path = fixture
        .directory
        .path()
        .join("foreground-completed")
        .to_string_lossy()
        .into_owned();
    let data = manifest(
        &fixture,
        job_id,
        &activation_path,
        &foreground_completion_path,
        vec![
            "-c".to_owned(),
            "while true; do printf running; sleep 0.05; done".to_owned(),
        ],
    );
    let ownership = register(
        &fixture,
        job_id,
        read_process_identity(std::process::id()).expect("process identity"),
    );
    write_detached_bash_activation(&activation_path, &ownership.identity).expect("activation");
    let manifest_path = fixture
        .directory
        .path()
        .join("launch.json")
        .to_string_lossy()
        .into_owned();
    write_detached_bash_launch_manifest(&manifest_path, &data).expect("manifest");

    let runner = tokio::spawn({
        let manifest_path = manifest_path.clone();
        async move { run_detached_bash_runner(&manifest_path, None).await }
    });
    let identity_path = std::path::Path::new(&data.artifacts.directory).join("payload.json");
    wait_for(|| identity_path.exists(), "payload identity").await;

    let coordinator = LifecycleCoordinator::new(LifecycleCoordinatorOptions {
        control_db_path: fixture.control_db_path.clone(),
        create_agent_id: Arc::new(|| Ok("unused".to_owned())),
        now: Arc::new(|| "2026-07-11T22:00:00.000Z".to_owned()),
        process_identity: test_process_identity("runner-1"),
        session_path: fixture.session_path.clone(),
    });
    let cancelling = coordinator
        .request_detached_cancellation(
            &pi_coding_agent::core::lifecycle_coordinator::DetachedCancellationCommandInput {
                agent: ownership.agent.clone(),
                output_label: data.artifacts.output_path.clone(),
                ownership: ownership.control_ownership.clone(),
                reason: None,
            },
        )
        .expect("cancelling");
    let LifecycleCommandResult::Ok {
        agent: cancelling_agent,
    } = cancelling
    else {
        panic!("expected cancelling state");
    };

    upsert_multi_agent_mailbox_message(
        &fixture.control_db_path,
        &fixture.session_path,
        "message_1",
        &serde_json::json!({
            "body": serde_json::json!({
                "command": "cancel",
                "identity": ownership.identity,
                "reason": "test cancel",
            })
            .to_string(),
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
                session_id: "main".to_owned(),
            },
            kind: RuntimeMailboxMessageKind::System,
            store_ref: RuntimeMailboxStoreRef {
                message_id: "message_1".to_owned(),
                session_path: fixture.session_path.clone(),
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
        .to_string_lossy()
        .into_owned();
    let foreground_completion_path = fixture
        .directory
        .path()
        .join("foreground-completed")
        .to_string_lossy()
        .into_owned();
    let data = manifest(
        &fixture,
        job_id,
        &activation_path,
        &foreground_completion_path,
        vec!["-c".to_owned(), "sleep 0.4; printf done".to_owned()],
    );
    let ownership = register(
        &fixture,
        job_id,
        read_process_identity(std::process::id()).expect("process identity"),
    );
    write_detached_bash_activation(&activation_path, &ownership.identity).expect("activation");
    let manifest_path = fixture
        .directory
        .path()
        .join("launch.json")
        .to_string_lossy()
        .into_owned();
    write_detached_bash_launch_manifest(&manifest_path, &data).expect("manifest");

    let runner = tokio::spawn({
        let manifest_path = manifest_path.clone();
        async move { run_detached_bash_runner(&manifest_path, None).await }
    });
    let identity_path = std::path::Path::new(&data.artifacts.directory).join("payload.json");
    wait_for(|| identity_path.exists(), "payload identity").await;

    let requester = RuntimeMailboxAddress {
        agent_id: None,
        session_id: "main".to_owned(),
    };
    register_runtime_mailbox_listener(
        &fixture.control_db_path,
        &requester,
        std::process::id(),
        Some(&fixture.session_path),
        Default::default(),
    )
    .expect("requester listener");
    enqueue_detached_job_status_request(
        &pi_coding_agent::core::detached_job_control::EnqueueDetachedJobStatusRequestInput {
            control_db_path: fixture.control_db_path.clone(),
            identity: ownership.identity.clone(),
            requester_address: requester.clone(),
            request_id: "status-1".to_owned(),
            runner_address: data.runner_address.clone(),
            session_path: fixture.session_path.clone(),
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
    assert_eq!(status["state"], "running");
    assert_eq!(status["outputPath"], data.artifacts.output_path);
}

async fn wait_for_status_response(
    fixture: &Fixture,
    requester: &RuntimeMailboxAddress,
    ownership: &DetachedJobOwnership,
) -> Vec<DetachedJobRuntimeCommand> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let commands = pi_coding_agent::core::detached_job_control::claim_detached_job_runtime_commands(
            &fixture.control_db_path,
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
        .block_on(run_detached_bash_runner(&manifest_path, None))
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
        .to_string_lossy()
        .into_owned();
    let foreground_completion_path = fixture
        .directory
        .path()
        .join("foreground-completed")
        .to_string_lossy()
        .into_owned();
    // The payload blocks until the test has inspected its parent, so a loaded
    // machine cannot let it exit before `/proc/<pid>/status` is read.
    let release_path = fixture.directory.path().join("release-payload");
    let data = manifest(
        &fixture,
        job_id,
        &activation_path,
        &foreground_completion_path,
        vec![
            "-c".to_owned(),
            format!(
                "while [ ! -e '{}' ]; do sleep 0.01; done; printf independent",
                release_path.display()
            ),
        ],
    );
    let manifest_path = fixture
        .directory
        .path()
        .join("launch.json")
        .to_string_lossy()
        .into_owned();
    write_detached_bash_launch_manifest(&manifest_path, &data).expect("manifest");
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
        job_id,
        read_process_identity(runner_pid).expect("runner identity"),
    );
    write_detached_bash_activation(&activation_path, &ownership.identity).expect("activation");
    let identity_path = std::path::Path::new(&data.artifacts.directory).join("payload.json");
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
        fs::read_to_string(&data.artifacts.output_path)
            .expect("output")
            .contains("independent")
    );
    runner.wait().expect("reap runner");
}

#[tokio::test]
async fn does_not_retry_non_database_terminal_finalization_failures() {
    let directory = tempfile::tempdir().expect("temp dir");
    let artifacts = pi_coding_agent::core::detached_job_runner::DetachedJobArtifacts {
        directory: directory.path().to_string_lossy().into_owned(),
        output_path: directory.path().join("output.log").to_string_lossy().into_owned(),
    };
    fs::write(&artifacts.output_path, "done").expect("output");
    let identity = pi_coding_agent::core::detached_job_runner::DetachedJobOwnershipIdentity {
        job_id: "job-1".to_owned(),
        owner: pi_coding_agent::core::session_control_db::MultiAgentRuntimeOwnerIdentity {
            agent_id: None,
            session_id: "runner".to_owned(),
        },
        process_identity: test_process_identity("retry"),
        output_label: "Bash output".to_owned(),
    };
    let terminal = create_detached_job_terminal_input(
        &artifacts,
        &identity,
        DetachedJobOutcome::Completed {
            exit_code: None,
            summary: Some("done".to_owned()),
        },
        "2026-07-11T22:00:00.000Z",
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
        directory: directory.path().to_string_lossy().into_owned(),
        output_path: directory.path().join("output.log").to_string_lossy().into_owned(),
    };
    fs::write(&artifacts.output_path, "done").expect("output");
    let identity = pi_coding_agent::core::detached_job_runner::DetachedJobOwnershipIdentity {
        job_id: "job-1".to_owned(),
        owner: pi_coding_agent::core::session_control_db::MultiAgentRuntimeOwnerIdentity {
            agent_id: None,
            session_id: "runner".to_owned(),
        },
        process_identity: test_process_identity("retry"),
        output_label: "Bash output".to_owned(),
    };
    let terminal = create_detached_job_terminal_input(
        &artifacts,
        &identity,
        DetachedJobOutcome::Completed {
            exit_code: None,
            summary: Some("done".to_owned()),
        },
        "2026-07-11T22:00:00.000Z",
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
        directory: directory.path().to_string_lossy().into_owned(),
        output_path: directory.path().join("output.log").to_string_lossy().into_owned(),
    };
    fs::write(&artifacts.output_path, "done").expect("output");
    let identity = pi_coding_agent::core::detached_job_runner::DetachedJobOwnershipIdentity {
        job_id: "job-1".to_owned(),
        owner: pi_coding_agent::core::session_control_db::MultiAgentRuntimeOwnerIdentity {
            agent_id: None,
            session_id: "runner".to_owned(),
        },
        process_identity: test_process_identity("retry"),
        output_label: "Bash output".to_owned(),
    };
    let terminal = create_detached_job_terminal_input(
        &artifacts,
        &identity,
        DetachedJobOutcome::Completed {
            exit_code: None,
            summary: None,
        },
        "2026-07-11T22:00:00.000Z",
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
    let path = directory.path().join("activation.json").to_string_lossy().into_owned();
    assert_eq!(read_detached_bash_activation(&path).expect("read"), None);
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
        .to_string_lossy()
        .into_owned();
    let foreground_completion_path = fixture
        .directory
        .path()
        .join("foreground-completed")
        .to_string_lossy()
        .into_owned();
    let data = manifest(
        &fixture,
        &job_id,
        &activation_path,
        &foreground_completion_path,
        vec!["-c".to_owned(), "printf detached-ok".to_owned()],
    );
    let ownership = register(&fixture, &job_id, test_process_identity("runner"));
    write_detached_bash_activation(&activation_path, &ownership.identity).expect("activation");
    let manifest_path = fixture
        .directory
        .path()
        .join("launch.json")
        .to_string_lossy()
        .into_owned();
    write_detached_bash_launch_manifest(&manifest_path, &data).expect("manifest");

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
        fs::read_to_string(&data.artifacts.output_path).expect("output"),
        "detached-ok"
    );
    let agent = read_multi_agent_agent(&fixture.control_db_path, &fixture.session_path, &job_id)
        .expect("read")
        .expect("agent");
    assert_eq!(agent.lifecycle, AgentLifecycle::Completed);
    assert_eq!(agent.revision, 2);
    assert_eq!(
        agent.result.expect("result").summary.as_deref(),
        Some("Process exited successfully.")
    );
}
