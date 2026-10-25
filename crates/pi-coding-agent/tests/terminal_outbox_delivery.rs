//! `core/terminal-outbox-delivery.ts` behavioral tests.

use pi_js_compat::js_json::{JsString, encode_node_utf8};
use std::fs;
use std::sync::Arc;

use pi_coding_agent::core::detached_job_runner::{
    DetachedJobAgentType, DetachedJobLifecycleController, DetachedJobLifecycleControllerOptions, DetachedJobOutcome,
    RegisterDetachedJobInput, create_detached_job_terminal_input,
};
use pi_coding_agent::core::lifecycle_coordinator::{LifecycleCoordinator, LifecycleCoordinatorOptions};
use pi_coding_agent::core::multi_agent_store::{
    AgentLifecycle, MultiAgentStore, MultiAgentStoreOptions, SessionPersistence,
};
use pi_coding_agent::core::runtime_process::ProcessIdentity;
use pi_coding_agent::core::session_control_db::{
    FinalizeDetachedJobInput, FinalizeDetachedJobResult, finalize_detached_job, read_multi_agent_agent,
};
use pi_coding_agent::core::terminal_outbox_delivery::{
    DeliverTerminalOutboxOptions, TERMINAL_OUTBOX_CLEANUP_INTERVAL_MS, deliver_terminal_outbox_projections,
    is_terminal_outbox_cleanup_due, terminal_outbox_retention_threshold,
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

struct FixturePersistence {
    control_db_path: String,
    session_path: String,
}

impl SessionPersistence for FixturePersistence {
    fn metadata_control_db_path(&self) -> Option<pi_js_compat::js_json::JsString> {
        Some(self.control_db_path.as_str().into())
    }

    fn session_file(&self) -> Option<pi_js_compat::js_json::JsString> {
        Some(self.session_path.as_str().into())
    }
}

struct Fixture {
    directory: TempDir,
    control_db_path: String,
    session_path: String,
    store: Arc<MultiAgentStore>,
}

fn fixture() -> Fixture {
    let directory = tempfile::tempdir().expect("temp dir");
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
    store.set_persistence_session_manager(Some(&FixturePersistence {
        control_db_path: control_db_path.clone(),
        session_path: session_path.clone(),
    }));
    Fixture {
        directory,
        control_db_path,
        session_path,
        store,
    }
}

fn terminal_job(fixture: &Fixture, job_id: &str) {
    terminal_job_in_root(fixture, job_id, fixture.directory.path().to_str().unwrap());
}

fn terminal_job_in_root(fixture: &Fixture, job_id: &str, root: &str) {
    let coordinator = Arc::new(LifecycleCoordinator::new(LifecycleCoordinatorOptions {
        control_db_path: fixture.control_db_path.clone().into(),
        create_agent_id: Arc::new(|| Ok("unused".to_owned().into())),
        now: Arc::new(|| NOW.to_owned()),
        process_identity: test_process_identity("runtime-1"),
        session_path: fixture.session_path.clone().into(),
    }));
    let controller = DetachedJobLifecycleController::new(DetachedJobLifecycleControllerOptions {
        artifact_root: root.into(),
        control_db_path: fixture.control_db_path.clone().into(),
        coordinator,
        owner_agent_id: None,
        owner_session_id: "supervisor-1".to_owned().into(),
        session_path: fixture.session_path.clone().into(),
        store: Arc::clone(&fixture.store),
        write_bash_launch_manifest: None,
    });
    let artifacts = controller.create_artifacts(&JsString::from(job_id)).expect("artifacts");
    let ownership = controller
        .register(&RegisterDetachedJobInput {
            agent_type: DetachedJobAgentType::Bash,
            cwd: "/repo".to_owned().into(),
            display_name: "Bash command".to_owned().into(),
            job_id: job_id.to_owned().into(),
            process_identity: test_process_identity("runner"),
            worker_handle_id: "runner".to_owned().into(),
            tool_call_id: None,
            detached: Some(true),
            file_refs: None,
        })
        .expect("register");
    fs::write(&encode_node_utf8(&artifacts.output_path), "done").expect("output");
    let terminal = create_detached_job_terminal_input(
        &artifacts,
        &ownership.identity,
        DetachedJobOutcome::Completed {
            exit_code: Some(0),
            summary: Some("done".to_owned().into()),
        },
        // Frozen mailbox retention rejects historical terminal births.
        &JsString::from(&pi_coding_agent::utils::iso_time::now_iso_string()),
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
}

fn options(fixture: &Fixture) -> DeliverTerminalOutboxOptions {
    DeliverTerminalOutboxOptions {
        artifact_root: Some(
            fixture
                .directory
                .path()
                .to_str()
                .expect("utf-8 test path")
                .to_owned()
                .into(),
        ),
        claim_id: "claim-1".to_owned().into(),
        control_db_path: fixture.control_db_path.clone().into(),
        now: Arc::new(|| Ok(NOW.into())),
        store: Arc::clone(&fixture.store),
    }
}

#[test]
fn delivers_a_terminal_projection_and_acknowledges_the_outbox_row() {
    let fixture = fixture();
    terminal_job(&fixture, "agent_1");
    let delivered = deliver_terminal_outbox_projections(&options(&fixture)).expect("deliver");
    assert_eq!(delivered, 1.0);
    let agent = fixture
        .store
        .get_agent(&JsString::from("agent_1"))
        .expect("projected agent");
    assert_eq!(agent.lifecycle, AgentLifecycle::Completed);
    assert_eq!(agent.revision, 2);
    assert_eq!(
        deliver_terminal_outbox_projections(&options(&fixture)).expect("second deliver"),
        0.0
    );
}

#[test]
fn agents_core_factory_root_controls_terminal_artifact_cleanup() {
    use pi_coding_agent::bundled_extensions::agents_core::runtime::deliver_terminal_outbox_for_store;
    use pi_coding_agent::bundled_extensions::agents_core::session_factories::{
        ProductionAttachedSessionFactoryOptions, create_production_attached_session_factory,
    };
    let fixture = fixture();
    let artifact_root = tempfile::tempdir().unwrap();
    terminal_job_in_root(&fixture, "old_job", artifact_root.path().to_str().unwrap());
    // Age an existing terminal artifact without generating an expired mailbox birth.
    let db = pi_coding_agent::core::sqlite::create_sqlite_database(&fixture.control_db_path).unwrap();
    db.execute("UPDATE multi_agent_agents SET data = json_set(data, '$.updatedAt', '2026-01-01T00:00:00.000Z'), updated_at = '2026-01-01T00:00:00.000Z' WHERE agent_id = 'old_job'", []).unwrap();
    drop(db);
    let persisted = read_multi_agent_agent(&fixture.control_db_path, &fixture.session_path, "old_job")
        .unwrap()
        .unwrap();
    let output = &persisted.result.as_ref().unwrap().file_refs.as_ref().unwrap()[0].path;
    let directory = std::path::Path::new(&encode_node_utf8(output))
        .parent()
        .unwrap()
        .to_path_buf();
    assert!(directory.exists());
    let _factory = create_production_attached_session_factory(ProductionAttachedSessionFactoryOptions {
        agent_dir: Some(artifact_root.path().to_str().unwrap().into()),
        browser_agent_config_root: None,
        create_session: Arc::new(|_| Box::pin(async { Err("unused".into()) })),
        extension_factories: None,
        multi_agent_store: Some(Arc::clone(&fixture.store)),
        session_dir: None,
    });
    assert_eq!(deliver_terminal_outbox_for_store(&fixture.store).unwrap(), 1.0);
    assert!(
        !directory.exists(),
        "cleanup must use factory agentDir, not process default root"
    );
}

#[test]
fn skips_delivery_without_a_persistence_target() {
    let fixture = fixture();
    let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default()));
    let delivered = deliver_terminal_outbox_projections(&DeliverTerminalOutboxOptions {
        artifact_root: None,
        claim_id: "claim-1".to_owned().into(),
        control_db_path: fixture.control_db_path.clone().into(),
        now: Arc::new(|| Ok(NOW.into())),
        store,
    })
    .expect("deliver");
    assert_eq!(delivered, 0.0);
}

