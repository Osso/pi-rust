//! Behavioral port of the architect observer, prompt, project-spec, and main helpers.

use pi_js_compat::js_json::JsString;
use std::sync::{Arc, Mutex};

use pi_coding_agent::architect::main::{
    ARCHITECT_EXCLUDED_TOOL_NAMES, ArchitectConsolePromptQueue, REQUEST_CLAIM_RENEW_INTERVAL_MS, SentArchitectMessage,
    block_architect_global_broadcast, complete_sent_architect_request, create_architect_console_snapshot,
    create_architect_settings_manager, run_architect_cycle,
};
use pi_coding_agent::architect::observer::{
    ArchitectObservation, ArchitectObservationReason, ArchitectObserver, ArchitectSessionSnapshot,
    create_architect_observation, read_architect_snapshot,
};
use pi_coding_agent::architect::project_spec::read_project_spec;
use pi_coding_agent::architect::prompt::{ARCHITECT_SYSTEM_PROMPT, build_architect_prompt};
use pi_coding_agent::core::session_control_db::{ArchitectRequest, ArchitectRequestStatus};
use pi_coding_agent::core::sqlite::create_sqlite_database;
use tempfile::TempDir;

fn session() -> ArchitectSessionSnapshot {
    ArchitectSessionSnapshot {
        cwd: "/repo".to_owned().into(),
        goal_json: Some(r#"{"objective":"Keep tests green"}"#.to_owned().into()),
        id: "main-session".to_owned().into(),
        is_subagent: false,
        name: Some("main".to_owned().into()),
    }
}

fn request(id: i64) -> ArchitectRequest {
    ArchitectRequest {
        id,
        sender_session_id: "main".to_owned().into(),
        project_cwd: None,
        body: "Architect: assess goal drift".to_owned().into(),
        status: ArchitectRequestStatus::Pending,
        created_at: "2026-07-10T00:00:00.000Z".to_owned(),
        claimed_at: None,
        claim_token: None,
        completed_at: None,
    }
}

#[test]
fn emits_initial_state_changes_and_stays_stable() {
    let initial = create_architect_observation(None, &[session()], &[]).unwrap();
    assert_eq!(initial.reason, ArchitectObservationReason::SessionStateChanged);
    assert_eq!(initial.sessions, [session()]);
    assert_eq!(create_architect_observation(Some(&initial), &[session()], &[]), None);

    let mut changed = session();
    changed.goal_json = Some(r#"{"objective":"Deploy safely"}"#.to_owned().into());
    let observation = create_architect_observation(Some(&initial), &[changed.clone()], &[]).unwrap();
    assert_eq!(observation.reason, ArchitectObservationReason::SessionStateChanged);
    assert_eq!(observation.sessions, [changed]);
}

#[test]
fn retains_live_session_across_goal_completion_and_stabilizes() {
    let initial = create_architect_observation(None, &[session()], &[]).unwrap();
    let mut completed = session();
    completed.goal_json = Some(
        r#"{"objective":"Keep tests green","completedAt":"2026-07-10T00:00:00.000Z"}"#
            .to_owned()
            .into(),
    );
    let completion = create_architect_observation(Some(&initial), &[completed.clone()], &[]).unwrap();
    assert_eq!(completion.reason, ArchitectObservationReason::SessionStateChanged);
    assert_eq!(completion.sessions[0].id, "main-session");
    assert_eq!(create_architect_observation(Some(&completion), &[completed], &[]), None);
}

#[test]
fn explicit_requests_take_priority_over_session_changes() {
    let initial = create_architect_observation(None, &[session()], &[]).unwrap();
    let observation = create_architect_observation(Some(&initial), &[session()], &[request(4)]).unwrap();
    assert_eq!(observation.reason, ArchitectObservationReason::ArchitectRequest);
    assert_eq!(observation.requests[0].id, 4);
}

#[test]
fn observer_reads_a_snapshot_on_every_observation() {
    let reads = Arc::new(Mutex::new(vec![
        (Vec::new(), vec![session()]),
        (vec![request(4)], vec![session()]),
    ]));
    let reader = {
        let reads = reads.clone();
        Arc::new(move || {
            let (requests, sessions) = reads.lock().unwrap().remove(0);
            Ok(pi_coding_agent::architect::observer::ArchitectSnapshot { requests, sessions })
        })
    };
    let observer = ArchitectObserver::new(&JsString::from("/unused"), Some(reader)).unwrap();

    let initial = observer.observe().unwrap().unwrap();
    assert_eq!(initial.reason, ArchitectObservationReason::SessionStateChanged);
    assert!(initial.requests.is_empty());
    let next = observer.observe().unwrap().unwrap();
    assert_eq!(next.reason, ArchitectObservationReason::ArchitectRequest);
    assert_eq!(next.requests[0].id, 4);
}

#[test]
fn missing_control_database_returns_empty_without_creating_it() {
    let temp = TempDir::new().unwrap();
    let control_db_path = temp.path().join("missing.sqlite").display().to_string();

    let snapshot = read_architect_snapshot(&JsString::from(&control_db_path)).unwrap();

    assert!(snapshot.requests.is_empty());
    assert!(snapshot.sessions.is_empty());
    assert!(!std::path::Path::new(&control_db_path).exists());
}

fn seed_architect_tables(db: &rusqlite::Connection) {
    db.execute_batch(
        "CREATE TABLE session_metadata (
            session_path TEXT NOT NULL, id TEXT NOT NULL, cwd TEXT NOT NULL, name TEXT, goal_json TEXT,
            is_subagent INTEGER NOT NULL, modified_at TEXT NOT NULL, updated_at TEXT NOT NULL
        );
        CREATE TABLE session_health (
            session_id TEXT NOT NULL, pid INTEGER, check_status TEXT NOT NULL,
            agent_generation INTEGER NOT NULL, checked_generation INTEGER, last_active_at TEXT
        );
        CREATE TABLE runtime_mailbox_listeners (
            recipient_session_id TEXT NOT NULL, recipient_agent_id_key TEXT NOT NULL,
            pid INTEGER NOT NULL, updated_at TEXT NOT NULL
        );",
    )
    .unwrap();
}

#[test]
fn keeps_the_deterministic_newest_live_metadata_row_per_session_id() {
    let temp = TempDir::new().unwrap();
    let control_db_path = temp.path().join("control.sqlite").display().to_string();
    let db = create_sqlite_database(&control_db_path).unwrap();
    seed_architect_tables(&db);
    for (path, id, cwd, name, goal, modified) in [
        (
            "a-old.jsonl",
            "live",
            "/old",
            "old",
            r#"{"objective":"Old"}"#,
            "2026-07-09T20:00:00.000Z",
        ),
        (
            "z-current.jsonl",
            "live",
            "/current",
            "current",
            r#"{"objective":"Current"}"#,
            "2026-07-09T22:00:00.000Z",
        ),
        (
            "a-current-stale.jsonl",
            "live",
            "/stale",
            "stale",
            r#"{"objective":"Stale"}"#,
            "2026-07-09T22:00:00.000Z",
        ),
        (
            "ended.jsonl",
            "ended",
            "/ended",
            "ended",
            r#"{"objective":"Ended"}"#,
            "2026-07-09T22:00:00.000Z",
        ),
    ] {
        db.execute(
            "INSERT INTO session_metadata VALUES (?, ?, ?, ?, ?, 0, ?, ?)",
            rusqlite::params![path, id, cwd, name, goal, modified, modified],
        )
        .unwrap();
    }
    let now = pi_coding_agent::utils::iso_time::now_iso_string();
    db.execute("INSERT INTO session_health VALUES ('live', 123, 'ok', 1, 1, ?)", [&now])
        .unwrap();
    db.execute(
        "INSERT INTO session_health VALUES ('ended', NULL, 'dead', 1, 1, NULL)",
        [],
    )
    .unwrap();
    db.execute(
        "INSERT INTO runtime_mailbox_listeners VALUES ('live', '', 123, '2026-07-09T19:00:00.000Z')",
        [],
    )
    .unwrap();
    db.close().unwrap();

    let snapshot = read_architect_snapshot(&JsString::from(&control_db_path)).unwrap();
    assert_eq!(snapshot.sessions.len(), 1);
    assert_eq!(snapshot.sessions[0].id, "live");
    assert_eq!(snapshot.sessions[0].cwd, "/current");
    assert_eq!(
        snapshot.sessions[0].goal_json.clone(),
        Some(JsString::from(r#"{"objective":"Current"}"#))
    );
    assert!(snapshot.requests.is_empty());
}

#[test]
fn keeps_the_most_recently_registered_main_session_per_live_process() {
    let temp = TempDir::new().unwrap();
    let control_db_path = temp.path().join("control.sqlite").display().to_string();
    let db = create_sqlite_database(&control_db_path).unwrap();
    seed_architect_tables(&db);
    for (path, id, cwd, name, goal, is_subagent, modified) in [
        (
            "historical-main.jsonl",
            "historical-main",
            "/old",
            Some("old"),
            Some(r#"{"objective":"Old"}"#),
            0,
            "2026-07-09T23:00:00.000Z",
        ),
        (
            "current-main.jsonl",
            "current-main",
            "/current",
            Some("current"),
            Some(r#"{"objective":"Current"}"#),
            0,
            "2026-07-09T22:00:00.000Z",
        ),
        (
            "current-child.jsonl",
            "current-child",
            "/current",
            Some("child"),
            None,
            1,
            "2026-07-09T23:30:00.000Z",
        ),
        (
            "other-main.jsonl",
            "other-main",
            "/other",
            Some("other"),
            Some(r#"{"objective":"Other"}"#),
            0,
            "2026-07-09T21:00:00.000Z",
        ),
        (
            "architect.jsonl",
            "architect",
            "/home/osso",
            None,
            None,
            0,
            "2026-07-09T23:30:00.000Z",
        ),
    ] {
        db.execute(
            "INSERT INTO session_metadata VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            rusqlite::params![path, id, cwd, name, goal, is_subagent, modified, modified],
        )
        .unwrap();
    }
    let now = pi_coding_agent::utils::iso_time::now_iso_string();
    for (id, pid) in [
        ("historical-main", 123),
        ("current-main", 123),
        ("current-child", 123),
        ("other-main", 456),
        ("architect", 789),
    ] {
        db.execute(
            "INSERT INTO session_health VALUES (?, ?, 'ok', 1, 1, ?)",
            rusqlite::params![id, pid, now],
        )
        .unwrap();
    }
    for (id, pid, updated_at) in [
        ("historical-main", 123, "2026-07-09T20:00:00.000Z"),
        ("current-main", 123, "2026-07-09T21:00:00.000Z"),
        ("current-child", 123, "2026-07-09T23:30:00.000Z"),
        ("other-main", 456, "2026-07-09T19:00:00.000Z"),
        ("architect", 789, "2026-07-09T23:30:00.000Z"),
    ] {
        db.execute(
            "INSERT INTO runtime_mailbox_listeners VALUES (?, '', ?, ?)",
            rusqlite::params![id, pid, updated_at],
        )
        .unwrap();
    }
    db.close().unwrap();

    let snapshot = read_architect_snapshot(&JsString::from(&control_db_path)).unwrap();
    assert_eq!(
        snapshot
            .sessions
            .iter()
            .map(|session| session.id.clone())
            .collect::<Vec<_>>(),
        ["current-main", "other-main"]
    );
}

#[test]
fn prompt_serializes_the_observation_after_the_bounded_instructions() {
    let observation = ArchitectObservation {
        requests: vec![request(4)],
        reason: ArchitectObservationReason::ArchitectRequest,
        sessions: vec![session()],
    };
    let prompt = build_architect_prompt(&observation);
    let parts: Vec<&str> = prompt.split("\n\n").collect();
    assert_eq!(parts[0], "Architect observation. Assess it now.");
    assert_eq!(
        parts[1],
        "Use this structured observation only for session inventory and liveness; do not call global session inventory tools."
    );
    assert_eq!(
        parts[2],
        "Send explicit-request replies through send_agent_message; send periodic advice only when the system prompt threshold is met."
    );
    let parsed: pi_js_compat::js_json::JsValue = serde_json::from_str(parts[3]).unwrap();
    assert_eq!(parsed["reason"], pi_js_compat::js_json!("architect_request"));
    assert_eq!(parsed["requests"][0]["id"], pi_js_compat::js_json!(4));
    assert_eq!(parsed["sessions"][0]["id"], pi_js_compat::js_json!("main-session"));
    assert_eq!(
        parsed["sessions"][0]["goalJson"],
        pi_js_compat::js_json!(r#"{"objective":"Keep tests green"}"#)
    );
    assert!(ARCHITECT_SYSTEM_PROMPT.contains("threadId \"architect-request:<request id>\""));
    assert!(ARCHITECT_SYSTEM_PROMPT.contains("Never use the global shared channel"));
}

#[test]
fn project_spec_reads_only_markdown_inside_docs_specs() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    std::fs::create_dir_all(root.join("docs/specs")).unwrap();
    std::fs::write(root.join("docs/specs/README.md"), "index").unwrap();
    std::fs::write(root.join("docs/specs/feature.md"), "feature spec").unwrap();
    std::fs::write(root.join("outside.md"), "outside").unwrap();
    std::fs::create_dir_all(root.join("nested")).unwrap();
    let root_text = root.display().to_string();

    assert_eq!(
        futures_block(read_project_spec(&root_text, "feature.md")).unwrap(),
        "feature spec"
    );
    assert_eq!(
        futures_block(read_project_spec(
            &root.join("nested").display().to_string(),
            "README.md"
        ))
        .unwrap(),
        "index"
    );
    assert_eq!(
        futures_block(read_project_spec(&root_text, "../outside.md"))
            .unwrap_err()
            .message,
        "Architect project spec path must reference a relative Markdown file inside docs/specs"
    );
    assert_eq!(
        futures_block(read_project_spec(&root_text, "/etc/passwd"))
            .unwrap_err()
            .message,
        "Architect project spec path must reference a relative Markdown file inside docs/specs"
    );
    assert_eq!(
        futures_block(read_project_spec(&root_text, "README.txt"))
            .unwrap_err()
            .message,
        "Architect project spec path must reference a relative Markdown file inside docs/specs"
    );

    std::fs::create_dir_all(root.join("elsewhere")).unwrap();
    std::fs::write(root.join("elsewhere/escape.md"), "escape").unwrap();
    std::os::unix::fs::symlink(root.join("elsewhere"), root.join("docs/specs/linked")).unwrap();
    assert_eq!(
        futures_block(read_project_spec(&root_text, "linked/escape.md"))
            .unwrap_err()
            .message,
        "Architect project spec path must remain inside docs/specs"
    );
}

#[test]
fn project_spec_replaces_invalid_utf8_in_index_and_content() {
    let temp = TempDir::new().unwrap();
    let specs = temp.path().join("docs/specs");
    std::fs::create_dir_all(&specs).unwrap();
    std::fs::write(specs.join("README.md"), b"index\xff survives").unwrap();
    std::fs::write(specs.join("feature.md"), b"before\xe2\x82after\xff end").unwrap();
    assert_eq!(
        futures_block(read_project_spec(&temp.path().display().to_string(), "feature.md")).unwrap(),
        "before\u{fffd}after\u{fffd} end"
    );
    assert_eq!(
        futures_block(read_project_spec(&temp.path().display().to_string(), "README.md")).unwrap(),
        "index\u{fffd} survives"
    );
}

#[test]
fn project_spec_replaces_invalid_utf8_in_content_with_valid_index() {
    let temp = TempDir::new().unwrap();
    let specs = temp.path().join("docs/specs");
    std::fs::create_dir_all(&specs).unwrap();
    std::fs::write(specs.join("README.md"), "index").unwrap();
    std::fs::write(specs.join("feature.md"), b"before\xe2\x82after\xff end").unwrap();
    assert_eq!(
        futures_block(read_project_spec(&temp.path().display().to_string(), "feature.md")).unwrap(),
        "before\u{fffd}after\u{fffd} end"
    );
}

#[test]
fn project_spec_reports_a_missing_spec_index() {
    let temp = TempDir::new().unwrap();
    let error = futures_block(read_project_spec(&temp.path().display().to_string(), "README.md")).unwrap_err();
    assert!(
        error
            .message
            .starts_with("No docs/specs/README.md found above Architect project cwd: ")
    );
}

fn futures_block<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}

struct CountingCompleter {
    completed: Mutex<Vec<(i64, JsString)>>,
}

impl pi_coding_agent::architect::main::ArchitectRequestCompleter for CountingCompleter {
    fn complete_request(&self, request_id: i64, sender_session_id: &JsString) -> Result<(), JsString> {
        self.completed
            .lock()
            .unwrap()
            .push((request_id, sender_session_id.to_owned()));
        Ok(())
    }
}

#[test]
fn completes_only_targeted_architect_request_threads() {
    let completer = CountingCompleter {
        completed: Mutex::new(Vec::new()),
    };
    complete_sent_architect_request(
        &completer,
        &SentArchitectMessage {
            thread_id: Some("architect-request:4".to_owned().into()),
            to_agent_id: "main".to_owned().into(),
            to_session_id: "sender-main".to_owned().into(),
        },
    )
    .unwrap();
    complete_sent_architect_request(
        &completer,
        &SentArchitectMessage {
            thread_id: Some("other-thread:4".to_owned().into()),
            to_agent_id: "main".to_owned().into(),
            to_session_id: "sender-main".to_owned().into(),
        },
    )
    .unwrap();
    complete_sent_architect_request(
        &completer,
        &SentArchitectMessage {
            thread_id: Some("architect-request:4".to_owned().into()),
            to_agent_id: "child".to_owned().into(),
            to_session_id: "sender-main".to_owned().into(),
        },
    )
    .unwrap();
    complete_sent_architect_request(
        &completer,
        &SentArchitectMessage {
            thread_id: Some("architect-request:0".to_owned().into()),
            to_agent_id: "main".to_owned().into(),
            to_session_id: "sender-main".to_owned().into(),
        },
    )
    .unwrap();
    assert_eq!(
        *completer.completed.lock().unwrap(),
        [(4, JsString::from("sender-main"))]
    );
}

#[test]
fn blocks_global_broadcast_tools_for_the_architect() {
    for (tool, reason) in [
        (
            "channel_post",
            "Pi Architect must use direct agent messaging, not channel_post.",
        ),
        (
            "list_sessions",
            "Pi Architect must use its bounded structured observer snapshot, not list_sessions.",
        ),
    ] {
        let blocked = block_architect_global_broadcast(tool).unwrap();
        assert_eq!(blocked.block, Some(true));
        assert_eq!(blocked.reason.clone(), Some(JsString::from(reason)));
    }
    assert!(block_architect_global_broadcast("read").is_none());
    assert_eq!(ARCHITECT_EXCLUDED_TOOL_NAMES.len(), 4);
}

#[test]
fn console_snapshot_and_settings_match_the_service_contract() {
    let snapshot = create_architect_console_snapshot(&JsString::from("/home"), 42, None, Vec::new());
    assert_eq!(
        snapshot.service,
        pi_coding_agent::core::resident_console_transport::ResidentConsoleService::Architect
    );
    assert_eq!(snapshot.session_id, "architect");
    assert_eq!(snapshot.generation, 42);
    let settings = create_architect_settings_manager();
    assert_eq!(
        settings.get_explicit_sandbox_profile(),
        Some(pi_coding_agent::core::permissions::presets::SandboxProfileName::ReadOnly)
    );
}

#[tokio::test]
async fn console_prompt_queue_is_fifo_and_wakes_waiters() {
    let queue = Arc::new(ArchitectConsolePromptQueue::new());
    let controller_signal = pi_js_compat::abort::AbortController::new();
    let signal = controller_signal.signal();
    queue.enqueue("first".to_owned().into(), "1".to_owned().into());
    queue.enqueue("second".to_owned().into(), "2".to_owned().into());
    assert_eq!(queue.take().unwrap().text, "first");
    assert_eq!(queue.take().unwrap().text, "second");
    assert!(queue.take().is_none());

    let waiting = {
        let queue = queue.clone();
        let signal = signal.clone();
        tokio::spawn(async move {
            queue.wait(&signal, 10_000).await;
        })
    };
    tokio::task::yield_now().await;
    queue.enqueue("third".to_owned().into(), "3".to_owned().into());
    tokio::time::timeout(std::time::Duration::from_secs(1), waiting)
        .await
        .expect("waiter wakes")
        .unwrap();
    assert_eq!(queue.take().unwrap().text, "third");
}

struct FixedObserver {
    observations: Mutex<Vec<Option<ArchitectObservation>>>,
    renewed: Mutex<Vec<Vec<i64>>>,
}

impl pi_coding_agent::architect::main::ArchitectCycleObserver for FixedObserver {
    fn observe(&self) -> Result<Option<ArchitectObservation>, JsString> {
        Ok(self.observations.lock().unwrap().remove(0))
    }

    fn renew_requests(&self, request_ids: &[i64]) -> Result<(), JsString> {
        self.renewed.lock().unwrap().push(request_ids.to_vec());
        Ok(())
    }
}

struct FailingRenewalObserver;

impl pi_coding_agent::architect::main::ArchitectCycleObserver for FailingRenewalObserver {
    fn observe(&self) -> Result<Option<ArchitectObservation>, JsString> {
        Ok(Some(ArchitectObservation {
            requests: vec![request(4)],
            reason: ArchitectObservationReason::ArchitectRequest,
            sessions: vec![session()],
        }))
    }

    fn renew_requests(&self, request_ids: &[i64]) -> Result<(), JsString> {
        assert_eq!(request_ids, &[4]);
        Err(JsString::from("renewal failed-") + JsString::from_utf16(vec![0xd800]))
    }
}

#[tokio::test]
async fn architect_cycle_reports_request_renewal_failure_during_prompt() {
    let result = run_architect_cycle(Arc::new(FailingRenewalObserver), |_| async {
        tokio::time::sleep(std::time::Duration::from_millis(REQUEST_CLAIM_RENEW_INTERVAL_MS + 100)).await;
        Ok(())
    })
    .await;
    assert_eq!(
        result,
        Err(JsString::from("renewal failed-") + JsString::from_utf16(vec![0xd800]))
    );
}

#[tokio::test]
async fn architect_cycle_prompts_only_when_an_observation_exists() {
    let observer = Arc::new(FixedObserver {
        observations: Mutex::new(vec![
            None,
            Some(ArchitectObservation {
                requests: vec![request(4)],
                reason: ArchitectObservationReason::ArchitectRequest,
                sessions: vec![session()],
            }),
        ]),
        renewed: Mutex::new(Vec::new()),
    });
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let recorded = prompts.clone();
    run_architect_cycle(observer.clone(), move |prompt: JsString| {
        let recorded = recorded.clone();
        async move {
            recorded.lock().unwrap().push(prompt);
            Ok(())
        }
    })
    .await
    .unwrap();
    assert!(prompts.lock().unwrap().is_empty());

    let prompts_for_closure = prompts.clone();
    run_architect_cycle(observer, move |prompt: JsString| {
        let recorded = prompts_for_closure.clone();
        async move {
            recorded.lock().unwrap().push(prompt);
            Ok(())
        }
    })
    .await
    .unwrap();
    let prompts = prompts.lock().unwrap();
    assert_eq!(prompts.len(), 1);
    assert!(prompts[0].contains("\"architect_request\""));
}

#[tokio::test]
async fn project_spec_tool_registration_reads_specs_and_reports_paths() {
    use pi_agent_core::AgentToolResult;
    use pi_ai::types::{TextContent, TextOrImageContent};
    use pi_coding_agent::architect::project_spec::register_architect_project_spec_tool;
    use pi_coding_agent::core::auth_storage::AuthStorage;
    use pi_coding_agent::core::extensions::ExtensionRunner;
    use pi_coding_agent::core::extensions::loader::ExtensionFactory;
    use pi_coding_agent::core::model_registry::ModelRegistry;
    use pi_coding_agent::core::resource_loader::{
        DefaultResourceLoader, DefaultResourceLoaderOptions, InlineExtension, ResourceLoader,
    };
    use pi_coding_agent::core::session_manager::SessionManager;
    use pi_js_compat::js_json as json;
    use pi_js_compat::js_json::JsObject as Map;

    let project = TempDir::new().unwrap();
    let agent = TempDir::new().unwrap();
    let specs = project.path().join("docs").join("specs");
    std::fs::create_dir_all(&specs).unwrap();
    std::fs::write(specs.join("README.md"), "spec index").unwrap();
    std::fs::write(specs.join("feature.md"), "feature contract").unwrap();

    let factory: ExtensionFactory = Arc::new(|api| {
        let result = register_architect_project_spec_tool(api);
        Box::pin(async move { result })
    });
    let loader = DefaultResourceLoader::new(DefaultResourceLoaderOptions {
        cwd: project.path().display().to_string().into(),
        agent_dir: agent.path().display().to_string().into(),
        extension_factories: vec![InlineExtension::Factory {
            extension_path: Some("<inline:architect>".to_owned().into()),
            factory,
        }],
        no_extensions: true,
        no_skills: true,
        no_prompt_templates: true,
        no_themes: true,
        no_context_files: true,
        ..Default::default()
    })
    .unwrap();
    loader.reload().await.unwrap();
    let extensions = loader.get_extensions();
    assert!(extensions.errors.is_empty());
    let definition = extensions.extensions[0]
        .read()
        .tools
        .get("read_project_spec")
        .expect("registered tool")
        .definition
        .clone();
    assert_eq!(definition.label, "Read project spec");
    assert_eq!(
        definition.description,
        "Read an authoritative Markdown spec from an explicit Architect request's project."
    );
    assert_eq!(definition.approval_required, Some(false));
    assert_eq!(definition.prompt_guidelines, Some(Vec::new()));
    assert_eq!(
        pi_js_compat::js_json::JsValue::from(definition.parameters),
        json!({
            "type": "object",
            "properties": {
                "project_cwd": {"type": "string", "description": "Exact projectCwd from the Architect request."},
                "spec_path": {"type": "string", "description": "Path relative to docs/specs, such as README.md."}
            },
            "required": ["project_cwd", "spec_path"]
        })
    );

    let model_registry = ModelRegistry::in_memory(AuthStorage::in_memory(Map::new())).unwrap();
    let runner = ExtensionRunner::new(
        extensions.extensions.clone(),
        extensions.runtime.clone(),
        project.path().display().to_string(),
        Arc::new(Mutex::new(
            SessionManager::in_memory(Some(&JsString::from(project.path().display().to_string())), None).unwrap(),
        )),
        Arc::new(model_registry),
        None,
    );
    let context = runner.create_context();
    let result: AgentToolResult = (definition.execute)(
        "call".to_owned(),
        json!({"project_cwd": project.path().display().to_string(), "spec_path": "feature.md"}),
        None,
        None,
        Some(context.clone()),
    )
    .await
    .expect("tool result");
    assert_eq!(
        result.content,
        vec![TextOrImageContent::Text(TextContent::new("feature contract"))]
    );
    assert_eq!(result.details, Some(json!({"path": "feature.md"})));

    let error = (definition.execute)(
        "call".to_owned(),
        json!({"project_cwd": project.path().display().to_string(), "spec_path": "../secret.md"}),
        None,
        None,
        Some(context),
    )
    .await
    .expect_err("escape rejected");
    assert_eq!(
        error.message,
        "Architect project spec path must reference a relative Markdown file inside docs/specs"
    );
}

#[tokio::test]
async fn console_server_exposes_the_architect_snapshot_and_prompt_queue() {
    use pi_ai::providers::faux::{FauxModelDefinition, RegisterFauxProviderOptions, faux_provider};
    use pi_coding_agent::architect::main::{
        ArchitectConsolePrompt, CreateArchitectConsoleServerInput, create_architect_console_server,
    };
    use pi_coding_agent::core::auth_storage::AuthStorage;
    use pi_coding_agent::core::model_registry::ModelRegistry;
    use pi_coding_agent::core::resident_console_transport::{ResidentConsoleClient, ResidentConsoleService};
    use pi_coding_agent::core::resource_loader::StaticResourceLoader;
    use pi_coding_agent::core::sdk::{CreateAgentSessionOptions, create_agent_session};
    use pi_coding_agent::core::session_manager::{NewSessionOptions, SessionEntry, SessionManager};

    let temp = TempDir::new().unwrap();
    let cwd = temp.path().display().to_string();
    let faux = faux_provider(RegisterFauxProviderOptions {
        provider: Some("openai-codex".to_owned()),
        models: Some(vec![FauxModelDefinition {
            id: "gpt-5.6-sol".to_owned(),
            name: Some("Sol".into()),
            reasoning: Some(true),
            input: None,
            cost: None,
            context_window: Some(200_000.0),
            max_tokens: Some(8192.0),
        }]),
        ..Default::default()
    });
    let runtime = ModelRegistry::in_memory(AuthStorage::in_memory(pi_js_compat::js_json::JsObject::new())).unwrap();
    scripted_registry::register_scripted_provider(&runtime, Arc::clone(&faux.provider));
    let model = runtime.find("openai-codex", "gpt-5.6-sol").unwrap();
    let session_manager = SessionManager::create(
        &JsString::from(&cwd),
        Some(&JsString::from(temp.path().join("sessions").display().to_string())),
        Some(NewSessionOptions {
            id: Some("architect".to_owned().into()),
            ..Default::default()
        }),
    )
    .unwrap();
    let session = create_agent_session(CreateAgentSessionOptions {
        cwd: Some(cwd.into()),
        agent_dir: Some(temp.path().display().to_string().into()),
        control_db_path: Some(temp.path().join("control.sqlite").display().to_string().into()),
        model_registry: Some(runtime),
        model: Some(model),
        resource_loader: Some(StaticResourceLoader::new()),
        session_manager: Some(session_manager),
        thinking_level: Some(pi_agent_core::harness::types::ThinkingLevel::High),
        ..Default::default()
    })
    .await
    .unwrap()
    .session;
    let control_db_path = temp.path().join("control.sqlite").display().to_string();
    let prompt_queue = Arc::new(ArchitectConsolePromptQueue::new());
    let server = create_architect_console_server(CreateArchitectConsoleServerInput {
        control_db_path: control_db_path.clone().into(),
        cwd: "/fixed/architect".to_owned().into(),
        generation: 17,
        prompt_queue: Arc::clone(&prompt_queue),
        session,
    });
    server.start().await.unwrap();
    let client = ResidentConsoleClient::<SessionEntry, pi_js_compat::js_json::JsValue>::connect(
        &format!("{control_db_path}.architect-console.sock"),
        ResidentConsoleService::Architect,
    )
    .await
    .unwrap();
    let snapshot = client.snapshot();
    assert_eq!(snapshot.service, ResidentConsoleService::Architect);
    assert_eq!(snapshot.session_id, "architect");
    assert_eq!(snapshot.cwd, "/fixed/architect");
    assert_eq!(snapshot.generation, 17);
    client.prompt("prompt-1", "inspect this").await.unwrap();
    assert_eq!(
        prompt_queue.take(),
        Some(ArchitectConsolePrompt {
            id: "prompt-1".to_owned().into(),
            text: "inspect this".to_owned().into(),
        })
    );
    client.close().await;
    server.close().await.unwrap();
}

use crate::shared_scripted_registry as scripted_registry;

// Shared support is compiled once per integration target.
#[path = "support/scripted_registry.rs"]
pub mod shared_scripted_registry;
