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
    RestartEnvironment, SelfRestartDependencies, SelfRestartHandoff, SelfRestartRequest, SelfRestartWaitDependencies,
    append_self_restart_notice, apply_self_restart_request, build_self_restart_env, consume_self_restart_request,
    get_self_restart_args, restart_current_process, spawn_self_restart, wait_for_self_restart_parent_exit,
};
use pi_coding_agent::core::session_manager::{SessionManager, load_entries_from_file};

use crate::shared_oracle as oracle;
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
            session_file,
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
        CustomMessageContent::Text(JsString::from(format!("Restarted. PID: {current_pid}.")))
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
            session_file,
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
        CustomMessageContent::Text(JsString::from(format!("Restarted. PID 1234 -> {current_pid}.")))
    );
}

// Frozen self-restart.ts:207-222: truthy prompt gating and visible persisted
// notices must survive actual SessionManager.open, not just in-memory inspection.
#[test]
fn restart_notice_reopen_matches_the_actual_frozen_module() {
    let mut source = oracle::TsOracle::spawn("self-restart-oracle.mts", "packages/coding-agent/src");
    for (prompt, old_pid) in [
        (Some("Restarted."), None),
        (Some("再開 🦀"), Some(1234)),
        (Some("Restarted."), Some(0)),
        (Some(""), None),
        (None, None),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let expected = source.request(&serde_json::json!({"root":directory.path(),"prompt":prompt,"oldPid":old_pid.map_or_else(||serde_json::json!("same"),|pid|serde_json::json!(pid))}));
        let current_pid = expected["pid"].as_u64().expect("source process pid") as u32;
        let (mut manager, session_file) = create_session(&directory);
        append_self_restart_notice(
            &mut manager,
            Some(&SelfRestartHandoff {
                session_file,
                prompt: prompt.map(JsString::from),
                old_pid: Some(old_pid.unwrap_or(current_pid)),
            }),
            current_pid,
        )
        .unwrap();
        flush_session(&mut manager);
        let reopened = SessionManager::open(manager.get_session_file().unwrap(), None, None).unwrap();
        let notice = reopened.get_entries().iter().find_map(|entry| entry.as_custom_message()).map(|entry| serde_json::json!({"customType":entry.custom_type,"content":entry.content,"display":entry.display}));
        assert_eq!(
            serde_json::json!({"pid":current_pid,"notice":notice}),
            expected,
            "prompt {prompt:?}"
        );
    }
}

