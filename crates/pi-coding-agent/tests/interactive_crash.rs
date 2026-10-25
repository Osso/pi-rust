//! Process-level crash handling in interactive mode: `isDeadTerminalError`,
//! `emergencyTerminalExit`, `uncaughtCrash`, and the signal-handler wiring.
//!
//! The handlers end the process, so the observable behavior is asserted at the
//! process boundary: exit codes, stderr text, and terminal restore sequences.

use pi_js_compat::js_json::JsString;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use pi_coding_agent::modes::interactive::{is_dead_terminal_error, terminal_error_message};

use crate::shared_executable_script as executable_script;
use crate::shared_oracle as oracle;
#[path = "support/pty.rs"]
mod pty;
#[path = "support/resource_display_home.rs"]
mod resource_home;

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
        ("PI_OFFLINE".to_owned(), "1".to_owned()),
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
    child.wait_for(
        "to show full startup help and loaded resources.",
        Duration::from_secs(20),
    );
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
    child.wait_for(
        "to show full startup help and loaded resources.",
        Duration::from_secs(20),
    );
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
    let mut source = oracle::TsOracle::spawn("interactive-sighup-failure-oracle.mts", "packages/coding-agent/src");
    let expected = source.request(&serde_json::json!({}));
    assert_eq!(expected["ok"], true, "source: {expected}");
    let fixture = fixture();
    let binary = fixture._temp.path().join("pi-restart");
    std::fs::copy(env!("CARGO_BIN_EXE_pi"), &binary).expect("copy the pi binary");
    let mut child = pty::spawn(&binary, &[], &agent_environment(&fixture), &fixture.work);
    child.wait_for(
        "to show full startup help and loaded resources.",
        Duration::from_secs(20),
    );
    std::fs::remove_file(&binary).expect("remove the copied binary so restart fails");
    let Some(target) = rustix::process::Pid::from_raw(child.id() as i32) else {
        panic!("invalid pid");
    };
    rustix::process::kill_process(target, rustix::process::Signal::HUP).expect("SIGHUP");

    let code = child.wait(Duration::from_secs(20));
    let text = child.output();
    let report = text
        .find("Error: ")
        .unwrap_or_else(|| panic!("missing restart error; output:\n{text}"));
    let restore = text
        .find("\x1b[?2004l")
        .unwrap_or_else(|| panic!("bracketed paste was not disabled; output:\n{text}"));
    assert_eq!(
        serde_json::json!({"code":code,"banner":text.contains("pi exiting due to uncaughtException:"),"reported":true,"restoredBeforeReport":restore<report}),
        expected["output"],
        "output:\n{text}"
    );
}

fn assert_worker_crash(kind: &str) {
    let fixture = fixture();
    let trigger = fixture._temp.path().join("panic-now");
    let mut environment = agent_environment(&fixture);
    environment.push(("PI_RUST_CRASH_TRIGGER".to_owned(), trigger.display().to_string()));
    environment.push(("PI_RUST_CRASH_WORKER".to_owned(), kind.to_owned()));
    let mut child = pty::spawn(
        &std::env::current_exe().expect("test executable"),
        &["--ignored", "--exact", "worker_panic_child", "--nocapture"],
        &environment,
        &fixture.work,
    );
    let original = child.initial_termios();
    child.wait_for(
        "to show full startup help and loaded resources.",
        Duration::from_secs(20),
    );
    // A DA reply without Kitty support enables modifyOtherKeys.
    child.write_input("\x1b[?1;2c");
    child.wait_for("\x1b[>4;2m", Duration::from_secs(5));
    assert_ne!(child.termios(), original, "interactive mode must enter raw mode");
    std::fs::write(&trigger, "panic").expect("trigger panic");
    let code = child.wait(Duration::from_secs(20));
    let text = child.output();
    if kind == "dead" {
        assert_eq!(code, 129, "dead worker terminal; output:\n{text}");
        assert!(!text.contains("pi exiting due to uncaughtException:"));
        assert!(!text.contains("\x1b[?2004l"));
        assert_ne!(child.termios(), original, "dead terminal must not attempt restoration");
        return;
    }
    assert_eq!(code, 1, "worker panic must exit 1; output:\n{text}");
    let banner = text
        .find("pi exiting due to uncaughtException:")
        .unwrap_or_else(|| panic!("missing crash banner; output:\n{text}"));
    for sequence in ["\x1b[?25h", "\x1b[?2004l", "\x1b[<u", "\x1b[>4;0m"] {
        assert!(
            text[..banner].contains(sequence),
            "missing restore {sequence:?}; output:\n{text}"
        );
    }
    assert!(text[banner..].contains("Error: worker boom"), "worker payload: {text}");
    assert_eq!(child.termios(), original, "restore original cooked mode");
}

#[test]
fn extension_thread_panic_restores_the_terminal() {
    assert_worker_crash("thread");
}

#[test]
fn dead_terminal_error_on_a_worker_exits_129_without_restoring() {
    assert_worker_crash("dead");
}

