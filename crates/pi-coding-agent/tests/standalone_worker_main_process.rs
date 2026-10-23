use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

struct OwnedChild(Option<std::process::Child>);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if let Some(child) = &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
#[test]
fn real_main_cli_forwards_standalone_flag_without_orchestration_or_persistence() {
    let temp = tempfile::TempDir::new().unwrap();
    let project = temp.path().join("project");
    let agent = temp.path().join("agent");
    let home = temp.path().join("home");
    let state = temp.path().join("state");
    let sessions = temp.path().join("sessions");
    for directory in [&project, &agent, &home, &state, &sessions] {
        std::fs::create_dir_all(directory).unwrap();
    }
    let snapshot = temp.path().join("standalone.json");
    let child = Command::new(env!("CARGO_BIN_EXE_pi-mode-harness"))
        .args([
            "--no-supervisor",
            "--no-session",
            "--no-extensions",
            "--provider",
            "faux",
            "--model",
            "faux-1",
            "-p",
            "hello",
        ])
        .current_dir(&project)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", temp.path().join("config"))
        .env(pi_coding_agent::config::env_agent_dir(), &agent)
        .env(pi_coding_agent::config::env_state_dir(), &state)
        .env("PI_HARNESS_MODE", "main")
        .env("PI_HARNESS_CWD", &project)
        .env("PI_HARNESS_AGENT_DIR", &agent)
        .env("PI_HARNESS_SESSION_DIR", &sessions)
        .env(
            "PI_HARNESS_RESPONSES",
            r#"[{"text":"standalone complete","endTurn":"done","toolId":"done"}]"#,
        )
        .env("PI_HARNESS_STANDALONE_STATE", &snapshot)
        .spawn()
        .unwrap();
    let mut child = OwnedChild(Some(child));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        if child.0.as_mut().unwrap().try_wait().unwrap().is_some() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "standalone main must finish without waiting for Supervisor"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let output = child.0.take().unwrap().wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("standalone complete"));
    let actual: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(snapshot).unwrap()).unwrap();
    assert_eq!(
        actual,
        serde_json::json!({"noSupervisor":true,"persisted":false,"hasStore":false,"hasDetach":false})
    );
    assert!(std::fs::read_dir(sessions).unwrap().next().is_none());
    let database = state.join("control.sqlite");
    if database.exists() {
        let connection = rusqlite::Connection::open(&database).unwrap();
        let listeners: i64 = connection
            .query_row("SELECT COUNT(*) FROM runtime_mailbox_listeners", [], |row| row.get(0))
            .unwrap();
        assert_eq!(listeners, 0, "standalone main must not advertise a runtime listener");
    }
}
