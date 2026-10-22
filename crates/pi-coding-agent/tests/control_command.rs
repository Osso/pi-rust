//! Ports `packages/coding-agent/test/control-command.test.ts`.

use pi_js_compat::js_json::JsString;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use pi_coding_agent::cli::{ControlCommandDependencies, handle_control_command};
use pi_coding_agent::core::session_control_db::{
    claim_latest_incoming_message, get_control_db_path, write_last_message, write_session_health,
};
use pi_coding_agent::core::session_health::{SessionCheckStatus, SessionHealthRecord};

struct Harness {
    control_db_path: JsString,
    stdout: Mutex<Vec<String>>,
    stderr: Mutex<Vec<String>>,
    signals: Mutex<Vec<u32>>,
}

impl Harness {
    fn new(dir: &Path) -> Self {
        Self {
            control_db_path: get_control_db_path(dir).into(),
            stdout: Mutex::new(Vec::new()),
            stderr: Mutex::new(Vec::new()),
            signals: Mutex::new(Vec::new()),
        }
    }
}

impl ControlCommandDependencies for Harness {
    fn stdout(&self, text: &str) {
        self.stdout.lock().unwrap().push(text.to_owned());
    }

    fn stderr(&self, text: &str) {
        self.stderr.lock().unwrap().push(text.to_owned());
    }

    fn control_db_path(&self) -> JsString {
        self.control_db_path.clone()
    }

    fn signal_process(&self, pid: u32) -> Result<(), String> {
        self.signals.lock().unwrap().push(pid);
        Ok(())
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("pi-control-command-{name}-{}-{unique}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn health(session_id: &str, pid: u32) -> SessionHealthRecord {
    SessionHealthRecord {
        session_id: session_id.to_owned().into(),
        agent_generation: 5,
        pid: Some(pid),
        last_active_at: Some("2026-07-15T00:48:35.798Z".to_owned()),
        last_checked_at: Some("2026-07-15T00:48:35.798Z".to_owned()),
        check_status: SessionCheckStatus::Ok,
        checked_generation: Some(5),
        check_latency_ms: Some(0),
        updated_at: "2026-07-15T00:48:35.798Z".to_owned(),
    }
}

#[test]
fn queues_a_control_message_and_signals_a_running_process() {
    let dir = temp_dir("send");
    let harness = Harness::new(&dir);
    let outcome = handle_control_command(&args(&["control", "send", "--pid", "1234", "finish now"]), &harness).unwrap();

    assert!(outcome.handled);
    assert_eq!(outcome.exit_code, None);
    assert_eq!(*harness.stdout.lock().unwrap(), vec!["queued 1\n", "signaled 1234\n"]);
    assert!(harness.stderr.lock().unwrap().is_empty());
    assert_eq!(*harness.signals.lock().unwrap(), vec![1234]);
    assert_eq!(
        claim_latest_incoming_message(&harness.control_db_path)
            .unwrap()
            .map(|message| message.content),
        Some("finish now".into())
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn restarts_a_running_session_by_exact_session_id() {
    let dir = temp_dir("restart");
    let harness = Harness::new(&dir);
    let session_id = "019f626e-bd57-7f89-ae55-9541228b8edb";
    write_session_health(&harness.control_db_path, &health(session_id, 870429)).unwrap();

    let outcome = handle_control_command(&args(&["control", "restart", "--session-id", session_id]), &harness).unwrap();

    assert!(outcome.handled);
    assert_eq!(outcome.exit_code, None);
    assert_eq!(
        *harness.stdout.lock().unwrap(),
        vec![format!("signaled session {session_id} (pid 870429)\n")]
    );
    assert!(harness.stderr.lock().unwrap().is_empty());
    assert_eq!(*harness.signals.lock().unwrap(), vec![870429]);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn reports_restart_of_unknown_or_dead_sessions() {
    let dir = temp_dir("restart-missing");
    let harness = Harness::new(&dir);
    let outcome = handle_control_command(
        &args(&["control", "restart", "--session-id", "missing-session"]),
        &harness,
    )
    .unwrap();
    assert_eq!(outcome.exit_code, Some(1));
    assert_eq!(
        *harness.stderr.lock().unwrap(),
        vec!["Session missing-session is not running.\n"]
    );

    let dead = health("dead-session", 4242);
    write_session_health(
        &harness.control_db_path,
        &SessionHealthRecord {
            check_status: SessionCheckStatus::Dead,
            ..dead
        },
    )
    .unwrap();
    let outcome =
        handle_control_command(&args(&["control", "restart", "--session-id", "dead-session"]), &harness).unwrap();
    assert_eq!(outcome.exit_code, Some(1));
    assert!(harness.signals.lock().unwrap().is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn prints_the_last_assistant_reply() {
    let dir = temp_dir("last");
    let harness = Harness::new(&dir);
    write_last_message(&harness.control_db_path, "assistant", "done").unwrap();

    let outcome = handle_control_command(&args(&["control", "last"]), &harness).unwrap();

    assert!(outcome.handled);
    assert_eq!(*harness.stdout.lock().unwrap(), vec!["done\n"]);
    assert!(harness.stderr.lock().unwrap().is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn prints_the_state_root_control_database_path() {
    let dir = temp_dir("path");
    let harness = Harness::new(&dir);
    let outcome = handle_control_command(&args(&["control", "path"]), &harness).unwrap();

    assert!(outcome.handled);
    assert_eq!(
        *harness.stdout.lock().unwrap(),
        vec![format!("{}\n", harness.control_db_path.to_utf8().unwrap())]
    );
    assert!(harness.stderr.lock().unwrap().is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn rejects_invalid_send_arguments() {
    let dir = temp_dir("send-invalid");
    for argv in [
        vec!["control", "send"],
        vec!["control", "send", "--pid", "0", "message"],
        vec!["control", "send", "--pid", "-1", "message"],
        vec!["control", "send", "--pid", "1.5", "message"],
        vec!["control", "send", "--pid"],
        vec!["control", "send", "--pid", "abc", "message"],
        vec!["control", "send", "   "],
    ] {
        let harness = Harness::new(&dir);
        let outcome = handle_control_command(
            &argv.iter().map(|value| (*value).to_owned()).collect::<Vec<_>>(),
            &harness,
        )
        .unwrap();
        assert!(outcome.handled, "argv: {argv:?}");
        assert_eq!(outcome.exit_code, Some(1), "argv: {argv:?}");
        assert!(harness.stdout.lock().unwrap().is_empty(), "argv: {argv:?}");
        assert_eq!(harness.stderr.lock().unwrap().len(), 1, "argv: {argv:?}");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn ignores_non_control_commands_and_prints_help() {
    let dir = temp_dir("help");
    let harness = Harness::new(&dir);
    let outcome = handle_control_command(&args(&["--help"]), &harness).unwrap();
    assert!(!outcome.handled);
    assert_eq!(outcome.exit_code, None);
    assert!(harness.stdout.lock().unwrap().is_empty());
    assert!(harness.stderr.lock().unwrap().is_empty());

    let outcome = handle_control_command(&args(&["control", "--help"]), &harness).unwrap();
    assert_eq!(outcome.exit_code, Some(0));
    assert!(harness.stdout.lock().unwrap()[0].starts_with("Usage:\n  pi control send"));

    let harness = Harness::new(&dir);
    let outcome = handle_control_command(&args(&["control", "bogus"]), &harness).unwrap();
    assert_eq!(outcome.exit_code, Some(1));
    assert_eq!(harness.stderr.lock().unwrap().len(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}
