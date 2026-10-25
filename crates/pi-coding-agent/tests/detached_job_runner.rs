//! `core/detached-job-runner.ts` behavioral tests.

use pi_js_compat::js_json::{JsString, encode_node_utf8};
use std::fs;
use std::sync::Arc;

use pi_coding_agent::core::detached_job_runner::{
    DetachedJobAgentType, DetachedJobArtifacts, DetachedJobError, DetachedJobLifecycleController,
    DetachedJobLifecycleControllerOptions, DetachedJobOutcome, DetachedJobOwnership, RegisterDetachedJobInput,
    create_detached_job_artifacts, create_detached_job_terminal_input,
};
use pi_coding_agent::core::lifecycle_coordinator::{
    LifecycleCommandResult, LifecycleCoordinator, LifecycleCoordinatorOptions,
};
use pi_coding_agent::core::multi_agent_store::{MultiAgentStore, MultiAgentStoreOptions};
use pi_coding_agent::core::runtime_process::{ProcessIdentity, read_process_identity};
use pi_coding_agent::core::session_control_db::{
    FinalizeDetachedJobInput, FinalizeDetachedJobResult, RuntimeMailboxAddress, RuntimeMailboxRegistrationOptions,
    finalize_detached_job, register_runtime_mailbox_listener,
};
use tempfile::TempDir;

const NOW: &str = "2026-07-11T22:00:00.000Z";

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

fn current_identity() -> ProcessIdentity {
    read_process_identity(std::process::id()).expect("process identity")
}

struct Fixture {
    _directory: Option<TempDir>,
    control_db_path: String,
    session_path: String,
    store: Arc<MultiAgentStore>,
    controller: DetachedJobLifecycleController,
}

fn fixture(owner_session_id: &str) -> Fixture {
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
        now: Arc::new(|| NOW.into()),
        process_identity: current_identity(),
        session_path: session_path.clone().into(),
    }));
    let controller = DetachedJobLifecycleController::new(DetachedJobLifecycleControllerOptions {
        artifact_root: root.into(),
        control_db_path: control_db_path.clone().into(),
        coordinator,
        owner_agent_id: None,
        owner_session_id: owner_session_id.into(),
        session_path: session_path.clone().into(),
        store: Arc::clone(&store),
        write_bash_launch_manifest: None,
    });
    Fixture {
        _directory: Some(directory),
        control_db_path,
        session_path,
        store,
        controller,
    }
}

fn register_bash(fixture: &Fixture, job_id: &JsString, identity: ProcessIdentity) -> DetachedJobOwnership {
    fixture
        .controller
        .register(&RegisterDetachedJobInput {
            agent_type: DetachedJobAgentType::Bash,
            cwd: "/repo".into(),
            display_name: "Bash command".into(),
            job_id: job_id.into(),
            process_identity: identity,
            worker_handle_id: "runner".into(),
            tool_call_id: None,
            detached: None,
            file_refs: None,
        })
        .expect("register")
}

#[test]
fn builds_an_immutable_terminal_input_from_the_completed_output_artifact() {
    let directory = tempfile::tempdir().expect("temp dir");
    let root = directory.path().to_str().expect("utf-8 test path").to_owned();
    let artifacts = create_detached_job_artifacts(&root, "job-1").expect("artifacts");
    fs::write(encode_node_utf8(&artifacts.output_path), "stdout\nstderr\n").expect("output");
    let identity = pi_coding_agent::core::detached_job_runner::DetachedJobOwnershipIdentity {
        job_id: "job-1".into(),
        owner: pi_coding_agent::core::session_control_db::MultiAgentRuntimeOwnerIdentity {
            agent_id: None,
            session_id: "supervisor-1".into(),
        },
        process_identity: test_process_identity("runtime-1"),
        output_label: "Bash output".into(),
    };
    let terminal = create_detached_job_terminal_input(
        &artifacts,
        &identity,
        DetachedJobOutcome::Completed {
            exit_code: Some(0),
            summary: Some("done".into()),
        },
        &JsString::from("2026-07-11T22:00:00.000Z"),
        Some(1_234),
        None,
    )
    .expect("terminal");
    assert_eq!(terminal.job_id, "job-1");
    assert_eq!(terminal.duration_ms, Some(1_234));
    assert_eq!(terminal.tool_call_id, None);
    assert_eq!(terminal.output.label, "Bash output");
    assert_eq!(terminal.output.path, artifacts.output_path);
    assert_eq!(
        terminal.output.sha256,
        "f7047c7da981acbadc372dd699784ec46eda4ffb15c03fe25ac25bc595eed04b"
    );
    assert_eq!(terminal.output.size, 14);
    assert_eq!(terminal.terminal_at, "2026-07-11T22:00:00.000Z");
}

