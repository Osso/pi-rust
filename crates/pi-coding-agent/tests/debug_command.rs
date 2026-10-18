//! Ports `packages/coding-agent/test/debug-command.test.ts`.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use pi_ai::request::BoxFuture;
use pi_coding_agent::cli::{DebugCommandDependencies, handle_debug_command};
use pi_coding_agent::core::session_control_db::{get_control_db_path, write_session_health};
use pi_coding_agent::core::session_health::{SessionCheckStatus, SessionHealthRecord};

struct Harness {
    agent_dir: String,
    control_db_path: String,
    stdout: Mutex<Vec<String>>,
    stderr: Mutex<Vec<String>>,
    attachments: Mutex<Vec<(String, String)>>,
    fail_attach: bool,
}

impl DebugCommandDependencies for Harness {
    fn agent_dir(&self) -> String {
        self.agent_dir.clone()
    }

    fn control_db_path(&self) -> String {
        self.control_db_path.clone()
    }

    fn stdout(&self, text: &str) {
        self.stdout.lock().unwrap().push(text.to_owned());
    }

    fn stderr(&self, text: &str) {
        self.stderr.lock().unwrap().push(text.to_owned());
    }

    fn attach<'a>(&'a self, socket_path: &'a str, session_id: &'a str) -> BoxFuture<'a, Result<(), String>> {
        self.attachments
            .lock()
            .unwrap()
            .push((socket_path.to_owned(), session_id.to_owned()));
        let fail = self.fail_attach;
        Box::pin(async move { if fail { Err("connect ENOENT".to_owned()) } else { Ok(()) } })
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("pi-debug-command-{name}-{}-{unique}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn make_harness(dir: &Path) -> Harness {
    Harness {
        agent_dir: dir.display().to_string(),
        control_db_path: get_control_db_path(dir),
        stdout: Mutex::new(Vec::new()),
        stderr: Mutex::new(Vec::new()),
        attachments: Mutex::new(Vec::new()),
        fail_attach: false,
    }
}

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

#[test]
fn prints_debug_command_help_successfully() {
    runtime().block_on(async {
        let dir = temp_dir("help");
        let harness = make_harness(&dir);
        let outcome = handle_debug_command(&args(&["debug", "--help"]), &harness).await;

        assert!(outcome.handled);
        assert_eq!(outcome.exit_code, Some(0));
        assert_eq!(
            *harness.stdout.lock().unwrap(),
            vec!["Usage: pi debug attach <session-id>\n"]
        );
        assert!(harness.stderr.lock().unwrap().is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    });
}

#[test]
fn attaches_to_the_debug_socket_owned_by_the_exact_live_session() {
    runtime().block_on(async {
        let dir = temp_dir("attach");
        let harness = make_harness(&dir);
        let session_id = "019f689a-8260-79d9-ad2c-ce6da9a4dd05";
        write_session_health(
            &harness.control_db_path,
            &SessionHealthRecord {
                session_id: session_id.to_owned(),
                agent_generation: 1,
                pid: Some(1131255),
                last_active_at: Some("2026-07-16T00:00:00.000Z".to_owned()),
                last_checked_at: Some("2026-07-16T00:00:00.000Z".to_owned()),
                check_status: SessionCheckStatus::Ok,
                checked_generation: Some(1),
                check_latency_ms: Some(0),
                updated_at: "2026-07-16T00:00:00.000Z".to_owned(),
            },
        )
        .unwrap();

        let outcome = handle_debug_command(&args(&["debug", "attach", session_id]), &harness).await;

        assert!(outcome.handled);
        assert_eq!(outcome.exit_code, None);
        assert_eq!(
            *harness.attachments.lock().unwrap(),
            vec![(
                dir.join("debug").join("1131255.sock").display().to_string(),
                session_id.to_owned()
            )]
        );
        std::fs::remove_dir_all(dir).unwrap();
    });
}

#[test]
fn reports_missing_sessions_and_invalid_usage() {
    runtime().block_on(async {
        let dir = temp_dir("missing");
        let harness = make_harness(&dir);
        let outcome = handle_debug_command(&args(&["debug", "attach", "missing-session"]), &harness).await;
        assert_eq!(outcome.exit_code, Some(1));
        assert_eq!(
            *harness.stderr.lock().unwrap(),
            vec!["Session missing-session is not running.\n"]
        );
        assert!(harness.attachments.lock().unwrap().is_empty());

        let harness = make_harness(&dir);
        let outcome = handle_debug_command(&args(&["debug"]), &harness).await;
        assert_eq!(outcome.exit_code, Some(1));
        assert_eq!(
            *harness.stderr.lock().unwrap(),
            vec!["Usage: pi debug attach <session-id>\n"]
        );

        let harness = make_harness(&dir);
        let outcome = handle_debug_command(&args(&["debug", "attach"]), &harness).await;
        assert_eq!(outcome.exit_code, Some(1));

        let harness = make_harness(&dir);
        let outcome = handle_debug_command(&args(&["status", "attach", "id"]), &harness).await;
        assert!(!outcome.handled);
        std::fs::remove_dir_all(dir).unwrap();
    });
}

#[test]
fn reports_attach_failures_with_the_source_message() {
    runtime().block_on(async {
        let dir = temp_dir("attach-failure");
        let mut harness = make_harness(&dir);
        harness.fail_attach = true;
        let session_id = "019f689a-8260-79d9-ad2c-ce6da9a4dd05";
        write_session_health(
            &harness.control_db_path,
            &SessionHealthRecord {
                session_id: session_id.to_owned(),
                agent_generation: 1,
                pid: Some(77),
                last_active_at: None,
                last_checked_at: None,
                check_status: SessionCheckStatus::Ok,
                checked_generation: None,
                check_latency_ms: None,
                updated_at: "2026-07-16T00:00:00.000Z".to_owned(),
            },
        )
        .unwrap();

        let outcome = handle_debug_command(&args(&["debug", "attach", session_id]), &harness).await;

        assert_eq!(outcome.exit_code, Some(1));
        assert_eq!(
            *harness.stderr.lock().unwrap(),
            vec![format!(
                "Could not attach to debug REPL for session {session_id}: connect ENOENT\n"
            )]
        );
        std::fs::remove_dir_all(dir).unwrap();
    });
}
