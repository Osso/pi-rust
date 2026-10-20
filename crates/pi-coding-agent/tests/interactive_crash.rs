//! Process-level crash handling in interactive mode: `isDeadTerminalError`,
//! `emergencyTerminalExit`, `uncaughtCrash`, and the signal-handler wiring.
//!
//! The handlers end the process, so the observable behavior is asserted at the
//! process boundary: exit codes, stderr text, and terminal restore sequences.

use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use pi_coding_agent::modes::interactive::{is_dead_terminal_error, terminal_error_message};

#[path = "support/executable_script.rs"]
mod executable_script;
#[path = "support/pty.rs"]
mod pty;

use executable_script::write_executable_script;

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

fn agent_environment(fixture: &Fixture) -> Vec<(String, String)> {
    vec![
        ("HOME".to_owned(), fixture.home.display().to_string()),
        ("PI_PACKAGE_DIR".to_owned(), fixture.package_dir.display().to_string()),
        (
            "PI_CODING_AGENT_DIR".to_owned(),
            fixture.agent_dir.display().to_string(),
        ),
        (
            "PI_CODING_AGENT_STATE_DIR".to_owned(),
            fixture.state_dir.display().to_string(),
        ),
        ("NO_COLOR".to_owned(), "1".to_owned()),
        ("TERM".to_owned(), "xterm-256color".to_owned()),
    ]
}

fn spawn_interactive(fixture: &Fixture, environment: &[(String, String)]) -> pty::PtyChild {
    pty::spawn(Path::new(env!("CARGO_BIN_EXE_pi")), &[], environment, &fixture.work)
}

#[test]
fn dead_terminal_errors_are_eio_epipe_and_enotconn() {
    let io_error = rustix::io::Errno::IO.raw_os_error();
    let pipe_error = rustix::io::Errno::PIPE.raw_os_error();
    let not_connected = rustix::io::Errno::NOTCONN.raw_os_error();
    let no_entry = rustix::io::Errno::NOENT.raw_os_error();

    assert!(is_dead_terminal_error(&io::Error::from_raw_os_error(io_error)));
    assert!(is_dead_terminal_error(&io::Error::from_raw_os_error(pipe_error)));
    assert!(is_dead_terminal_error(&io::Error::from_raw_os_error(not_connected)));

    assert!(!is_dead_terminal_error(&io::Error::from_raw_os_error(no_entry)));
    assert!(!is_dead_terminal_error(&io::Error::other("write failed")));
    assert!(!is_dead_terminal_error(&io::Error::new(
        io::ErrorKind::BrokenPipe,
        "kind only, no errno code"
    )));
}

/// `uncaughtCrash`: a throw from extension-facing code (the external editor
/// read) reaches the process handler, which restores the terminal, reports the
/// error, and exits 1.
#[test]
fn uncaught_exception_restores_the_terminal_and_exits_one() {
    let fixture = fixture();
    let bin = fixture._temp.path().join("bin");
    std::fs::create_dir_all(&bin).expect("bin directory");
    let editor = bin.join("editor");
    write_executable_script(&editor, "#!/bin/sh\nrm -f \"$1\"\nexit 0\n");
    let settings = serde_json::json!({ "externalEditor": editor.display().to_string() });
    std::fs::write(
        fixture.agent_dir.join("settings.json"),
        serde_json::to_string(&settings).expect("settings json"),
    )
    .expect("settings.json");

    let mut child = spawn_interactive(&fixture, &agent_environment(&fixture));
    child.wait_for("Ctrl+C twice to exit", Duration::from_secs(20));
    child.write_input("\x07");
    let code = child.wait(Duration::from_secs(20));
    let text = child.output();

    assert_eq!(code, 1, "uncaught exceptions exit 1; output:\n{text}");
    let banner = text
        .find("pi exiting due to uncaughtException:")
        .unwrap_or_else(|| panic!("missing crash banner; output:\n{text}"));
    assert!(
        text.contains("Error: ENOENT: no such file or directory, open '"),
        "the thrown error is reported; output:\n{text}"
    );
    let restore = text
        .find("\x1b[?2004l")
        .unwrap_or_else(|| panic!("bracketed paste was not disabled; output:\n{text}"));
    assert!(restore < banner, "the terminal is restored before the report");
    assert!(text.contains("\x1b[?25h"), "the cursor is shown again; output:\n{text}");
    assert!(
        text.contains("\x1b[<u"),
        "the kitty protocol is popped; output:\n{text}"
    );
}

/// `emergencyTerminalExit`: once the terminal is gone (the pty master closed,
/// so the stdin read fails with `EIO`), restore writes would fail again, so
/// the process exits 129 without the crash report.
#[test]
fn dead_terminal_exits_129_without_restoring() {
    let fixture = fixture();
    let mut child = spawn_interactive(&fixture, &agent_environment(&fixture));
    child.wait_for("Ctrl+C twice to exit", Duration::from_secs(20));
    child.close_master();

    let code = child.wait(Duration::from_secs(20));
    let text = child.output();
    assert_eq!(code, 129, "dead terminals exit 129; output:\n{text}");
    assert!(
        !text.contains("pi exiting due to uncaughtException:"),
        "the terminal is not restored; output:\n{text}"
    );
}

/// `void restartProcess({ fromSignal: true })`: a rejected restart promise
/// becomes an uncaught exception, which restores the terminal and exits 1.
#[test]
fn sighup_restart_failure_restores_the_terminal_and_exits_one() {
    let fixture = fixture();
    let binary = fixture._temp.path().join("pi-restart");
    std::fs::copy(env!("CARGO_BIN_EXE_pi"), &binary).expect("copy the pi binary");
    let mut child = pty::spawn(&binary, &[], &agent_environment(&fixture), &fixture.work);
    child.wait_for("Ctrl+C twice to exit", Duration::from_secs(20));
    std::fs::remove_file(&binary).expect("remove the copied binary so restart fails");
    let Some(target) = rustix::process::Pid::from_raw(child.id() as i32) else {
        panic!("invalid pid");
    };
    rustix::process::kill_process(target, rustix::process::Signal::HUP).expect("SIGHUP");

    let code = child.wait(Duration::from_secs(20));
    let text = child.output();
    assert_eq!(code, 1, "failed restarts exit 1; output:\n{text}");
    let banner = text
        .find("pi exiting due to uncaughtException:")
        .unwrap_or_else(|| panic!("missing crash banner; output:\n{text}"));
    assert!(
        text.contains("Error: "),
        "the restart error is reported; output:\n{text}"
    );
    let restore = text
        .find("\x1b[?2004l")
        .unwrap_or_else(|| panic!("bracketed paste was not disabled; output:\n{text}"));
    assert!(restore < banner, "the terminal is restored before the report");
}

#[test]
fn rethrown_terminal_errors_use_the_node_stream_message() {
    // Node: `Error: write EPIPE` / `Error: read EBADF` for stream `error` events.
    let write = pi_tui::TerminalIoError {
        syscall: "write",
        error: io::Error::from_raw_os_error(rustix::io::Errno::PIPE.raw_os_error()),
    };
    let read = pi_tui::TerminalIoError {
        syscall: "read",
        error: io::Error::from_raw_os_error(rustix::io::Errno::BADF.raw_os_error()),
    };
    assert_eq!(terminal_error_message(&write), "write EPIPE");
    assert_eq!(terminal_error_message(&read), "read EBADF");
}
