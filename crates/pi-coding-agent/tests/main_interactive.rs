//! Interactive startup through a real pty (`main.ts` interactive dispatch,
//! `--resume` picker, first-time setup).
//!
//! `util-linux`'s `script` allocates the pty, so the binary observes
//! `stdin`/`stdout` as TTYs and resolves `interactive` mode. Input is written
//! to the pty on a schedule; `script -e` forwards the child's exit status.

use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::encode_node_utf8;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

#[path = "support/executable_script.rs"]
mod executable_script;

use executable_script::write_executable_script;

const CTRL_C: &str = "\x03";
const ESCAPE: &str = "\x1b";

struct Fixture {
    _temp: tempfile::TempDir,
    work: PathBuf,
    agent_dir: PathBuf,
    state_dir: PathBuf,
    home: PathBuf,
    package_dir: PathBuf,
}

fn fixture() -> Fixture {
    let temp = tempfile::tempdir().expect("temp dir");
    let work = temp.path().join("work");
    let agent_dir = temp.path().join("agent");
    let state_dir = temp.path().join("state");
    let home = temp.path().join("home");
    let package_dir = temp.path().join("package");
    for directory in [&work, &agent_dir, &state_dir, &home, &package_dir] {
        std::fs::create_dir_all(directory).expect("fixture directory");
    }
    std::fs::write(
        package_dir.join("package.json"),
        r#"{"name":"@earendil-works/pi-coding-agent","version":"0.84.1"}"#,
    )
    .expect("package.json");
    copy_directory(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("theme"),
        &package_dir.join("theme"),
    );
    Fixture {
        _temp: temp,
        work,
        agent_dir,
        state_dir,
        home,
        package_dir,
    }
}

fn copy_directory(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).expect("theme directory");
    for entry in std::fs::read_dir(source).expect("theme entries") {
        let entry = entry.expect("theme entry");
        std::fs::copy(entry.path(), target.join(entry.file_name())).expect("theme file copy");
    }
}

fn base_environment(fixture: &Fixture) -> Vec<(String, String)> {
    vec![
        ("HOME".to_owned(), fixture.home.display().to_string()),
        (
            "XDG_CONFIG_HOME".to_owned(),
            fixture.home.join(".config").display().to_string(),
        ),
        ("PI_PACKAGE_DIR".to_owned(), fixture.package_dir.display().to_string()),
        ("NO_COLOR".to_owned(), "1".to_owned()),
        ("PI_OFFLINE".to_owned(), "1".to_owned()),
    ]
}

fn agent_environment(fixture: &Fixture) -> Vec<(String, String)> {
    let mut environment = base_environment(fixture);
    environment.push((
        "PI_CODING_AGENT_DIR".to_owned(),
        fixture.agent_dir.display().to_string(),
    ));
    environment.push((
        "PI_CODING_AGENT_STATE_DIR".to_owned(),
        fixture.state_dir.display().to_string(),
    ));
    environment
}

struct PtyOutcome {
    code: i32,
    stdout: String,
}

/// One scheduled pty write: wait until the rendered output contains
/// `wait_for`, pause briefly, then write `send`.
struct PtyInput {
    wait_for: &'static str,
    send: &'static str,
    pause: Duration,
}

fn input(wait_for: &'static str, send: &'static str) -> PtyInput {
    PtyInput {
        wait_for,
        send,
        pause: Duration::from_millis(150),
    }
}

/// Runs the `pi` binary under `script` and writes `inputs` once their markers
/// appear, so parallel test load cannot race the TUI startup.
fn run_under_pty(
    fixture: &Fixture,
    args: &[&str],
    environment: &[(String, String)],
    inputs: &[PtyInput],
) -> PtyOutcome {
    let binary = env!("CARGO_BIN_EXE_pi");
    let mut command_line = binary.to_owned();
    for arg in args {
        command_line.push(' ');
        command_line.push_str(arg);
    }
    let (mut child, output, reader) = spawn_pty_command(fixture, &command_line, environment);

    let mut stdin = child.stdin.take().expect("pty stdin");
    for step in inputs {
        wait_for_marker(&output, step.wait_for);
        std::thread::sleep(step.pause);
        if let Err(error) = stdin.write_all(step.send.as_bytes()) {
            if error.kind() == std::io::ErrorKind::BrokenPipe {
                break;
            }
            panic!("pty input: {error}");
        }
        stdin.flush().ok();
    }
    drop(stdin);

    let code = wait_with_timeout(&mut child, Duration::from_secs(25));
    reader.join().expect("pty reader");
    let stdout = output.lock().expect("pty buffer").clone();
    PtyOutcome { code, stdout }
}