#[test]
fn blocking_pool_panic_restores_the_terminal() {
    assert_worker_crash("blocking");
}

#[test]
fn tokio_worker_panic_restores_the_frozen_terminal() {
    assert_worker_crash("task");
}

#[test]
#[ignore = "PTY child entry point"]
fn worker_panic_child() {
    let Some(trigger) = std::env::var_os("PI_RUST_CRASH_TRIGGER") else {
        return;
    };
    pi_coding_agent::utils::uncaught::install_uncaught_handler();
    let dead = std::env::var("PI_RUST_CRASH_WORKER").expect("worker kind") == "dead";
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("runtime");
    runtime.block_on(tokio::task::LocalSet::new().run_until(async move {
        let crash = move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(30);
            while !Path::new(&trigger).exists() {
                assert!(std::time::Instant::now() < deadline, "panic trigger timed out");
                std::thread::sleep(Duration::from_millis(10));
            }
            if dead {
                std::panic::panic_any(io::Error::from_raw_os_error(rustix::io::Errno::IO.raw_os_error()));
            }
            panic!("worker boom");
        };
        match std::env::var("PI_RUST_CRASH_WORKER").expect("worker kind").as_str() {
            "thread" | "dead" => {
                std::thread::spawn(crash);
            }
            "blocking" => {
                tokio::task::spawn_blocking(crash);
            }
            "task" => {
                tokio::spawn(async move { crash() });
            }
            _ => panic!("unknown worker kind"),
        }
        let code = pi_coding_agent::main_runner::main(&[], Default::default())
            .await
            .expect("main");
        std::process::exit(code);
    }));
}

fn capture_stop_output(fixture: &Fixture, worker: bool) -> String {
    let mut environment = agent_environment(fixture);
    environment.push(("PI_RUST_STOP_MODE".to_owned(), worker.to_string()));
    let mut child = pty::spawn(
        &std::env::current_exe().expect("test executable"),
        &["--ignored", "--exact", "terminal_stop_child", "--nocapture"],
        &environment,
        &fixture.work,
    );
    child.wait_for("STOP_READY", Duration::from_secs(10));
    // Fetch the stored baseline only after the child has entered raw mode.
    let original = child.initial_termios();
    assert_ne!(child.termios(), original, "interactive mode must enter raw mode");
    child.write_input("\x1b[?1;2c");
    child.wait_for("\x1b[>4;2m", Duration::from_secs(5));
    child.write_input("x");
    assert_eq!(child.wait(Duration::from_secs(10)), 0);
    assert_eq!(child.termios(), original);
    child
        .output()
        .split_once("STOP_START\n")
        .expect("stop marker")
        .1
        .to_owned()
}

#[test]
fn worker_restore_matches_normal_ui_stop_bytes() {
    let fixture = fixture();
    let normal = capture_stop_output(&fixture, false);
    let worker = capture_stop_output(&fixture, true);
    assert_eq!(worker, normal, "stop output must be byte-identical");
}

