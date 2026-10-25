//! Process-global signal handling stays in owned child processes.
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use pi_orchestrator::{config::Config, ipc::client::send_ipc_request};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[tokio::test]
#[ignore = "child helper invoked by cli_serve_signals_match_frozen"]
async fn cli_serve_child() {
    assert_eq!(std::env::var("PI_ORCHESTRATOR_TEST_CHILD").as_deref(), Ok("1"));
    let config = Config::from_environment();
    let mut stdout = tokio::io::stdout();
    let mut stderr = tokio::io::stderr();
    let code = pi_orchestrator::cli::run_cli(
        &["serve".into()],
        &JsString::from(std::env::current_dir().unwrap().to_str().unwrap()),
        config,
        tokio::io::empty(),
        &mut stdout,
        &mut stderr,
    )
    .await;
    stdout.flush().await.unwrap();
    stderr.flush().await.unwrap();
    std::process::exit(code);
}

#[tokio::test]
async fn cli_serve_startup_failure_returns_frozen_error_message_and_exit_one() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lock: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("docs/porting/source-lock.json")).unwrap()).unwrap();
    let source = std::path::Path::new(lock["worktree"].as_str().unwrap());
    let directory = tempfile::tempdir().unwrap();
    let blocked = directory.path().join("blocked");
    std::fs::write(&blocked, "keep").unwrap();
    let script = directory.path().join("serve-failure.mjs");
    std::fs::write(&script, format!(
        "import {{serve}} from '{}'; try {{await serve(); throw new Error('expected failure');}} catch(error) {{process.stdout.write(JSON.stringify(error.message));}}",
        source.join("packages/orchestrator/src/serve.ts").display(),
    )).unwrap();
    let expected = tokio::process::Command::new(source.join("node_modules/.bin/tsx"))
        .arg("--tsconfig")
        .arg(source.join("tsconfig.json"))
        .arg(&script)
        .env("PI_ORCHESTRATOR_DIR", &blocked)
        .env_remove("PI_RADIUS_API_KEY")
        .output()
        .await
        .unwrap();
    assert!(expected.status.success());
    let expected_message: String = serde_json::from_slice(&expected.stdout).unwrap();
    assert!(expected_message.starts_with("EEXIST:"));
    let actual = tokio::process::Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "cli_serve_child", "--nocapture"])
        .env("PI_ORCHESTRATOR_TEST_CHILD", "1")
        .env("PI_ORCHESTRATOR_DIR", &blocked)
        .env_remove("PI_RADIUS_API_KEY")
        .output()
        .await
        .unwrap();
    assert_eq!(actual.status.code(), Some(1));
    assert_eq!(actual.stderr, format!("{expected_message}\n").into_bytes());
    assert_eq!(std::fs::read_to_string(&blocked).unwrap(), "keep");
}

#[tokio::test]
async fn cli_serve_signals_match_frozen() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lock: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("docs/porting/source-lock.json")).unwrap()).unwrap();
    let source = std::path::Path::new(lock["worktree"].as_str().unwrap());
    for signal in [rustix::process::Signal::TERM, rustix::process::Signal::INT] {
        let directory = tempfile::tempdir().unwrap();
        let config = Config {
            dir: directory.path().join("runtime").to_str().unwrap().into(),
        };
        let mut expected_logs = None;
        for frozen in [true, false] {
            let mut command = if frozen {
                let mut command = tokio::process::Command::new(source.join("node_modules/.bin/tsx"));
                command
                    .arg("--tsconfig")
                    .arg(source.join("tsconfig.json"))
                    .arg(source.join("packages/orchestrator/src/cli.ts"))
                    .arg("serve");
                command
            } else {
                let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
                command
                    .args(["--ignored", "--exact", "cli_serve_child", "--nocapture"])
                    .env("PI_ORCHESTRATOR_TEST_CHILD", "1");
                command
            };
            let mut child = command
                .env("PI_ORCHESTRATOR_DIR", encode_node_utf8(&config.dir))
                .env_remove("PI_RADIUS_API_KEY")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .spawn()
                .unwrap();
            let mut stdout = BufReader::new(child.stdout.take().unwrap());
            let mut logs = Vec::new();
            loop {
                let mut line = String::new();
                let count = tokio::time::timeout(std::time::Duration::from_secs(30), stdout.read_line(&mut line))
                    .await
                    .unwrap()
                    .unwrap();
                assert_ne!(count, 0, "serve exited before listening: frozen={frozen}");
                if line.starts_with("radius integration ") || line.starts_with("orchestrator listening ") {
                    logs.push(line.clone());
                }
                if line.starts_with("orchestrator listening ") {
                    break;
                }
            }
            let response = send_ipc_request(&config.socket_path(), &pi_js_compat::js_json!({"type":"list"}))
                .await
                .unwrap();
            assert_eq!(
                response,
                pi_js_compat::js_json!({"type":"list_result","ok":true,"instances":[]})
            );
            let pid = rustix::process::Pid::from_raw(child.id().unwrap() as i32).unwrap();
            rustix::process::kill_process(pid, signal).unwrap();
            let status = tokio::time::timeout(std::time::Duration::from_secs(10), child.wait())
                .await
                .unwrap()
                .unwrap();
            assert!(status.success(), "frozen={frozen}, signal={signal:?}, status={status}");
            assert!(!std::path::Path::new(&encode_node_utf8(&config.socket_path())).exists());
            if let Some(expected) = &expected_logs {
                assert_eq!(&logs, expected);
            } else {
                expected_logs = Some(logs);
            }
        }
    }
}
