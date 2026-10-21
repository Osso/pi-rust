use pi_orchestrator::cli::{CliAction, parse_args};
use serde_json::json;
fn args(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| s.to_string()).collect()
}
#[test]
fn cli_actions_preserve_first_flags_usage_and_json() {
    assert_eq!(
        parse_args(&args(&["--version"]), "/cwd").unwrap(),
        CliAction::Output {
            stdout: "0.80.3\n".into(),
            stderr: String::new(),
            exit_code: 0
        }
    );
    assert_eq!(
        parse_args(&args(&["spawn", "--cwd", "/a", "--cwd", "/b", "--label", "x"]), "/cwd").unwrap(),
        CliAction::Request(json!({"type":"spawn","cwd":"/a","label":"x"}))
    );
    assert_eq!(
        parse_args(&args(&["spawn", "--cwd"]), "/cwd").unwrap(),
        CliAction::Request(json!({"type":"spawn","cwd":"/cwd"}))
    );
    assert_eq!(
        parse_args(&args(&["rpc", "i", "{\"type\":\"get_state\"}"]), "/cwd").unwrap(),
        CliAction::Request(json!({"type":"rpc","instanceId":"i","command":{"type":"get_state"}}))
    );
    assert_eq!(
        parse_args(&args(&["status"]), "/cwd").unwrap(),
        CliAction::Output {
            stdout: String::new(),
            stderr: "Usage: orchestrator status <instance-id>\n".into(),
            exit_code: 1
        }
    );
}

#[tokio::test]
async fn cli_text_and_exit_codes_match_frozen_entry() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lock: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("docs/porting/source-lock.json")).unwrap()).unwrap();
    let source = std::path::Path::new(lock["worktree"].as_str().unwrap());
    for case in [
        vec![],
        vec!["--help"],
        vec!["-h"],
        vec!["--version"],
        vec!["-v"],
        vec!["status"],
        vec!["stop"],
        vec!["rpc"],
        vec!["rpc", "i"],
        vec!["rpc-stream"],
        vec!["not-real"],
    ] {
        let output = tokio::process::Command::new(source.join("node_modules/.bin/tsx"))
            .arg("--tsconfig")
            .arg(source.join("tsconfig.json"))
            .arg(source.join("packages/orchestrator/src/cli.ts"))
            .args(&case)
            .output()
            .await
            .unwrap();
        let CliAction::Output {
            stdout,
            stderr,
            exit_code,
        } = parse_args(&args(&case), "/cwd").unwrap()
        else {
            panic!("expected output")
        };
        assert_eq!(stdout.as_bytes(), output.stdout, "{case:?}");
        assert_eq!(stderr.as_bytes(), output.stderr, "{case:?}");
        assert_eq!(exit_code, output.status.code().unwrap(), "{case:?}");
    }
}
#[tokio::test]
async fn cli_one_shot_request_outputs_match_frozen() {
    use pi_orchestrator::{
        cli::run_cli,
        config::Config,
        ipc::server::{IpcRequestHandler, OnMessage, RpcStream, start_ipc_server},
    };
    use serde_json::Value;
    use std::sync::Arc;
    struct Echo;
    impl IpcRequestHandler for Echo {
        fn handle(&self, request: Value) -> pi_ai::request::BoxFuture<'_, Result<Value, String>> {
            Box::pin(async move { Ok(request) })
        }
        fn open_rpc_stream(&self, _id: &str, _callback: OnMessage) -> Option<Arc<dyn RpcStream>> {
            None
        }
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lock: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("docs/porting/source-lock.json")).unwrap()).unwrap();
    let source = std::path::Path::new(lock["worktree"].as_str().unwrap());
    let dir = tempfile::tempdir().unwrap();
    let config = Config { dir: dir.path().into() };
    let server = start_ipc_server(&config.socket_path(), Arc::new(Echo)).await.unwrap();
    for case in [
        vec!["list"],
        vec!["spawn"],
        vec!["spawn", "--cwd", "/中", "--label", "work"],
        vec!["status", "i"],
        vec!["stop", "i"],
        vec!["rpc", "i", r#"{"type":"get_state","id":"r"}"#],
    ] {
        let output = tokio::process::Command::new(source.join("node_modules/.bin/tsx"))
            .arg("--tsconfig")
            .arg(source.join("tsconfig.json"))
            .arg(source.join("packages/orchestrator/src/cli.ts"))
            .args(&case)
            .current_dir(dir.path())
            .env("PI_ORCHESTRATOR_DIR", dir.path())
            .output()
            .await
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let mut stdout = vec![];
        let mut stderr = vec![];
        assert_eq!(
            run_cli(
                &args(&case),
                dir.path().to_str().unwrap(),
                config.clone(),
                tokio::io::empty(),
                &mut stdout,
                &mut stderr
            )
            .await,
            0
        );
        assert_eq!(stdout, output.stdout, "{case:?}");
        assert!(stderr.is_empty());
    }
    server.close().await;
}