#[test]
fn rejects_job_ids_that_escape_their_artifact_root() {
    let directory = tempfile::tempdir().expect("temp dir");
    let root = directory.path().to_str().expect("utf-8 test path").to_owned();
    let error = create_detached_job_artifacts(&root, "../escape").expect_err("invalid id");
    assert_eq!(error.to_string(), "Detached job ID must be one path segment");
    assert!(matches!(error, DetachedJobError::InvalidJobId));
}

#[test]
fn rejects_an_existing_detached_job_directory_before_runner_launch() {
    let fixture = fixture("supervisor-1");
    fixture
        .controller
        .create_artifacts(&JsString::from("agent_1"))
        .expect("first");
    let error = fixture
        .controller
        .create_artifacts(&JsString::from("agent_1"))
        .expect_err("exists");
    assert!(matches!(error, DetachedJobError::ArtifactExists(_)));
    assert!(error.to_string().contains("already exists"));
}

#[test]
fn isolates_reused_agent_ids_by_supervisor_session() {
    let first_directory = tempfile::tempdir().expect("temp dir");
    let root = first_directory.path().to_str().expect("utf-8 test path").to_owned();
    let first = fixture_in_root(&root, "session-1.jsonl", "supervisor-1");
    let second = fixture_in_root(&root, "session-2.jsonl", "supervisor-2");
    let first_artifacts = first
        .controller
        .create_artifacts(&JsString::from("agent_1"))
        .expect("first");
    let second_artifacts = second
        .controller
        .create_artifacts(&JsString::from("agent_1"))
        .expect("second");
    assert_ne!(first_artifacts.directory, second_artifacts.directory);
    assert!(first_artifacts.directory.contains("session-1"));
    assert!(second_artifacts.directory.contains("session-2"));
}

fn fixture_in_root(root: &str, session_name: &str, owner_session_id: &str) -> Fixture {
    let control_db_path = std::path::Path::new(root)
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let session_path = std::path::Path::new(root)
        .join(session_name)
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default()));
    let coordinator = Arc::new(LifecycleCoordinator::new(LifecycleCoordinatorOptions {
        control_db_path: control_db_path.clone().into(),
        create_agent_id: Arc::new(|| Ok("unused".into())),
        now: Arc::new(|| NOW.into()),
        process_identity: current_identity(),
        session_path: session_path.clone().into(),
    }));
    let controller = DetachedJobLifecycleController::new(DetachedJobLifecycleControllerOptions {
        artifact_root: root.into(),
        control_db_path: control_db_path.into(),
        coordinator,
        owner_agent_id: None,
        owner_session_id: owner_session_id.into(),
        session_path: session_path.into(),
        store: Arc::clone(&store),
        write_bash_launch_manifest: None,
    });
    Fixture {
        _directory: None,
        control_db_path: String::new(),
        session_path: String::new(),
        store,
        controller,
    }
}

#[test]
fn reserves_distinct_jobs_without_reusing_retained_artifact_directories() {
    let fixture = fixture("supervisor-1");
    let retained = fixture
        .controller
        .create_artifacts(&JsString::from("bash_1"))
        .expect("retained");
    let marker_path = std::path::Path::new(&encode_node_utf8(&retained.directory)).join("retained-marker");
    fs::write(&marker_path, "preserve this evidence").expect("marker");

    let first = fixture
        .controller
        .reserve_job(DetachedJobAgentType::Bash)
        .expect("first");
    let second = fixture
        .controller
        .reserve_job(DetachedJobAgentType::Bash)
        .expect("second");
    assert_eq!(first.job_id, "bash_2");
    assert_eq!(second.job_id, "bash_3");
    assert_ne!(first.artifacts.directory, retained.directory);
    assert_ne!(first.artifacts.directory, second.artifacts.directory);
    assert_ne!(first.artifacts.output_path, second.artifacts.output_path);
    assert!(std::path::Path::new(&encode_node_utf8(&first.artifacts.directory)).exists());
    assert!(std::path::Path::new(&encode_node_utf8(&second.artifacts.directory)).exists());
    assert_eq!(
        fs::read_to_string(&marker_path).expect("marker"),
        "preserve this evidence"
    );
    assert!(std::path::Path::new(&encode_node_utf8(&retained.directory)).exists());
}

