//! Interactive startup through a real pty (`main.ts` interactive dispatch,
//! `--resume` picker, first-time setup).
//!
//! `util-linux`'s `script` allocates the pty, so the binary observes
//! `stdin`/`stdout` as TTYs and resolves `interactive` mode. Input is written
//! to the pty on a schedule; `script -e` forwards the child's exit status.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

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
        ("PI_PACKAGE_DIR".to_owned(), fixture.package_dir.display().to_string()),
        ("NO_COLOR".to_owned(), "1".to_owned()),
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

/// Runs the `pi` binary under `script` and writes `inputs` on a schedule.
fn run_under_pty(
    fixture: &Fixture,
    args: &[&str],
    environment: &[(String, String)],
    inputs: &[(Duration, &str)],
) -> PtyOutcome {
    let binary = env!("CARGO_BIN_EXE_pi");
    let mut command_line = binary.to_owned();
    for arg in args {
        command_line.push(' ');
        command_line.push_str(arg);
    }
    let mut command = Command::new("script");
    command
        .arg("-qefc")
        .arg(&command_line)
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
    let reader = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let mut stdout = stdout;
        stdout.read_to_end(&mut buffer).ok();
        buffer
    });

    let mut stdin = child.stdin.take().expect("pty stdin");
    for (delay, input) in inputs {
        std::thread::sleep(*delay);
        stdin.write_all(input.as_bytes()).expect("pty input");
        stdin.flush().ok();
    }
    drop(stdin);

    let code = wait_with_timeout(&mut child, Duration::from_secs(20));
    let output = reader.join().expect("pty reader");
    PtyOutcome {
        code,
        stdout: String::from_utf8_lossy(&output).into_owned(),
    }
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

fn visible(output: &str) -> String {
    pi_tui::utils::strip_terminal_sequences(output)
}

fn double_ctrl_c() -> Vec<(Duration, &'static str)> {
    vec![
        (Duration::from_millis(1500), CTRL_C),
        (Duration::from_millis(150), CTRL_C),
    ]
}

#[test]
fn interactive_mode_starts_the_tui_and_exits_on_double_ctrl_c() {
    let fixture = fixture();
    let outcome = run_under_pty(&fixture, &[], &agent_environment(&fixture), &double_ctrl_c());
    let text = visible(&outcome.stdout);
    assert_eq!(outcome.code, 0, "stdout:\n{text}");
    assert!(text.contains("Ctrl+C twice to exit"), "stdout:\n{text}");
    assert!(text.contains("Pi can explain its own features"), "stdout:\n{text}");
    assert!(!text.contains("Interactive mode is not ported"));
}

#[test]
fn resume_flag_opens_the_session_picker_and_cancels_with_escape() {
    let fixture = fixture();
    let outcome = run_under_pty(
        &fixture,
        &["--resume"],
        &agent_environment(&fixture),
        &[(Duration::from_millis(1500), ESCAPE)],
    );
    let text = visible(&outcome.stdout);
    assert_eq!(outcome.code, 0, "stdout:\n{text}");
    assert!(text.contains("Resume Session"), "stdout:\n{text}");
    assert!(text.contains("No sessions in current folder"), "stdout:\n{text}");
    assert!(text.contains("No session selected"), "stdout:\n{text}");
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
        (Duration::from_millis(1500), "j"),
        (Duration::from_millis(150), "\r"),
        (Duration::from_millis(250), "\r"),
        (Duration::from_millis(1200), CTRL_C),
        (Duration::from_millis(150), CTRL_C),
    ];
    let outcome = run_under_pty(&fixture, &[], &environment, &inputs);
    let text = visible(&outcome.stdout);
    assert_eq!(outcome.code, 0, "stdout:\n{text}");
    assert!(text.contains("Welcome to pi"), "stdout:\n{text}");
    assert!(text.contains("Pick a theme."), "stdout:\n{text}");
    assert!(text.contains("Ctrl+C twice to exit"), "stdout:\n{text}");

    let settings_path = fixture.home.join(".config/pi/agent/settings.json");
    let settings = std::fs::read_to_string(&settings_path).expect("settings.json written");
    assert!(settings.contains(r#""theme": "light""#), "{settings}");
    assert!(settings.contains(r#""enableAnalytics": true"#), "{settings}");
}