#[test]
fn fails_a_projection_that_does_not_match_the_persisted_agent() {
    let fixture = fixture();
    // Initialize the schema, then queue an outbox row for an agent that does not exist.
    read_multi_agent_agent(&fixture.control_db_path, &fixture.session_path, "missing").expect("schema");
    let connection =
        pi_coding_agent::core::sqlite::create_sqlite_database(&fixture.control_db_path).expect("open control db");
    connection
        .execute(
            "INSERT INTO multi_agent_terminal_outbox
                (session_path, agent_id, terminal_revision, event_kind, status, attempt_count, updated_at)
             VALUES (?, 'missing', 2, 'completed', 'pending', 0, ?)",
            rusqlite::params![fixture.session_path, NOW],
        )
        .expect("insert outbox row");
    drop(connection);

    let error = deliver_terminal_outbox_projections(&options(&fixture)).expect_err("mismatch");
    assert_eq!(
        error.message,
        "Terminal outbox projection does not match agent missing revision 2"
    );
    let connection =
        pi_coding_agent::core::sqlite::create_sqlite_database(&fixture.control_db_path).expect("open control db");
    let (status, attempts, failure): (String, i64, Option<String>) = connection
        .query_row(
            "SELECT status, attempt_count, last_error FROM multi_agent_terminal_outbox WHERE agent_id = 'missing'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("outbox row");
    assert_eq!(status, "pending");
    assert_eq!(attempts, 1);
    assert_eq!(
        failure.as_deref(),
        Some("Terminal outbox projection does not match agent missing revision 2")
    );
}

#[test]
fn cleanup_is_due_at_the_hourly_interval() {
    assert!(is_terminal_outbox_cleanup_due(None, 1_000.0));
    assert!(!is_terminal_outbox_cleanup_due(
        Some(1_000.0),
        (1_000 + TERMINAL_OUTBOX_CLEANUP_INTERVAL_MS - 1) as f64
    ));
    assert!(is_terminal_outbox_cleanup_due(
        Some(1_000.0),
        (1_000 + TERMINAL_OUTBOX_CLEANUP_INTERVAL_MS) as f64
    ));
}

#[test]
fn retention_threshold_is_seven_days_before_now() {
    let now = pi_js_compat::date::date_parse_millis("2026-07-11T22:00:00.000Z").expect("now");
    assert_eq!(
        terminal_outbox_retention_threshold(now as f64).unwrap(),
        "2026-07-04T22:00:00.000Z"
    );
}