#[test]
fn preserves_retained_artifacts_when_launch_manifest_persistence_fails() {
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
        now: Arc::new(|| NOW.into()),
        process_identity: current_identity(),
        session_path: session_path.clone().into(),
    }));
    let controller = DetachedJobLifecycleController::new(DetachedJobLifecycleControllerOptions {
        artifact_root: root.into(),
        control_db_path: control_db_path.into(),
        coordinator,
        owner_agent_id: None,
        owner_session_id: "supervisor-1".into(),
        session_path: session_path.into(),
        store: Arc::clone(&store),
        write_bash_launch_manifest: Some(Arc::new(|_, _| Err("manifest disk failure".into()))),
    });
    let retained = controller
        .create_artifacts(&JsString::from("bash_1"))
        .expect("retained");
    let marker_path = std::path::Path::new(&encode_node_utf8(&retained.directory)).join("retained-marker");
    fs::write(&marker_path, "preserve this evidence").expect("marker");

    let error = controller
        .launch_bash(&pi_coding_agent::core::detached_job_runner::LaunchDetachedBashInput {
            args: vec!["-e".into(), "setInterval(() => {}, 1000)".into()],
            command: "/bin/sh".into(),
            cwd: "/repo".into(),
            env: Vec::new(),
            timeout_ms: None,
            tool_call_id: None,
        })
        .expect_err("manifest failure");
    assert_eq!(error.to_string(), "manifest disk failure");
    assert_eq!(
        fs::read_to_string(&marker_path).expect("marker"),
        "preserve this evidence"
    );
    assert_eq!(store.list_agents().len(), 0);
}

#[test]
fn binds_preallocated_artifacts_ownership_projection_and_exact_finalization() {
    let fixture = fixture("supervisor-1");
    let job_id = fixture.controller.allocate_job_id(DetachedJobAgentType::Bash).unwrap();
    let artifacts = fixture.controller.create_artifacts(&job_id).expect("artifacts");
    let ownership = fixture
        .controller
        .register(&RegisterDetachedJobInput {
            agent_type: DetachedJobAgentType::Bash,
            cwd: "/repo".into(),
            display_name: "Bash command".into(),
            job_id: job_id.clone(),
            process_identity: test_process_identity("runner"),
            worker_handle_id: "123".into(),
            tool_call_id: None,
            detached: None,
            file_refs: None,
        })
        .expect("register");
    assert_eq!(ownership.agent.id, job_id);
    assert_eq!(
        ownership.agent.lifecycle,
        pi_coding_agent::core::multi_agent_store::AgentLifecycle::Running
    );
    assert_eq!(ownership.agent.revision, 1);
    assert_eq!(ownership.artifacts, artifacts);
    assert_eq!(
        fixture.store.get_agent(&job_id).expect("agent").lifecycle,
        pi_coding_agent::core::multi_agent_store::AgentLifecycle::Running
    );

    fs::write(encode_node_utf8(&artifacts.output_path), "done").expect("output");
    let terminal = create_detached_job_terminal_input(
        &artifacts,
        &ownership.identity,
        DetachedJobOutcome::Completed {
            exit_code: Some(0),
            summary: Some("done".into()),
        },
        &JsString::from("2026-07-11T22:00:30.000Z"),
        None,
        None,
    )
    .expect("terminal");
    let finalized = finalize_detached_job(
        &fixture.control_db_path,
        &FinalizeDetachedJobInput {
            session_path: fixture.session_path.clone().into(),
            terminal,
        },
    )
    .expect("finalize");
    let FinalizeDetachedJobResult::Ok {
        terminal_agent,
        terminal_revision,
    } = finalized
    else {
        panic!("expected terminal result");
    };
    assert_eq!(terminal_agent.id, job_id);
    assert_eq!(terminal_agent.revision, 2);
    assert_eq!(terminal_revision, 2);
}