#[test]
#[ignore = "PTY child entry point"]
fn terminal_stop_child() {
    use pi_coding_agent::modes::interactive::interactive_mode::{
        InteractiveTuiHandle, InteractiveTuiOptions, create_interactive_tui,
    };
    use pi_tui::Terminal;
    use pi_tui::tui::component;
    use std::cell::RefCell;
    use std::io::Write;
    use std::rc::Rc;

    let Ok(mode) = std::env::var("PI_RUST_STOP_MODE") else {
        return;
    };
    let worker = mode.as_str();
    let terminal: Rc<RefCell<dyn Terminal>> = Rc::new(RefCell::new(pi_tui::ProcessTerminal::new()));
    let restore = terminal.borrow().crash_restore().expect("real terminal restore");
    let ui = InteractiveTuiHandle::new(create_interactive_tui(InteractiveTuiOptions {
        show_hardware_cursor: false,
        log_directory: JsString::from(std::env::var("PI_CODING_AGENT_DIR").expect("agent dir")),
        terminal: Some(terminal.clone()),
    }));
    ui.add_child(component(pi_tui::Text::new("restore fixture", 0, 0)));
    ui.set_terminal_color_scheme_notifications(true);
    ui.start().expect("start");
    ui.render_now().unwrap();
    terminal.borrow_mut().set_progress(true);
    // Drain input directly so the final 'x' is a deterministic stop request.
    std::io::stdout().write_all(b"STOP_READY\n").expect("ready");
    loop {
        let events = terminal.borrow_mut().poll_events();
        if events
            .iter()
            .any(|event| matches!(event, pi_tui::terminal::TerminalEvent::Input(text) if text == "x"))
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    std::io::stdout().write_all(b"STOP_START\n").expect("stop marker");
    if worker == "true" {
        std::thread::spawn(move || {
            restore.restore_then(|result| {
                result.expect("restore");
                std::process::exit(0);
            })
        })
        .join()
        .expect("restore thread");
    } else {
        ui.stop().unwrap();
        std::process::exit(0);
    }
}

#[test]
fn rethrown_terminal_errors_use_the_node_stream_message() {
    // Node: `Error: write EPIPE` / `Error: read EBADF` for stream `error` events.
    let write = pi_tui::terminal::TerminalIoError {
        syscall: "write",
        error: io::Error::from_raw_os_error(rustix::io::Errno::PIPE.raw_os_error()),
    };
    let read = pi_tui::terminal::TerminalIoError {
        syscall: "read",
        error: io::Error::from_raw_os_error(rustix::io::Errno::BADF.raw_os_error()),
    };
    assert_eq!(terminal_error_message(&write), "write EPIPE");
    assert_eq!(terminal_error_message(&read), "read EBADF");
}

/// Characterization (no RED by design): SessionList's home failure is uncaught above render.
#[test]
fn session_path_render_home_failure_restores_terminal_before_crash_report() {
    let fixture = fixture();
    let library = fixture._temp.path().join("passwd-failure.so");
    let marker = fixture._temp.path().join("fail-passwd");
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/passwd_failure.c");
    let output = std::process::Command::new("cc")
        .args(["-shared", "-fPIC", "-o"])
        .arg(&library)
        .arg(source)
        .arg("-ldl")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let cwd = JsString::from(fixture.work.to_str().unwrap());
    let agent = JsString::from(fixture.agent_dir.to_str().unwrap());
    let session_dir = pi_coding_agent::core::session_manager::get_default_session_dir(&cwd, Some(&agent)).unwrap();
    let session_dir = PathBuf::from(session_dir.to_utf8().unwrap());
    std::fs::create_dir_all(&session_dir).unwrap();
    let header = serde_json::json!({"type":"session","version":3,"id":"q13-crash-fixture","timestamp":"2026-10-23T00:00:00.000Z","cwd":fixture.work});
    let message = serde_json::json!({"type":"message","id":"q13-user","parentId":null,"timestamp":"2026-10-23T00:00:00.000Z","message":{"role":"user","content":[{"type":"text","text":"Q13 resume crash fixture"}],"timestamp":1792713600000.0}});
    std::fs::write(
        session_dir.join("2026-10-23T00-00-00-000Z_q13-crash-fixture.jsonl"),
        format!("{header}\n{message}\n"),
    )
    .unwrap();
    let metadata = serde_json::from_value(serde_json::json!({
        "sessionPath": session_dir.join("2026-10-23T00-00-00-000Z_q13-crash-fixture.jsonl"),
        "id": "q13-crash-fixture", "cwd": fixture.work,
        "createdAt": "2026-10-23T00:00:00.000Z", "modifiedAt": "2026-10-23T00:00:00.000Z",
        "messageCount": 1, "firstMessage": "Q13 resume crash fixture"
    }))
    .unwrap();
    pi_coding_agent::core::session_control_db::write_session_metadata(
        JsString::from(fixture.state_dir.join("control.sqlite").to_str().unwrap()),
        &metadata,
    )
    .unwrap();
    let mut environment = agent_environment(&fixture);
    environment.extend([
        ("LD_PRELOAD".to_owned(), library.to_str().unwrap().to_owned()),
        (
            "PI_TEST_PASSWD_FAILURE_FILE".to_owned(),
            marker.to_str().unwrap().to_owned(),
        ),
        (
            "PI_TEST_PASSWD_HOME".to_owned(),
            fixture.home.to_str().unwrap().to_owned(),
        ),
    ]);
    let mut child = pty::spawn(
        Path::new("/usr/bin/env"),
        &["-u", "HOME", env!("CARGO_BIN_EXE_pi")],
        &environment,
        &fixture.work,
    );
    child.wait_for(
        "to show full startup help and loaded resources.",
        Duration::from_secs(20),
    );
    child.write_input("/resume\r");
    child.wait_for("Q13 resume crash fixture", Duration::from_secs(20));
    std::fs::write(marker, "fail").unwrap();
    child.write_input("\x10");
    assert_eq!(child.wait(Duration::from_secs(20)), 1);
    let text = child.output();
    let banner = text
        .find("pi exiting due to uncaughtException:")
        .unwrap_or_else(|| panic!("missing crash report: {text}"));
    let restore = text
        .find("\x1b[?2004l")
        .unwrap_or_else(|| panic!("missing terminal restore: {text}"));
    assert!(restore < banner, "{text}");
    assert!(
        text.contains(
            "SystemError [ERR_SYSTEM_ERROR]: A system error occurred: uv_os_homedir returned EIO (i/o error)"
        ),
        "{text}"
    );
    assert!(text.contains("\x1b[?25h"), "{text}");
    assert!(
        !text.contains("thread '"),
        "native crash must not print a Rust panic: {text}"
    );
}

// Shared support is compiled once per integration target.
#[path = "support/executable_script.rs"]
pub mod shared_executable_script;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