fn wait_for_marker(output: &std::sync::Arc<std::sync::Mutex<String>>, marker: &str) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if output.lock().expect("pty buffer").contains(marker) {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!(
        "marker {marker:?} not rendered; output:\n{}",
        output.lock().expect("pty buffer")
    );
}

fn wait_with_timeout(child: &mut Child, timeout: Duration) -> i32 {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().expect("child status") {
            return status.code().unwrap_or(-1);
        }
        if Instant::now() >= deadline {
            child.kill().ok();
            child.wait().ok();
            panic!("interactive process did not exit within {timeout:?}");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn visible(output: &str) -> JsString {
    pi_tui::utils::strip_terminal_sequences(output)
}

fn double_ctrl_c(marker: &'static str) -> Vec<PtyInput> {
    vec![input(marker, CTRL_C), input(marker, CTRL_C)]
}

#[test]
fn package_list_prompts_for_project_trust_on_the_real_terminal() {
    let fixture = fixture();
    std::fs::create_dir_all(fixture.work.join(".pi")).unwrap();
    std::fs::write(fixture.work.join(".pi/settings.json"), r#"{"packages":[]}"#).unwrap();
    let outcome = run_under_pty(
        &fixture,
        &["list"],
        &agent_environment(&fixture),
        &[input("Trust project folder?", "\r")],
    );
    let text = visible(&outcome.stdout);
    assert_eq!(outcome.code, 0, "{text:?}");
    assert!(text.contains("Trust project folder?"), "{text:?}");
    let trust =
        pi_coding_agent::core::trust_manager::ProjectTrustStore::new(&fixture.agent_dir.to_str().unwrap().into())
            .unwrap();
    assert_eq!(
        trust.get(&fixture.work.to_str().unwrap().into()).unwrap(),
        pi_coding_agent::core::trust_manager::ProjectTrustDecision::Trusted
    );
}

#[test]
fn main_runtime_prompts_for_project_trust_on_the_real_terminal() {
    let fixture = fixture();
    std::fs::create_dir_all(fixture.work.join(".pi")).unwrap();
    std::fs::write(fixture.work.join(".pi/settings.json"), "{}").unwrap();
    let mut inputs = vec![input("Trust project folder?", "\r")];
    inputs.extend(double_ctrl_c("to show full startup help and loaded resources."));
    let outcome = run_under_pty(&fixture, &[], &agent_environment(&fixture), &inputs);
    let text = visible(&outcome.stdout);
    assert_eq!(outcome.code, 0, "{text:?}");
    assert!(text.contains("Trust project folder?"), "{text:?}");
    let trust =
        pi_coding_agent::core::trust_manager::ProjectTrustStore::new(&fixture.agent_dir.to_str().unwrap().into())
            .unwrap();
    assert_eq!(
        trust.get(&fixture.work.to_str().unwrap().into()).unwrap(),
        pi_coding_agent::core::trust_manager::ProjectTrustDecision::Trusted
    );
}

#[test]
fn interactive_mode_starts_the_tui_and_exits_on_double_ctrl_c() {
    let fixture = fixture();
    let outcome = run_under_pty(
        &fixture,
        &[],
        &agent_environment(&fixture),
        &double_ctrl_c("to show full startup help and loaded resources."),
    );
    let text = visible(&outcome.stdout);
    assert_eq!(outcome.code, 0, "stdout:\n{text:?}");
    assert!(
        text.contains("to show full startup help and loaded resources."),
        "stdout:\n{text:?}"
    );
    assert!(text.contains("Pi can explain its own features"), "stdout:\n{text:?}");
    assert!(!text.contains("Interactive mode is not ported"));
}

#[test]
fn resume_flag_opens_the_session_picker_and_cancels_with_escape() {
    let fixture = fixture();
    let outcome = run_under_pty(
        &fixture,
        &["--resume"],
        &agent_environment(&fixture),
        // Startup theme detection reads terminal replies for up to 100 ms and
        // would swallow an immediate ESC; the cancel handler ignores repeats.
        &[
            PtyInput {
                wait_for: "Resume Session",
                send: ESCAPE,
                pause: Duration::from_millis(500),
            },
            input("Resume Session", ESCAPE),
        ],
    );
    let text = visible(&outcome.stdout);
    assert_eq!(outcome.code, 0, "stdout:\n{text:?}");
    assert!(text.contains("Resume Session"), "stdout:\n{text:?}");
    assert!(text.contains("No sessions in current folder"), "stdout:\n{text:?}");
    assert!(text.contains("No session selected"), "stdout:\n{text:?}");
}

#[test]
fn first_time_setup_persists_theme_and_analytics() {
    let fixture = fixture();
    let mut environment = base_environment(&fixture);
    environment.push(("PI_EXPERIMENTAL".to_owned(), "1".to_owned()));
    environment.push((
        "PI_CODING_AGENT_STATE_DIR".to_owned(),
        fixture.state_dir.display().to_string(),
    ));
    let inputs = vec![
        input("Pick a theme.", "j"),
        input("Pick a theme.", "\r"),
        input("Opt-in to anonymous usage data sharing?", "\r"),
        input("to show full startup help and loaded resources.", CTRL_C),
        input("to show full startup help and loaded resources.", CTRL_C),
    ];
    let outcome = run_under_pty(&fixture, &[], &environment, &inputs);
    let text = visible(&outcome.stdout);
    assert_eq!(outcome.code, 0, "stdout:\n{text:?}");
    assert!(text.contains("Welcome to pi"), "stdout:\n{text:?}");
    assert!(text.contains("Pick a theme."), "stdout:\n{text:?}");
    assert!(
        text.contains("to show full startup help and loaded resources."),
        "stdout:\n{text:?}"
    );

    let settings_path = fixture.home.join(".config/pi/agent/settings.json");
    let settings = std::fs::read_to_string(&settings_path).expect("settings.json written");
    assert!(settings.contains(r#""theme": "light""#), "{settings}");
    assert!(settings.contains(r#""enableAnalytics": true"#), "{settings}");
}

#[test]
fn import_failure_exits_through_the_fatal_runtime_handler() {
    let fixture = fixture();
    std::fs::create_dir(fixture.work.join("bad-session")).expect("unreadable session path");
    let inputs = vec![
        input(
            "to show full startup help and loaded resources.",
            "/import bad-session\r",
        ),
        input("to show full startup help and loaded resources.", ""),
    ];
    let outcome = run_under_pty(&fixture, &[], &agent_environment(&fixture), &inputs);
    let text = visible(&outcome.stdout);
    assert_eq!(outcome.code, 1, "stdout:\n{text:?}");
}

#[test]
fn quit_prints_the_resume_command() {
    let fixture = fixture();
    let work = fixture.work.display().to_string();
    let session_dir = pi_coding_agent::core::session_manager::get_default_session_dir(
        &JsString::from(&work),
        Some(&JsString::from(&fixture.agent_dir.display().to_string())),
    )
    .expect("session dir");
    std::fs::create_dir_all(&&encode_node_utf8(&session_dir)).expect("session dir");
    let session_file = Path::new(&encode_node_utf8(&session_dir)).join("quit-hint.jsonl");
    let entries = [
        serde_json::json!({"type":"session","version":3,"id":"quit-hint","timestamp":"2026-08-04T00:00:00.000Z","cwd":work}),
        serde_json::json!({"type":"message","id":"m-1","parentId":null,"timestamp":"2026-08-04T00:00:01.000Z","message":{"role":"user","content":"quit-hint-test","timestamp":1}}),
    ];
    let jsonl: String = entries.iter().map(|entry| format!("{entry}\n")).collect();
    std::fs::write(&session_file, jsonl).expect("session file");

    let inputs = vec![
        input("quit-hint-test", "\r"),
        input("to show full startup help and loaded resources.", CTRL_C),
        input("to show full startup help and loaded resources.", CTRL_C),
    ];
    let outcome = run_under_pty(
        &fixture,
        &["--session", &session_file.display().to_string()],
        &agent_environment(&fixture),
        &inputs,
    );
    let text = visible(&outcome.stdout);
    assert_eq!(outcome.code, 0, "stdout:\n{text:?}");
    assert!(
        text.contains("To resume this session: pi --session quit-hint"),
        "stdout:\n{text:?}"
    );
}

#[test]
fn resume_flag_selects_a_session_and_starts_interactive_mode() {
    let fixture = fixture();
    let work = fixture.work.display().to_string();
    let session_dir = pi_coding_agent::core::session_manager::get_default_session_dir(
        &JsString::from(&work),
        Some(&JsString::from(&fixture.agent_dir.display().to_string())),
    )
    .expect("session dir");
    std::fs::create_dir_all(&&encode_node_utf8(&session_dir)).expect("session dir");
    let entries = [
        serde_json::json!({"type":"session","version":3,"id":"resume-pick","timestamp":"2026-08-04T00:00:00.000Z","cwd":work}),
        serde_json::json!({"type":"message","id":"m-1","parentId":null,"timestamp":"2026-08-04T00:00:01.000Z","message":{"role":"user","content":"resume-pick-test","timestamp":1}}),
    ];
    let jsonl: String = entries.iter().map(|entry| format!("{entry}\n")).collect();
    std::fs::write(
        Path::new(&encode_node_utf8(&session_dir)).join("resume-pick.jsonl"),
        jsonl,
    )
    .expect("session file");

    let inputs = vec![
        input("resume-pick-test", "\r"),
        input("to show full startup help and loaded resources.", CTRL_C),
        input("to show full startup help and loaded resources.", CTRL_C),
    ];
    let outcome = run_under_pty(&fixture, &["--resume"], &agent_environment(&fixture), &inputs);
    let text = visible(&outcome.stdout);
    assert_eq!(outcome.code, 0, "stdout:\n{text:?}");
    assert!(text.contains("Resume Session"), "stdout:\n{text:?}");
    assert!(
        text.contains("to show full startup help and loaded resources."),
        "stdout:\n{text:?}"
    );
    assert!(!text.contains("No session selected"), "stdout:\n{text:?}");
}

fn share_environment(fixture: &Fixture, gh_body: &str) -> Vec<(String, String)> {
    let bin = fixture._temp.path().join("bin");
    std::fs::create_dir_all(&bin).expect("bin directory");
    let gh = bin.join("gh");
    write_executable_script(&gh, gh_body);
    let mut environment = agent_environment(fixture);
    environment.push((
        "PATH".to_owned(),
        format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default()),
    ));
    environment
}

#[test]
fn share_command_reports_a_missing_gh_login() {
    let fixture = fixture();
    let environment = share_environment(&fixture, "#!/bin/sh\nexit 1\n");
    let inputs = vec![
        input("to show full startup help and loaded resources.", "/share\r"),
        input("GitHub CLI is not logged in.", CTRL_C),
        input("to show full startup help and loaded resources.", CTRL_C),
    ];
    let outcome = run_under_pty(&fixture, &[], &environment, &inputs);
    let text = visible(&outcome.stdout);
    assert_eq!(outcome.code, 0, "stdout:\n{text:?}");
    assert!(
        text.contains("GitHub CLI is not logged in. Run 'gh auth login' first."),
        "stdout:\n{text:?}"
    );
}

fn spawn_pty_command(
    fixture: &Fixture,
    command_line: &str,
    environment: &[(String, String)],
) -> (
    Child,
    std::sync::Arc<std::sync::Mutex<String>>,
    std::thread::JoinHandle<()>,
) {
    let mut command = Command::new("script");
    command
        .arg("-qefc")
        .arg(command_line)
        .env_remove("PI_CODING_AGENT_DIR")
        .arg("/dev/null")
        .current_dir(&fixture.work)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (name, value) in environment {
        command.env(name, value);
    }
    let mut child = command.spawn().expect("script must allocate a pty");
    let stdout = child.stdout.take().expect("pty stdout");
    let stderr = child.stderr.take().expect("script stderr");
    let output = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let buffer = std::sync::Arc::clone(&output);
    let reader = std::thread::spawn(move || {
        std::thread::scope(|scope| {
            // Include script's own errors in marker-failure diagnostics, not just
            // PTY stdout. Drain both concurrently to avoid pipe deadlocks.
            scope.spawn(|| capture_output(stderr, &buffer));
            capture_output(stdout, &buffer);
        });
    });
    (child, output, reader)
}

fn capture_output(mut stream: impl Read, output: &std::sync::Mutex<String>) {
    let mut bytes = [0_u8; 4096];
    loop {
        let read = stream.read(&mut bytes).expect("script output");
        if read == 0 {
            return;
        }
        output
            .lock()
            .expect("pty buffer")
            .push_str(&String::from_utf8_lossy(&bytes[..read]));
    }
}

fn wait_for_pidfile(path: &Path) -> u32 {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if let Ok(content) = std::fs::read_to_string(path)
            && let Ok(pid) = content.trim().parse::<u32>()
        {
            return pid;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("pid file {:?} was not written", path);
}

fn wait_for_marker_count(output: &std::sync::Arc<std::sync::Mutex<String>>, marker: &str, count: usize) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        let occurrences = output.lock().expect("pty buffer").matches(marker).count();
        if occurrences >= count {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!(
        "marker {marker:?} did not render {count} times; output:\n{}",
        output.lock().expect("pty buffer")
    );
}

fn process_group_of(pid: u32) -> u32 {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).expect("process stat");
    let (_, fields) = stat.rsplit_once(')').expect("stat command terminator");
    fields
        .split_whitespace()
        .nth(2)
        .expect("stat pgrp field")
        .parse()
        .expect("numeric pgrp")
}

fn kill_group(group: u32) {
    let Some(pgid) = rustix::process::Pid::from_raw(group as i32) else {
        return;
    };
    rustix::process::kill_process_group(pgid, rustix::process::Signal::KILL).ok();
}

#[test]
fn sigterm_shuts_down_interactive_mode_gracefully() {
    let fixture = fixture();
    let pidfile = fixture._temp.path().join("pi.pid");
    let binary = env!("CARGO_BIN_EXE_pi");
    let command_line = format!("echo $$ > {}; exec {binary}", pidfile.display());
    let (mut child, output, reader) = spawn_pty_command(&fixture, &command_line, &agent_environment(&fixture));

    wait_for_marker(&output, "to show full startup help and loaded resources.");
    let pid = wait_for_pidfile(&pidfile);
    let Some(target) = rustix::process::Pid::from_raw(pid as i32) else {
        panic!("invalid pid {pid}");
    };
    rustix::process::kill_process(target, rustix::process::Signal::TERM).expect("SIGTERM");

    let code = wait_with_timeout(&mut child, Duration::from_secs(20));
    reader.join().expect("pty reader");
    let text = visible(&output.lock().expect("pty buffer"));
    assert_eq!(code, 0, "stdout:\n{text:?}");
    assert!(
        text.contains("to show full startup help and loaded resources."),
        "stdout:\n{text:?}"
    );
}

#[test]
fn sighup_restarts_interactive_mode_in_place() {
    let fixture = fixture();
    let pidfile = fixture._temp.path().join("pi.pid");
    let binary = env!("CARGO_BIN_EXE_pi");
    let command_line = format!(
        "{binary} < /dev/tty & echo $! > {}; wait $!; sleep 30",
        pidfile.display()
    );
    let (mut child, output, reader) = spawn_pty_command(&fixture, &command_line, &agent_environment(&fixture));

    wait_for_marker(&output, "to show full startup help and loaded resources.");
    let pid = wait_for_pidfile(&pidfile);
    let group = process_group_of(pid);
    let Some(target) = rustix::process::Pid::from_raw(pid as i32) else {
        panic!("invalid pid {pid}");
    };
    rustix::process::kill_process(target, rustix::process::Signal::HUP).expect("SIGHUP");

    wait_for_marker_count(&output, "to show full startup help and loaded resources.", 2);
    let text = visible(&output.lock().expect("pty buffer"));
    assert!(text.contains("Pi can explain its own features"), "stdout:\n{text:?}");

    kill_group(group);
    child.kill().ok();
    child.wait().ok();
    reader.join().expect("pty reader");
}