#[test]
fn finds_the_active_bash_attempt_for_a_repeated_tool_call() {
    let fixture = fixture("supervisor-1");
    let tool_call_id = "tool-retried";
    let first_job_id = fixture.controller.allocate_job_id(DetachedJobAgentType::Bash).unwrap();
    let first_artifacts = fixture.controller.create_artifacts(&first_job_id).expect("artifacts");
    let first = fixture
        .controller
        .register(&RegisterDetachedJobInput {
            agent_type: DetachedJobAgentType::Bash,
            cwd: "/repo".into(),
            display_name: "Bash command".into(),
            job_id: first_job_id.clone(),
            process_identity: test_process_identity("runner-first"),
            worker_handle_id: "runner-first".into(),
            tool_call_id: Some(tool_call_id.into()),
            detached: None,
            file_refs: None,
        })
        .expect("register");
    fs::write(encode_node_utf8(&first_artifacts.output_path), "failed").expect("output");
    let terminal = create_detached_job_terminal_input(
        &first_artifacts,
        &first.identity,
        DetachedJobOutcome::Failed {
            error: pi_coding_agent::core::multi_agent_store::AgentError {
                message: "failed".into(),
                code: Some("exit_nonzero".into()),
            },
            exit_code: None,
        },
        &JsString::from("2026-07-11T22:00:10.000Z"),
        None,
        Some(&JsString::from(tool_call_id)),
    )
    .expect("terminal");
    assert!(matches!(
        finalize_detached_job(
            &fixture.control_db_path,
            &FinalizeDetachedJobInput {
                session_path: fixture.session_path.clone().into(),
                terminal,
            },
        )
        .expect("finalize"),
        FinalizeDetachedJobResult::Ok { .. }
    ));

    let current_job_id = fixture.controller.allocate_job_id(DetachedJobAgentType::Bash).unwrap();
    fixture.controller.create_artifacts(&current_job_id).expect("artifacts");
    let current = fixture
        .controller
        .register(&RegisterDetachedJobInput {
            agent_type: DetachedJobAgentType::Bash,
            cwd: "/repo".into(),
            display_name: "Bash command".into(),
            job_id: current_job_id.clone(),
            process_identity: test_process_identity("runner-current"),
            worker_handle_id: "runner-current".into(),
            tool_call_id: Some(tool_call_id.into()),
            detached: None,
            file_refs: None,
        })
        .expect("register");
    assert!(matches!(
        fixture
            .controller
            .cancel(&current, Some(&JsString::from("cancel current attempt")))
            .expect("cancel"),
        LifecycleCommandResult::Ok { .. }
    ));
    let found = fixture
        .controller
        .find_bash_job_by_tool_call_id(&JsString::from(tool_call_id))
        .expect("find")
        .expect("candidate");
    assert_eq!(found.id, current_job_id);
    assert_eq!(
        found.lifecycle,
        pi_coding_agent::core::multi_agent_store::AgentLifecycle::Cancelling
    );
}

#[test]
fn requests_detached_cancellation_and_publishes_cancelling_state() {
    let fixture = fixture("supervisor-1");
    let job_id = fixture.controller.allocate_job_id(DetachedJobAgentType::Bash).unwrap();
    fixture.controller.create_artifacts(&job_id).expect("artifacts");
    let ownership = register_bash(&fixture, &job_id, test_process_identity("runner"));
    let cancelled = fixture
        .controller
        .cancel(&ownership, Some(&JsString::from("user requested")))
        .expect("cancel");
    let LifecycleCommandResult::Ok { agent } = cancelled else {
        panic!("expected cancellation");
    };
    assert_eq!(agent.id, job_id);
    assert_eq!(
        agent.lifecycle,
        pi_coding_agent::core::multi_agent_store::AgentLifecycle::Cancelling
    );
    assert_eq!(agent.revision, 2);
    assert_eq!(
        fixture.store.get_agent(&job_id).expect("agent").lifecycle,
        pi_coding_agent::core::multi_agent_store::AgentLifecycle::Cancelling
    );
}

