//! Host relay process and cross-implementation behavior; always run by the Cargo sandbox runner.
use serde_json::Value;
use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn serve_dispatches_before_provider_initialization_and_exits_on_eof() {
    let temp = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_pi"))
        .args(["relay", "serve"])
        .env("PI_CODING_AGENT_DIR", temp.path())
        .env("PI_CODING_AGENT_STATE_DIR", temp.path())
        .env("PI_OFFLINE", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"{\"type\":\"hello\",\"host\":\"fixture-peer\"}\n")
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let frames: Vec<Value> = String::from_utf8(result.stdout)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0]["type"], "hello");
    assert_eq!(
        frames[0]["host"],
        pi_js_compat::node_fs::decode_node_path(gethostname::gethostname().as_os_str())
            .to_utf8()
            .unwrap()
    );
}

#[test]
fn standalone_flag_and_resident_console_rejection_precede_startup() {
    use pi_coding_agent::cli::args::parse_args;
    let valid = parse_args(&["--no-supervisor".into(), "--no-session".into()]);
    assert_eq!(valid.no_supervisor, Some(true));
    assert!(valid.diagnostics.is_empty());
    assert!(!valid.unknown_flags.contains_key("no-supervisor"));
    let invalid = parse_args(&["--no-supervisor".into(), "--name".into(), "x".into()]);
    assert_eq!(
        invalid
            .diagnostics
            .iter()
            .map(|d| d.message.as_str())
            .collect::<Vec<_>>(),
        vec![
            "--no-supervisor requires --no-session",
            "--no-supervisor cannot be combined with --name"
        ]
    );
    let temp = tempfile::tempdir().unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_pi"))
        .args(["--supervisor", "--no-supervisor", "--no-session"])
        .env("PI_CODING_AGENT_DIR", temp.path())
        .env("PI_CODING_AGENT_STATE_DIR", temp.path())
        .env("PI_OFFLINE", "1")
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
    assert!(
        String::from_utf8(result.stderr)
            .unwrap()
            .contains("resident consoles are unavailable in standalone worker mode")
    );
}