#[test]
fn skips_the_notice_without_a_prompt() {
    let directory = tempfile::tempdir().expect("temp dir");
    let (mut session, session_file) = create_session(&directory);
    append_self_restart_notice(
        &mut session,
        Some(&SelfRestartHandoff {
            session_file,
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
            &["/usr/bin/pi".into(), "/$bunfs/root/pi".into(), "/$bunfs/root/pi".into()],
        )
        .is_empty()
    );
    assert_eq!(
        get_self_restart_args(
            &["--import".into(), "file:///loader.mjs".into()],
            &[
                "/usr/bin/node".into(),
                "/repo/packages/coding-agent/src/cli.ts".into(),
                "--flag".into()
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
        &RestartEnvironment::from([("KEEP".into(), "value".into())]),
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
) -> std::borrow::Cow<'_, pi_coding_agent::core::session_manager::CustomMessageEntry> {
    entries
        .iter()
        .find_map(|entry| entry.entry().and_then(|entry| entry.as_custom_message()))
        .unwrap_or_else(|| panic!("custom message entry in {entries:?}"))
}

#[test]
fn self_restart_exec_entry() {
    let Ok(directory) = std::env::var(TEST_EXEC_MARKER) else {
        return;
    };
    if std::env::var(ENV_SELF_RESTART_REQUEST).as_deref() == Ok("1") {
        fs::write(
            std::env::var("PI_TEST_ARGV_FILE").unwrap(),
            std::env::args().next().unwrap() + "\n",
        )
        .unwrap();
        fs::write(std::env::var("PI_TEST_ENV_FILE").unwrap(), process_environment_text()).unwrap();
        return;
    }
    let dependencies = SelfRestartDependencies {
        argv0: Some("pi".into()),
        ..Default::default()
    };
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let error = executor
        .block_on(restart_current_process(
            SelfRestartRequest {
                session_file: JsString::from("/tmp/session.jsonl"),
                prompt: Some(JsString::from("Restarted.")),
                old_pid: None,
            },
            &dependencies,
        ))
        .expect_err("exec must not return");
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
        .env_remove(ENV_SELF_RESTART_REQUEST)
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
    let execve = SelfRestartDependencies::default().execve.unwrap();
    let error = execve(
        &"/definitely/missing/pi".into(),
        &["pi".into()],
        &RestartEnvironment::new(),
    )
    .unwrap_err();
    let error = error
        .original
        .as_ref()
        .unwrap()
        .downcast_ref::<std::io::Error>()
        .unwrap();
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
}

fn process_environment_text() -> String {
    std::env::vars()
        .map(|(key, value)| format!("{key}={value}\n"))
        .collect()
}

#[test]
fn self_restart_spawn_entry() {
    if std::env::var(ENV_SELF_RESTART_REQUEST).as_deref() != Ok("1") {
        return;
    }
    let path = std::env::var(ENV_SELF_RESTART_SESSION).unwrap();
    fs::write(path, process_environment_text()).unwrap();
}

#[test]
fn self_restart_delayed_entry() {
    if std::env::var(ENV_SELF_RESTART_REQUEST).as_deref() != Ok("1") {
        return;
    }
    std::thread::sleep(Duration::from_millis(200));
    fs::write(std::env::var(ENV_SELF_RESTART_SESSION).unwrap(), "done").unwrap();
}

#[tokio::test]
async fn spawns_a_replacement_process_and_waits_for_its_exit() {
    let directory = tempfile::tempdir().expect("temp dir");
    let env_file = directory.path().join("spawn-env.txt");
    let dependencies = SelfRestartDependencies {
        argv: Some(vec![
            "ignored".into(),
            "--exact".into(),
            "self_restart_spawn_entry".into(),
            "--nocapture".into(),
        ]),
        ..Default::default()
    };
    let exit_code = spawn_self_restart(
        &SelfRestartRequest {
            session_file: env_file.to_str().unwrap().into(),
            prompt: Some(JsString::from("Restarted.")),
            old_pid: None,
        },
        &dependencies,
    )
    .await
    .expect("spawn");
    assert_eq!(exit_code, 0.0);
    let environment = fs::read_to_string(&env_file).expect("env file");
    assert!(environment.contains(&format!("{ENV_SELF_RESTART_REQUEST}=1")));
    assert!(environment.contains(&format!("{ENV_SELF_RESTART_OLD_PID}={}", std::process::id())));
}

#[tokio::test]
async fn can_return_immediately_after_spawning_the_restarted_child() {
    let directory = tempfile::tempdir().expect("temp dir");
    let marker = directory.path().join("delayed-marker");
    let dependencies = SelfRestartDependencies {
        argv: Some(vec![
            "ignored".into(),
            "--exact".into(),
            "self_restart_delayed_entry".into(),
            "--nocapture".into(),
        ]),
        wait_for_exit: Some(false),
        ..Default::default()
    };
    let started = Instant::now();
    let exit_code = spawn_self_restart(
        &SelfRestartRequest {
            session_file: marker.to_str().unwrap().into(),
            prompt: None,
            old_pid: None,
        },
        &dependencies,
    )
    .await
    .expect("spawn");
    assert_eq!(exit_code, 0.0);
    assert!(started.elapsed() < Duration::from_millis(200));
    assert!(!marker.exists());
    let deadline = Instant::now() + Duration::from_secs(10);
    while !marker.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(fs::read_to_string(&marker).expect("restarted child marker"), "done");
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
