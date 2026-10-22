//! `core/self-restart.ts` behavioral tests.

use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use std::collections::BTreeMap;
use std::fs;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use pi_agent_core::harness::messages::CustomMessageContent;
use pi_coding_agent::cli::args::Args;
use pi_coding_agent::core::self_restart::{
    ENV_SELF_RESTART_OLD_PID, ENV_SELF_RESTART_PROMPT, ENV_SELF_RESTART_REQUEST, ENV_SELF_RESTART_SESSION,
    RestartTarget, SelfRestartHandoff, SelfRestartRequest, SelfRestartWaitDependencies, append_self_restart_notice,
    apply_self_restart_request, build_self_restart_env, consume_self_restart_request, exec_self_restart,
    get_self_restart_args, restart_current_process, spawn_self_restart, wait_for_self_restart_parent_exit,
};
use pi_coding_agent::core::session_manager::{KnownSessionEntry, SessionEntry, SessionManager, load_entries_from_file};
use tempfile::TempDir;

const TEST_EXEC_MARKER: &str = "PI_TEST_SELF_RESTART_EXEC_DIR";

fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect()
}

fn create_args() -> Args {
    Args {
        continue_session: Some(true),
        diagnostics: Vec::new(),
        file_args: vec!["file.txt".to_owned()],
        fork: Some("branch".to_owned()),
        messages: vec!["old prompt".to_owned()],
        no_session: Some(true),
        resume: Some(true),
        session_id: Some("old-id".to_owned()),
        ..Args::default()
    }
}

#[test]
fn ignores_leaked_restart_session_env_without_a_restart_request_marker_and_clears_it() {
    let mut environment = env(&[
        (ENV_SELF_RESTART_SESSION, "/tmp/session.jsonl"),
        (ENV_SELF_RESTART_PROMPT, "Restarted."),
    ]);
    assert_eq!(consume_self_restart_request(&mut environment, 100, 1), None);
    assert!(!environment.contains_key(ENV_SELF_RESTART_SESSION));
    assert!(!environment.contains_key(ENV_SELF_RESTART_PROMPT));
}

#[test]
fn consumes_an_exec_in_place_restart_request_and_removes_it_from_the_environment() {
    let mut environment = env(&[
        (ENV_SELF_RESTART_REQUEST, "1"),
        (ENV_SELF_RESTART_SESSION, "/tmp/session.jsonl"),
        (ENV_SELF_RESTART_PROMPT, "Restarted."),
        (ENV_SELF_RESTART_OLD_PID, "100"),
    ]);
    let handoff = consume_self_restart_request(&mut environment, 100, 1).expect("handoff");
    assert_eq!(
        handoff,
        SelfRestartHandoff {
            session_file: JsString::from("/tmp/session.jsonl"),
            prompt: Some(JsString::from("Restarted.")),
            old_pid: Some(100),
        }
    );
    assert!(environment.is_empty());
}

#[test]
fn consumes_a_spawn_handoff_whose_old_pid_is_the_parent_process() {
    let mut environment = env(&[
        (ENV_SELF_RESTART_REQUEST, "1"),
        (ENV_SELF_RESTART_SESSION, "/tmp/session.jsonl"),
        (ENV_SELF_RESTART_OLD_PID, "42"),
    ]);
    let handoff = consume_self_restart_request(&mut environment, 100, 42).expect("handoff");
    assert_eq!(handoff.prompt, None);
    assert_eq!(handoff.old_pid, Some(42));
}

#[test]
fn discards_a_restart_request_leaked_from_an_unrelated_process() {
    let mut environment = env(&[
        (ENV_SELF_RESTART_REQUEST, "1"),
        (ENV_SELF_RESTART_SESSION, "/tmp/session.jsonl"),
        (ENV_SELF_RESTART_OLD_PID, "9999"),
    ]);
    assert_eq!(consume_self_restart_request(&mut environment, 100, 42), None);
}

#[test]
fn does_not_override_startup_args_without_a_consumed_request() {
    let mut args = create_args();
    apply_self_restart_request(&mut args, None);
    assert_eq!(args.session, None);
    assert_eq!(args.messages, vec!["old prompt"]);
    assert_eq!(args.file_args, vec!["file.txt"]);
}

#[test]
fn resumes_the_requested_session_without_submitting_the_restart_notice_as_a_prompt() {
    let mut args = create_args();
    apply_self_restart_request(
        &mut args,
        Some(&SelfRestartHandoff {
            session_file: JsString::from("/tmp/session.jsonl"),
            prompt: Some(JsString::from("Restarted.")),
            old_pid: None,
        }),
    );
    assert_eq!(args.session, Some("/tmp/session.jsonl".into()));
    assert!(args.messages.is_empty());
    assert!(args.file_args.is_empty());
    assert_eq!(args.continue_session, Some(true));
    assert_eq!(args.resume, Some(false));
    assert_eq!(args.fork, None);
    assert_eq!(args.no_session, Some(false));
    assert_eq!(args.session_id, None);
}