#[tokio::test]
async fn rpc_stream_only_forwards_complete_jsonl_stdin_lines() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let dir = tempfile::tempdir().unwrap();
    let config = pi_orchestrator::config::Config { dir: dir.path().into() };
    let listener = tokio::net::UnixListener::bind(config.socket_path()).unwrap();
    let task = tokio::spawn(async move {
        let mut all_requests = vec![];
        for _ in 0..2 {
            let (socket, _) = listener.accept().await.unwrap();
            let (read, mut write) = socket.into_split();
            let mut read = BufReader::new(read);
            let mut line = String::new();
            read.read_line(&mut line).await.unwrap();
            write
                .write_all(b"{\"type\":\"rpc_ready\",\"ok\":true,\"instance\":{\"id\":\"i\"}}\n")
                .await
                .unwrap();
            let mut requests = vec![];
            loop {
                line.clear();
                if tokio::time::timeout(std::time::Duration::from_millis(100), read.read_line(&mut line))
                    .await
                    .is_err()
                {
                    break;
                }
                let request: serde_json::Value = serde_json::from_str(&line).unwrap();
                requests.push(request);
                write
                    .write_all(b"{\"type\":\"response\",\"success\":true}\n")
                    .await
                    .unwrap();
            }
            write.shutdown().await.unwrap();
            all_requests.push(requests);
        }
        all_requests
    });
    let input = "\u{feff}\u{a0}\u{1680}\u{2000}\u{2001}\u{2002}\u{2003}\u{2004}\u{2005}\u{2006}\u{2007}\u{2008}\u{2009}\u{200a}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}\n\u{feff}{\"type\":\"abort\",\"id\":\"complete\"}\u{feff}\n{\"type\":\"get_state\",\"id\":\"incomplete\"}".as_bytes();
    let mut stdout = vec![];
    let mut stderr = vec![];
    assert_eq!(
        pi_orchestrator::cli::run_cli(
            &args(&["rpc-stream", "i"]),
            "/tmp",
            config.clone(),
            input,
            &mut stdout,
            &mut stderr
        )
        .await,
        0
    );
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lock: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("docs/porting/source-lock.json")).unwrap()).unwrap();
    let source = std::path::Path::new(lock["worktree"].as_str().unwrap());
    let mut child = tokio::process::Command::new(source.join("node_modules/.bin/tsx"))
        .arg("--tsconfig")
        .arg(source.join("tsconfig.json"))
        .arg(source.join("packages/orchestrator/src/cli.ts"))
        .args(["rpc-stream", "i"])
        .env("PI_ORCHESTRATOR_DIR", &config.dir)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).await.unwrap();
    let output = child.wait_with_output().await.unwrap();
    assert!(output.status.success());
    assert_eq!(stdout, output.stdout);
    assert_eq!(stderr, output.stderr);
    let expected = vec![json!({"type":"abort","id":"complete"})];
    assert_eq!(task.await.unwrap(), vec![expected.clone(), expected]);
}