#[test]
fn recovers_an_active_job_when_its_persisted_runner_identity_is_no_longer_alive() {
    let fixture = fixture("supervisor-1");
    register_listener(&fixture, "supervisor-1");
    let job_id = fixture.controller.allocate_job_id(DetachedJobAgentType::Bash).unwrap();
    fixture.controller.create_artifacts(&job_id).expect("artifacts");
    let mut dead = current_identity();
    dead.start_time_ticks += 1;
    let ownership = register_bash(&fixture, &job_id, dead);
    let observed = fixture
        .controller
        .observe(&ownership.agent.id)
        .expect("observe")
        .expect("agent");
    assert_eq!(
        observed.lifecycle,
        pi_coding_agent::core::multi_agent_store::AgentLifecycle::Failed
    );
    assert_eq!(observed.revision, 2);
    assert_eq!(
        observed.error.as_ref().expect("error").code.as_deref(),
        Some("lost_runtime")
    );
    assert_eq!(
        fixture.store.get_agent(&job_id).expect("agent").lifecycle,
        pi_coding_agent::core::multi_agent_store::AgentLifecycle::Failed
    );
}

#[test]
fn keeps_an_active_job_running_when_its_exact_persisted_runner_identity_is_alive() {
    let fixture = fixture("supervisor-1");
    let job_id = fixture.controller.allocate_job_id(DetachedJobAgentType::Bash).unwrap();
    fixture.controller.create_artifacts(&job_id).expect("artifacts");
    let ownership = register_bash(&fixture, &job_id, current_identity());
    let observed = fixture
        .controller
        .observe(&ownership.agent.id)
        .expect("observe")
        .expect("agent");
    assert_eq!(
        observed.lifecycle,
        pi_coding_agent::core::multi_agent_store::AgentLifecycle::Running
    );
    assert_eq!(observed.revision, 1);
}

#[test]
fn observes_and_publishes_a_terminal_snapshot_committed_by_an_external_runner() {
    let fixture = fixture("supervisor-1");
    let job_id = fixture.controller.allocate_job_id(DetachedJobAgentType::Bash).unwrap();
    let artifacts = fixture.controller.create_artifacts(&job_id).expect("artifacts");
    let ownership = register_bash(&fixture, &job_id, test_process_identity("runner"));
    fs::write(encode_node_utf8(&artifacts.output_path), "done").expect("output");
    let terminal = create_detached_job_terminal_input(
        &artifacts,
        &ownership.identity,
        DetachedJobOutcome::Completed {
            exit_code: Some(0),
            summary: None,
        },
        &JsString::from("2026-07-11T22:00:30.000Z"),
        None,
        None,
    )
    .expect("terminal");
    assert!(matches!(
        finalize_detached_job(
            &fixture.control_db_path,
            &FinalizeDetachedJobInput {
                session_path: fixture.session_path.clone().into(),
                terminal,
            },
        )
        .expect("finalize"),
        FinalizeDetachedJobResult::Ok {
            terminal_revision: 2,
            ..
        }
    ));
    assert_eq!(fixture.store.get_agent(&job_id).expect("agent").revision, 1);
    let observed = fixture.controller.observe(&job_id).expect("observe").expect("agent");
    assert_eq!(
        observed.lifecycle,
        pi_coding_agent::core::multi_agent_store::AgentLifecycle::Completed
    );
    assert_eq!(observed.revision, 2);
    assert_eq!(fixture.store.get_agent(&job_id).expect("agent").revision, 2);
}

fn register_listener(fixture: &Fixture, owner_session_id: &str) {
    let identity = current_identity();
    register_runtime_mailbox_listener(
        &fixture.control_db_path,
        &RuntimeMailboxAddress {
            agent_id: None,
            session_id: owner_session_id.into(),
        },
        identity.pid,
        Some(&JsString::from(&fixture.session_path)),
        RuntimeMailboxRegistrationOptions {
            runtime_instance_id: Some(pi_coding_agent::core::session_control_db::serialize_process_identity(
                &identity,
            )),
            ..Default::default()
        },
    )
    .expect("register listener");
}

#[test]
fn artifact_paths_are_scoped_under_the_detached_jobs_root() {
    let fixture = fixture("supervisor-1");
    let artifacts: DetachedJobArtifacts = fixture
        .controller
        .create_artifacts(&JsString::from("bash_1"))
        .expect("artifacts");
    assert!(artifacts.directory.ends_with("detached-jobs/session/bash_1"));
    assert!(
        artifacts
            .output_path
            .ends_with("detached-jobs/session/bash_1/output.log")
    );
}