fn create_session(directory: &TempDir) -> (SessionManager, JsString) {
    let root = directory.path().to_string_lossy().into_owned();
    let sessions = directory.path().join("sessions").to_string_lossy().into_owned();
    let session = SessionManager::create(&JsString::from(&root), Some(&JsString::from(&sessions)), None)
        .expect("session manager");
    let session_file = session.get_session_file().expect("session file").clone();
    (session, session_file)
}

#[test]
fn appends_a_restarted_notice_with_the_current_pid_after_an_exec_in_place_restart() {
    let directory = tempfile::tempdir().expect("temp dir");
    let (mut session, session_file) = create_session(&directory);
    let current_pid = std::process::id();
    append_self_restart_notice(
        &mut session,
        Some(&SelfRestartHandoff {
            session_file: JsString::from(session_file),
            prompt: Some(JsString::from("Restarted.")),
            old_pid: Some(current_pid),
        }),
        current_pid,
    )
    .expect("notice");
    flush_session(&mut session);
    let entries = load_entries_from_file(session.get_session_file().expect("session file")).unwrap();
    let message = custom_message(&entries);
    assert_eq!(message.custom_type, "self_restart");
    assert_eq!(
        message.content,
        CustomMessageContent::Text(format!("Restarted. PID: {current_pid}."))
    );
    assert!(message.display);
}

#[test]
fn appends_a_restarted_notice_with_the_old_and_new_pid_after_a_spawn_handoff() {
    let directory = tempfile::tempdir().expect("temp dir");
    let (mut session, session_file) = create_session(&directory);
    let current_pid = std::process::id();
    append_self_restart_notice(
        &mut session,
        Some(&SelfRestartHandoff {
            session_file: JsString::from(session_file),
            prompt: Some(JsString::from("Restarted.")),
            old_pid: Some(1234),
        }),
        current_pid,
    )
    .expect("notice");
    flush_session(&mut session);
    let entries = load_entries_from_file(session.get_session_file().expect("session file")).unwrap();
    let message = custom_message(&entries);
    assert_eq!(
        message.content,
        CustomMessageContent::Text(format!("Restarted. PID 1234 -> {current_pid}."))
    );
}

#[test]
fn skips_the_notice_without_a_prompt() {
    let directory = tempfile::tempdir().expect("temp dir");
    let (mut session, session_file) = create_session(&directory);
    append_self_restart_notice(
        &mut session,
        Some(&SelfRestartHandoff {
            session_file: JsString::from(session_file),
            prompt: None,
            old_pid: None,
        }),
        std::process::id(),
    )
    .expect("notice");
    let entries = load_entries_from_file(session.get_session_file().expect("session file")).unwrap();
    assert!(!entries.iter().any(|entry| {
        entry
            .entry()
            .is_some_and(|entry| entry.entry_type() == "custom_message")
    }));
}

#[tokio::test]
async fn waits_for_the_old_parent_process_before_continuing_restarted_startup() {
    let alive_checks = Arc::new(AtomicUsize::new(0));
    let sleeps = Arc::new(AtomicUsize::new(0));
    let checks = Arc::clone(&alive_checks);
    let sleep_counter = Arc::clone(&sleeps);
    let dependencies = SelfRestartWaitDependencies {
        current_pid: 100,
        is_process_alive: Arc::new(move |pid| {
            assert_eq!(pid, 1234);
            let count = checks.fetch_add(1, Ordering::SeqCst) + 1;
            count < 3
        }),
        now: {
            let sleeps = Arc::clone(&sleeps);
            Arc::new(move || sleeps.load(Ordering::SeqCst) as i64)
        },
        sleep: Arc::new(move |milliseconds| {
            assert_eq!(milliseconds, 25);
            let sleeps = Arc::clone(&sleep_counter);
            Box::pin(async move {
                sleeps.fetch_add(1, Ordering::SeqCst);
            })
        }),
    };
    wait_for_self_restart_parent_exit(
        Some(&SelfRestartHandoff {
            session_file: JsString::from("/tmp/session.jsonl"),
            prompt: None,
            old_pid: Some(1234),
        }),
        &dependencies,
    )
    .await;
    assert_eq!(alive_checks.load(Ordering::SeqCst), 3);
    assert_eq!(sleeps.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn skips_the_parent_exit_wait_after_an_exec_in_place_restart() {
    let alive_checks = Arc::new(AtomicUsize::new(0));
    let checks = Arc::clone(&alive_checks);
    let dependencies = SelfRestartWaitDependencies {
        current_pid: 100,
        is_process_alive: Arc::new(move |_| {
            checks.fetch_add(1, Ordering::SeqCst);
            true
        }),
        ..SelfRestartWaitDependencies::default()
    };
    wait_for_self_restart_parent_exit(
        Some(&SelfRestartHandoff {
            session_file: JsString::from("/tmp/session.jsonl"),
            prompt: None,
            old_pid: Some(100),
        }),
        &dependencies,
    )
    .await;
    assert_eq!(alive_checks.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn skips_the_parent_exit_wait_when_no_restart_request_was_consumed() {
    let alive_checks = Arc::new(AtomicUsize::new(0));
    let checks = Arc::clone(&alive_checks);
    let dependencies = SelfRestartWaitDependencies {
        is_process_alive: Arc::new(move |_| {
            checks.fetch_add(1, Ordering::SeqCst);
            true
        }),
        ..SelfRestartWaitDependencies::default()
    };
    wait_for_self_restart_parent_exit(None, &dependencies).await;
    assert_eq!(alive_checks.load(Ordering::SeqCst), 0);
}

#[test]
fn does_not_pass_bun_virtual_entrypoint_paths_back_to_compiled_binaries() {
    assert!(
        get_self_restart_args(
            &[],
            &[
                "/usr/bin/pi".to_owned(),
                "/$bunfs/root/pi".to_owned(),
                "/$bunfs/root/pi".to_owned()
            ],
        )
        .is_empty()
    );
    assert_eq!(
        get_self_restart_args(
            &["--import".to_owned(), "file:///loader.mjs".to_owned()],
            &[
                "/usr/bin/node".to_owned(),
                "/repo/packages/coding-agent/src/cli.ts".to_owned(),
                "--flag".to_owned()
            ],
        ),
        vec![
            "--import",
            "file:///loader.mjs",
            "/repo/packages/coding-agent/src/cli.ts",
            "--flag"
        ]
    );
}

#[test]
fn builds_restart_environment_with_the_requested_values() {
    let environment = build_self_restart_env(
        &SelfRestartRequest {
            session_file: JsString::from("/tmp/session.jsonl"),
            prompt: Some(JsString::from("Restarted.")),
            old_pid: None,
        },
        &env(&[("KEEP", "value")]),
        777,
    );
    assert_eq!(environment["KEEP"], "value");
    assert_eq!(environment[ENV_SELF_RESTART_REQUEST], "1");
    assert_eq!(environment[ENV_SELF_RESTART_SESSION], "/tmp/session.jsonl");
    assert_eq!(environment[ENV_SELF_RESTART_PROMPT], "Restarted.");
    assert_eq!(environment[ENV_SELF_RESTART_OLD_PID], "777");
}

fn flush_session(session: &mut SessionManager) {
    session
        .append_message(pi_agent_core::agent::AgentMessage::Custom(
            (serde_json::json!({"role": "assistant", "content": [], "timestamp": 0})
                .as_object()
                .expect("assistant object")
                .clone())
            .into_iter()
            .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
            .collect::<pi_js_compat::js_json::JsObject>(),
        ))
        .expect("flush assistant message");
}

fn custom_message(
    entries: &[pi_coding_agent::core::session_manager::FileEntry],
) -> &pi_coding_agent::core::session_manager::CustomMessageEntry {
    entries
        .iter()
        .find_map(|entry| match entry.entry() {
            Some(SessionEntry::Known(KnownSessionEntry::CustomMessage(message))) => Some(message),
            _ => None,
        })
        .unwrap_or_else(|| panic!("custom message entry in {entries:?}"))
}

#[test]
fn self_restart_exec_entry() {
    let Ok(directory) = std::env::var(TEST_EXEC_MARKER) else {
        return;
    };
    let target = RestartTarget {
        argv: vec![
            "/bin/sh".to_owned(),
            "-c".to_owned(),
            "printf '%s\\n' \"$0\" \"$@\" > \"$PI_TEST_ARGV_FILE\"; env > \"$PI_TEST_ENV_FILE\"".to_owned(),
        ],
        argv0: Some("pi".to_owned()),
        exec_argv: Vec::new(),
        executable: Some("/bin/sh".to_owned()),
    };
    let error = restart_current_process(
        SelfRestartRequest {
            session_file: JsString::from("/tmp/session.jsonl"),
            prompt: Some(JsString::from("Restarted.")),
            old_pid: None,
        },
        &target,
    );
    fs::write(format!("{directory}/exec-error"), error.to_string()).expect("exec error marker");
}

#[test]
fn replaces_the_process_image_in_place_with_the_same_argv_and_a_restart_request_env() {
    let directory = tempfile::tempdir().expect("temp dir");
    let argv_file = directory.path().join("argv.txt");
    let env_file = directory.path().join("env.txt");
    let child = std::process::Command::new(std::env::current_exe().expect("test binary"))
        .args(["--exact", "self_restart_exec_entry", "--nocapture"])
        .env(TEST_EXEC_MARKER, directory.path())
        .env("PI_TEST_ARGV_FILE", &argv_file)
        .env("PI_TEST_ENV_FILE", &env_file)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("child process");
    let child_pid = child.id();
    let status = child.wait_with_output().expect("wait");
    assert!(status.status.success(), "child failed: {status:?}");
    let argv = fs::read_to_string(&argv_file).expect("argv file");
    let argv: Vec<&str> = argv.lines().collect();
    assert_eq!(argv, vec!["pi"]);
    let environment = fs::read_to_string(&env_file).expect("env file");
    assert!(environment.contains(&format!("{ENV_SELF_RESTART_REQUEST}=1")));
    assert!(environment.contains(&format!("{ENV_SELF_RESTART_SESSION}=/tmp/session.jsonl")));
    assert!(environment.contains(&format!("{ENV_SELF_RESTART_PROMPT}=Restarted.")));
    assert!(environment.contains(&format!("{ENV_SELF_RESTART_OLD_PID}={child_pid}")));
}

#[test]
fn rejects_an_exec_restart_when_the_target_cannot_start() {
    let error = exec_self_restart(
        &SelfRestartRequest {
            session_file: JsString::from("/tmp/session.jsonl"),
            prompt: None,
            old_pid: None,
        },
        &RestartTarget {
            executable: Some("/definitely/missing/pi".to_owned()),
            ..RestartTarget::default()
        },
    );
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
}

#[tokio::test]
async fn spawns_a_replacement_process_and_waits_for_its_exit() {
    let directory = tempfile::tempdir().expect("temp dir");
    let env_file = directory.path().join("spawn-env.txt");
    let target = RestartTarget {
        argv: vec![
            "/usr/bin/node".to_owned(),
            "-c".to_owned(),
            format!("env > {}", env_file.to_string_lossy()),
        ],
        executable: Some("/bin/sh".to_owned()),
        ..RestartTarget::default()
    };
    let exit_code = spawn_self_restart(
        &SelfRestartRequest {
            session_file: JsString::from("/tmp/session.jsonl"),
            prompt: Some(JsString::from("Restarted.")),
            old_pid: None,
        },
        &target,
        true,
    )
    .expect("spawn");
    assert_eq!(exit_code, 0);
    let environment = fs::read_to_string(&env_file).expect("env file");
    assert!(environment.contains(&format!("{ENV_SELF_RESTART_REQUEST}=1")));
    assert!(environment.contains(&format!("{ENV_SELF_RESTART_OLD_PID}={}", std::process::id())));
}

#[test]
fn can_return_immediately_after_spawning_the_restarted_child() {
    let directory = tempfile::tempdir().expect("temp dir");
    let marker = directory.path().join("delayed-marker");
    let target = RestartTarget {
        argv: vec![
            "/bin/sh".to_owned(),
            "-c".to_owned(),
            format!("sleep 0.2; printf done > {}", marker.to_string_lossy()),
        ],
        executable: Some("/bin/sh".to_owned()),
        ..RestartTarget::default()
    };
    let started = Instant::now();
    let exit_code = spawn_self_restart(
        &SelfRestartRequest {
            session_file: JsString::from("/tmp/session.jsonl"),
            prompt: None,
            old_pid: None,
        },
        &target,
        false,
    )
    .expect("spawn");
    assert_eq!(exit_code, 0);
    assert!(started.elapsed() < Duration::from_millis(200));
    assert!(!marker.exists());
    let deadline = Instant::now() + Duration::from_secs(10);
    while !marker.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(fs::read_to_string(&marker).expect("restarted child marker"), "done");
}

#[test]
fn rejects_when_the_restarted_child_fails_to_spawn() {
    let error = spawn_self_restart(
        &SelfRestartRequest {
            session_file: JsString::from("/tmp/session.jsonl"),
            prompt: None,
            old_pid: None,
        },
        &RestartTarget {
            executable: Some("/definitely/missing/pi".to_owned()),
            ..RestartTarget::default()
        },
        true,
    )
    .expect_err("spawn failure");
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
}
