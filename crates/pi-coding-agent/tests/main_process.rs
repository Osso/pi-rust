//! Process differentials for the `pi` binary against the pinned `cli.ts`.
//!
//! Startup paths that do not depend on unported bundled extensions or network
//! providers are compared byte-for-byte. `--help`, `tools`, and `extensions`
//! diverge while first-party factories remain pending, so they are covered by
//! behavioral assertions instead.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde_json::Value;

const SOURCE_LOCK: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/porting/source-lock.json"
));

struct SourceLock {
    worktree: PathBuf,
    commit: String,
}

fn source_lock() -> SourceLock {
    let value: Value = serde_json::from_str(SOURCE_LOCK).expect("source lock must be valid JSON");
    SourceLock {
        worktree: PathBuf::from(value["worktree"].as_str().expect("source lock worktree")),
        commit: value["commit"].as_str().expect("source lock commit").to_owned(),
    }
}

fn verify_source_head(lock: &SourceLock) {
    let output = Command::new("git")
        .arg("-C")
        .arg(&lock.worktree)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("git must verify the pinned TypeScript source HEAD");
    assert!(output.status.success(), "git rev-parse failed");
    let actual = String::from_utf8(output.stdout).expect("git HEAD must be UTF-8");
    assert_eq!(
        actual.trim(),
        lock.commit,
        "TypeScript source HEAD must equal docs/porting/source-lock.json"
    );
}

struct ProcessOutput {
    stdout: String,
    stderr: String,
    code: i32,
}

struct Fixture {
    _temp: tempfile::TempDir,
    work: PathBuf,
    agent_dir: PathBuf,
    state_dir: PathBuf,
    home: PathBuf,
    package_dir: PathBuf,
}

fn fixture(lock: &SourceLock) -> Fixture {
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
        std::fs::read(lock.worktree.join("packages/coding-agent/package.json")).expect("frozen package.json"),
    )
    .expect("package.json");
    copy_directory(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("theme"),
        &package_dir.join("theme"),
    );
    // The TypeScript source resolves built-in themes from `src/modes/interactive/theme`.
    copy_directory(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("theme"),
        &package_dir.join("src/modes/interactive/theme"),
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

fn environment(fixture: &Fixture, package_dir: &Path) -> Vec<(String, String)> {
    vec![
        (
            "PI_CODING_AGENT_DIR".to_owned(),
            fixture.agent_dir.display().to_string(),
        ),
        (
            "PI_CODING_AGENT_STATE_DIR".to_owned(),
            fixture.state_dir.display().to_string(),
        ),
        ("HOME".to_owned(), fixture.home.display().to_string()),
        ("PI_PACKAGE_DIR".to_owned(), package_dir.display().to_string()),
        ("PI_OFFLINE".to_owned(), "1".to_owned()),
        ("NO_COLOR".to_owned(), "1".to_owned()),
    ]
}

fn run(command: &mut Command, env: &[(String, String)]) -> ProcessOutput {
    for (name, value) in env {
        command.env(name, value);
    }
    // A TTY-less harness leaves stdin open; piped stdin would block startup.
    command.stdin(std::process::Stdio::null());
    let output = command.output().expect("process must run");
    ProcessOutput {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        code: output.status.code().unwrap_or(-1),
    }
}

fn run_rust(fixture: &Fixture, args: &[&str]) -> ProcessOutput {
    let mut command = Command::new(env!("CARGO_BIN_EXE_pi"));
    command.args(args).current_dir(&fixture.work);
    run(&mut command, &environment(fixture, &fixture.package_dir))
}

/// Runs the binary without `PI_PACKAGE_DIR`, so package.json and the built-in
/// themes come from the assets embedded in the executable.
fn run_rust_with_embedded_assets(fixture: &Fixture, args: &[&str]) -> ProcessOutput {
    let mut command = Command::new(env!("CARGO_BIN_EXE_pi"));
    command.args(args).current_dir(&fixture.work);
    let env: Vec<(String, String)> = environment(fixture, &fixture.package_dir)
        .into_iter()
        .filter(|(name, _)| name != "PI_PACKAGE_DIR")
        .collect();
    command.env_remove("PI_PACKAGE_DIR");
    run(&mut command, &env)
}

fn run_typescript(lock: &SourceLock, fixture: &Fixture, args: &[&str]) -> ProcessOutput {
    let package_dir = lock.worktree.join("packages/coding-agent");
    let mut command = Command::new(lock.worktree.join("node_modules/.bin/tsx"));
    command
        .arg("--tsconfig")
        .arg(lock.worktree.join("tsconfig.json"))
        .arg(package_dir.join("src/cli.ts"))
        .args(args)
        .current_dir(&fixture.work);
    run(&mut command, &environment(fixture, &package_dir))
}

#[test]
fn native_unknown_extension_keeps_frozen_diagnostic_and_exit_one() {
    let temp = tempfile::tempdir().unwrap();
    let agent = temp.path().join("agent");
    std::fs::create_dir_all(&agent).unwrap();
    let extension = temp.path().join("unknown.ts");
    std::fs::write(&extension, "export default () => {};").unwrap();
    std::fs::write(
        agent.join("settings.json"),
        serde_json::to_vec(&serde_json::json!({
            "extensions": [extension],
        }))
        .unwrap(),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_pi"))
        .args(["--print", "--no-session", "hello"])
        .current_dir(temp.path())
        .env("HOME", temp.path())
        .env("PI_CODING_AGENT_DIR", &agent)
        .env("PI_CODING_AGENT_STATE_DIR", temp.path().join("state"))
        .env("PI_OFFLINE", "1")
        .env("NO_COLOR", "1")
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{:?}", output);
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains(&format!(
            "Extension does not export a valid factory function: {}",
            extension.display()
        )),
        "{stderr}"
    );
}

#[test]
fn startup_sweeps_missing_empty_sessions_before_name_validation_unless_standalone() {
    use pi_coding_agent::core::session_control_db::{
        WritableSessionMetadata, get_control_db_path, read_session_metadata, write_session_metadata,
    };
    use pi_js_compat::js_json::JsString;
    let lock = source_lock();
    verify_source_head(&lock);
    for standalone in [false, true] {
        let rust_fixture = fixture(&lock);
        let source_fixture = fixture(&lock);
        let seed = |fixture: &Fixture| {
            let db = get_control_db_path(fixture.state_dir.to_str().unwrap());
            let path = JsString::from(fixture.agent_dir.join("sessions/missing.jsonl").to_str().unwrap());
            let metadata: WritableSessionMetadata = serde_json::from_value(serde_json::json!({"id":"orphan-empty","sessionPath":path,"cwd":fixture.work,"createdAt":"2026-10-23T00:00:00.000Z","modifiedAt":"2026-10-23T00:00:00.000Z","messageCount":0,"firstMessage":""})).unwrap();
            write_session_metadata(&db, &metadata).unwrap();
            (db, path)
        };
        let (rust_db, rust_path) = seed(&rust_fixture);
        let (source_db, source_path) = seed(&source_fixture);
        let args = if standalone {
            vec![
                "--print",
                "--no-supervisor",
                "--no-session",
                "--provider",
                "__empty_cleanup_probe__",
                "--model",
                "__missing__",
            ]
        } else {
            vec!["--print", "--name", ""]
        };
        let source = run_typescript(&lock, &source_fixture, &args);
        let native = run_rust(&rust_fixture, &args);
        assert_eq!(source.code, 1, "source: {}", source.stderr);
        let expected_error = if standalone {
            "Unknown provider \"__empty_cleanup_probe__\""
        } else {
            "--name requires a non-empty value"
        };
        assert!(
            source.stderr.contains(expected_error),
            "source standalone={standalone}: {}",
            source.stderr
        );
        assert_eq!(native.code, source.code, "native: {}", native.stderr);
        assert!(
            native.stderr.contains(expected_error),
            "native standalone={standalone}: {}",
            native.stderr
        );
        let source_retained = read_session_metadata(&source_db, &source_path).unwrap().is_some();
        assert_eq!(source_retained, standalone, "source standalone={standalone}");
        assert_eq!(
            read_session_metadata(&rust_db, &rust_path).unwrap().is_some(),
            source_retained,
            "native standalone={standalone}"
        );
    }
}

// Frozen main.ts:1086-1087 resolves the consumed restart batch before its
// notice. Empty --name stops after this boundary, before any provider runs.
#[test]
fn restart_handoff_resolves_pending_calls_before_notice_and_name_validation() {
    use pi_ai::types::{AssistantContent, AssistantMessage, StopReason, ToolCall};
    let lock = source_lock();
    verify_source_head(&lock);
    let observe = |fixture: &Fixture, typescript: bool| {
        let mut assistant = AssistantMessage::empty("faux", "faux", "faux-1", 0.0);
        assistant.stop_reason = StopReason::ToolUse;
        assistant.content = vec![
            AssistantContent::ToolCall(ToolCall::new("restart-call", "restart_self", Default::default())),
            AssistantContent::ToolCall(ToolCall::new("other-call", "read", Default::default())),
        ];
        use pi_coding_agent::core::session_manager::{SessionManager, get_default_session_dir};
        use pi_js_compat::js_json::JsString;
        let cwd = JsString::from(fixture.work.to_str().unwrap());
        let agent_dir = JsString::from(fixture.agent_dir.to_str().unwrap());
        let directory = get_default_session_dir(&cwd, Some(&agent_dir)).unwrap();
        let mut manager = SessionManager::create(&cwd, Some(&directory), None).unwrap();
        manager
            .append_message(pi_agent_core::agent::AgentMessage::Assistant(Box::new(assistant)))
            .unwrap();
        manager.persist_for_recovery().unwrap();
        let path = PathBuf::from(manager.get_session_file().unwrap().to_utf8().unwrap());
        let package = lock.worktree.join("packages/coding-agent");
        let mut command = if typescript {
            let mut command = Command::new(lock.worktree.join("node_modules/.bin/tsx"));
            command
                .arg("--tsconfig")
                .arg(lock.worktree.join("tsconfig.json"))
                .arg(package.join("src/cli.ts"));
            command
        } else {
            Command::new(env!("CARGO_BIN_EXE_pi"))
        };
        command
            .args(["--print", "--name", ""])
            .current_dir(&fixture.work)
            .env("PI_SELF_RESTART_REQUEST", "1")
            .env("PI_SELF_RESTART_SESSION", &path)
            .env("PI_SELF_RESTART_PROMPT", "Reopened.")
            .env_remove("PI_SELF_RESTART_OLD_PID");
        let package = if typescript { &package } else { &fixture.package_dir };
        let output = run(&mut command, &environment(fixture, package));
        assert_eq!(output.code, 1, "{}", output.stderr);
        assert!(
            output.stderr.contains("--name requires a non-empty value"),
            "{}",
            output.stderr
        );
        let entries: Vec<Value> = std::fs::read_to_string(&path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        entries.into_iter().filter_map(|entry| {
            if entry["type"]=="custom_message" {
                let text=entry["content"].as_str().unwrap();
                assert!(text.starts_with("Reopened. PID: "),"{entry}");
                let pid=text.strip_prefix("Reopened. PID: ").unwrap().strip_suffix('.').unwrap().parse::<u32>().unwrap();
                assert!(pid>0);
                Some(serde_json::json!({"customType":entry["customType"],"notice":"Reopened. PID: <pid>.","display":entry["display"]}))
            } else if entry["message"]["role"]=="toolResult" {
                let mut message=entry["message"].clone();
                assert!(message["timestamp"].as_f64().unwrap()>0.0);
                message.as_object_mut().unwrap().remove("timestamp");
                Some(message)
            } else {None}
        }).collect::<Vec<_>>()
    };
    let expected = observe(&fixture(&lock), true);
    assert_eq!(
        expected.len(),
        3,
        "source must resolve both calls before the notice: {expected:?}"
    );
    assert_eq!(expected[0]["toolCallId"], "restart-call");
    assert_eq!(expected[0]["isError"], false);
    assert_eq!(expected[1]["toolCallId"], "other-call");
    assert_eq!(expected[1]["isError"], true);
    assert_eq!(observe(&fixture(&lock), false), expected);
}

fn assert_matches(lock: &SourceLock, args: &[&str]) {
    verify_source_head(lock);
    let rust_fixture = fixture(lock);
    let ts_fixture = fixture(lock);
    let rust = run_rust(&rust_fixture, args);
    let typescript = run_typescript(lock, &ts_fixture, args);
    let rust_stdout = rust.stdout.replace(&rust_fixture.work.display().to_string(), "<work>");
    let rust_stderr = rust.stderr.replace(&rust_fixture.work.display().to_string(), "<work>");
    let ts_stdout = typescript
        .stdout
        .replace(&ts_fixture.work.display().to_string(), "<work>");
    let ts_stderr = typescript
        .stderr
        .replace(&ts_fixture.work.display().to_string(), "<work>");
    assert_eq!(typescript.code, rust.code, "exit code for {args:?}");
    assert_eq!(ts_stdout, rust_stdout, "stdout for {args:?}");
    assert_eq!(ts_stderr, rust_stderr, "stderr for {args:?}");
}

#[test]
fn version_differential() {
    assert_matches(&source_lock(), &["--version"]);
}

#[test]
fn embedded_assets_version_differential() {
    let lock = source_lock();
    verify_source_head(&lock);
    let rust = run_rust_with_embedded_assets(&fixture(&lock), &["--version"]);
    let typescript = run_typescript(&lock, &fixture(&lock), &["--version"]);
    assert_eq!(typescript.code, rust.code);
    assert_eq!(typescript.stdout, rust.stdout);
    assert_eq!(typescript.stderr, rust.stderr);
}

#[test]
fn embedded_assets_are_byte_copies_of_the_pinned_source() {
    let lock = source_lock();
    verify_source_head(&lock);
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source_dir = lock.worktree.join("packages/coding-agent");
    for (embedded, source) in [
        ("package.json", "package.json"),
        ("theme/dark.json", "src/modes/interactive/theme/dark.json"),
        ("theme/light.json", "src/modes/interactive/theme/light.json"),
    ] {
        assert_eq!(
            std::fs::read(source_dir.join(source)).expect("pinned source asset"),
            std::fs::read(crate_dir.join(embedded)).expect("embedded asset"),
            "{embedded} must equal the pinned {source}"
        );
    }
}

#[test]
fn package_list_differential() {
    assert_matches(&source_lock(), &["list"]);
}

#[test]
fn invalid_session_id_differential() {
    assert_matches(&source_lock(), &["--session-id", "-bad"]);
}

#[test]
fn missing_session_differential() {
    assert_matches(&source_lock(), &["--fork", "--continue"]);
}

#[test]
fn fork_conflict_differential() {
    assert_matches(&source_lock(), &["--fork", "--session", "x"]);
}

#[test]
fn missing_export_file_differential() {
    assert_matches(&source_lock(), &["--export", "missing.jsonl"]);
}

#[test]
fn remove_missing_package_differential() {
    assert_matches(&source_lock(), &["remove", "npm:not-installed"]);
}

#[test]
fn install_without_source_differential() {
    assert_matches(&source_lock(), &["install"]);
}

const SSE_BODY: &str = concat!(
    "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"demo-model\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"hello\"},\"finish_reason\":null}]}\n\n",
    "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"demo-model\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call-1\",\"type\":\"function\",\"function\":{\"name\":\"end_turn\",\"arguments\":\"{\\\"reason\\\":\\\"done\\\"}\"}}]},\"finish_reason\":null}]}\n\n",
    "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"demo-model\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":5,\"total_tokens\":15}}\n\n",
    "data: [DONE]\n\n",
);

fn spawn_sse_server() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("mock provider listener");
    let port = listener.local_addr().expect("mock provider address").port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            std::thread::spawn(move || serve_sse(stream));
        }
    });
    port
}

fn serve_sse(mut stream: TcpStream) {
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
    let mut request = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let Ok(read) = stream.read(&mut buffer) else {
            return;
        };
        if read == 0 {
            return;
        }
        request.extend_from_slice(&buffer[..read]);
        let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&request[..header_end]).to_ascii_lowercase();
        let content_length = headers
            .lines()
            .find_map(|line| line.strip_prefix("content-length:"))
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(0);
        if request.len() >= header_end + 4 + content_length {
            break;
        }
    }
    let response = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        SSE_BODY.len(),
        SSE_BODY
    );
    let _ = stream.write_all(response.as_bytes());
}

fn write_models(fixture: &Fixture, port: u16) {
    let models = format!(
        r#"{{"providers":{{"demo":{{"baseUrl":"http://127.0.0.1:{port}/v1","api":"openai-completions","models":[{{"id":"demo-model","name":"Demo","reasoning":false,"input":["text"],"contextWindow":64000,"maxTokens":4096}}]}}}}"#
    );
    std::fs::write(fixture.agent_dir.join("models.json"), models).expect("models.json");
}

/// Valid `models.json` for tests that must reach the provider. The legacy
/// `write_models` literal above is missing its root brace; see the traceability
/// discrepancy. Keep it until the json-mode differential is fixed.
fn write_valid_models(fixture: &Fixture, port: u16) {
    let models = serde_json::json!({
        "providers": {
            "demo": {
                "baseUrl": format!("http://127.0.0.1:{port}/v1"),
                "api": "openai-completions",
                "models": [{
                    "id": "demo-model",
                    "name": "Demo",
                    "reasoning": false,
                    "input": ["text"],
                    "contextWindow": 64000,
                    "maxTokens": 4096,
                }],
            }
        }
    });
    std::fs::write(fixture.agent_dir.join("models.json"), models.to_string()).expect("models.json");
}

fn normalize_process_output(text: &str, fixture: &Fixture) -> String {
    let text = text.replace(&fixture._temp.path().display().to_string(), "<tmp>");
    let uuid = regex::Regex::new(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}").unwrap();
    let text = uuid.replace_all(&text, "<uuid>").into_owned();
    let timestamp = regex::Regex::new(r#""timestamp":("[^"]*"|-?\d+(?:\.\d+)?)"#).unwrap();
    let text = timestamp
        .replace_all(&text, r#""timestamp":"<timestamp>""#)
        .into_owned();
    let duration = regex::Regex::new(r#""(startedAt|finishedAt|durationMs)":-?\d+(?:\.\d+)?"#).unwrap();
    duration.replace_all(&text, r#""$1":<duration>"#).into_owned()
}

fn assert_model_differential(lock: &SourceLock, mode_args: &[&str]) {
    assert_model_differential_with(lock, mode_args, run_rust);
}

fn assert_model_differential_with(
    lock: &SourceLock,
    mode_args: &[&str],
    run_rust: fn(&Fixture, &[&str]) -> ProcessOutput,
) {
    verify_source_head(lock);
    let port = spawn_sse_server();
    let rust_fixture = fixture(lock);
    let ts_fixture = fixture(lock);
    write_models(&rust_fixture, port);
    write_models(&ts_fixture, port);
    let mut args = vec!["-ne"];
    args.extend_from_slice(mode_args);
    args.extend_from_slice(&["--model", "demo/demo-model", "--api-key", "test", "hello"]);
    let rust = run_rust(&rust_fixture, &args);
    let typescript = run_typescript(lock, &ts_fixture, &args);
    assert_eq!(typescript.code, rust.code, "exit code for {args:?}");
    assert_eq!(
        normalize_process_output(&typescript.stderr, &ts_fixture),
        normalize_process_output(&rust.stderr, &rust_fixture),
        "stderr for {args:?}"
    );
    assert_eq!(
        normalize_process_output(&typescript.stdout, &ts_fixture),
        normalize_process_output(&rust.stdout, &rust_fixture),
        "stdout for {args:?}"
    );
}

#[test]
fn print_mode_differential() {
    assert_model_differential(&source_lock(), &[]);
}

#[test]
fn json_mode_differential() {
    assert_model_differential(&source_lock(), &["--mode", "json"]);
}

#[test]
fn rpc_mode_empty_stdin_differential() {
    assert_model_differential(&source_lock(), &["--mode", "rpc"]);
}

#[test]
fn print_mode_with_embedded_assets_differential() {
    assert_model_differential_with(&source_lock(), &[], run_rust_with_embedded_assets);
}

const TOOL_CALL_BODY: &str = concat!(
    "data: {\"id\":\"chatcmpl-2\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"demo-model\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":null},\"finish_reason\":null}]}\n\n",
    "data: {\"id\":\"chatcmpl-2\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"demo-model\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call-env\",\"type\":\"function\",\"function\":{\"name\":\"bash\",\"arguments\":\"{\\\"command\\\":\\\"printf '%s|%s|%s' \\\\\\\"$PI_CODING_AGENT\\\\\\\" \\\\\\\"$AI_AGENT\\\\\\\" \\\\\\\"$PI_OFFLINE\\\\\\\" > env.txt\\\"}\"}}]},\"finish_reason\":null}]}\n\n",
    "data: {\"id\":\"chatcmpl-2\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"demo-model\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":5,\"total_tokens\":15}}\n\n",
    "data: [DONE]\n\n",
);

const FINAL_BODY: &str = concat!(
    "data: {\"id\":\"chatcmpl-3\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"demo-model\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"done\"},\"finish_reason\":null}]}\n\n",
    "data: {\"id\":\"chatcmpl-3\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"demo-model\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":20,\"completion_tokens\":5,\"total_tokens\":25}}\n\n",
    "data: [DONE]\n\n",
);

fn spawn_tool_call_server() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("mock provider listener");
    let port = listener.local_addr().expect("mock provider address").port();
    let requests = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let first = requests.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0;
            let body = if first { TOOL_CALL_BODY } else { FINAL_BODY };
            std::thread::spawn(move || serve_body(stream, body));
        }
    });
    port
}

fn serve_body(mut stream: TcpStream, body: &str) {
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
    let mut request = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let Ok(read) = stream.read(&mut buffer) else {
            return;
        };
        if read == 0 {
            return;
        }
        request.extend_from_slice(&buffer[..read]);
        let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&request[..header_end]).to_ascii_lowercase();
        let content_length = headers
            .lines()
            .find_map(|line| line.strip_prefix("content-length:"))
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(0);
        if request.len() >= header_end + 4 + content_length {
            break;
        }
    }
    let response = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
}

/// `cli.ts` exports `PI_CODING_AGENT`, and `main.ts --offline` writes
/// `PI_OFFLINE`/`PI_SKIP_VERSION_CHECK`; child processes must observe both.
#[test]
fn offline_environment_reaches_child_processes_differential() {
    let lock = source_lock();
    verify_source_head(&lock);
    let rust_fixture = fixture(&lock);
    let ts_fixture = fixture(&lock);
    let rust_port = spawn_tool_call_server();
    let ts_port = spawn_tool_call_server();
    write_valid_models(&rust_fixture, rust_port);
    write_valid_models(&ts_fixture, ts_port);
    let args = [
        "-ne",
        "--offline",
        "--model",
        "demo/demo-model",
        "--api-key",
        "test",
        "hello",
    ];
    let mut rust_env = environment(&rust_fixture, &rust_fixture.package_dir);
    let mut ts_env = environment(&ts_fixture, &ts_fixture.package_dir);
    rust_env.retain(|(name, _)| name != "PI_OFFLINE");
    ts_env.retain(|(name, _)| name != "PI_OFFLINE");

    let mut rust_command = Command::new(env!("CARGO_BIN_EXE_pi"));
    rust_command
        .args(args)
        .current_dir(&rust_fixture.work)
        .env_remove("AI_AGENT");
    let rust = run(&mut rust_command, &rust_env);

    let package_dir = lock.worktree.join("packages/coding-agent");
    let mut ts_command = Command::new(lock.worktree.join("node_modules/.bin/tsx"));
    ts_command
        .arg("--tsconfig")
        .arg(lock.worktree.join("tsconfig.json"))
        .arg(package_dir.join("src/cli.ts"))
        .args(args)
        .current_dir(&ts_fixture.work)
        .env_remove("AI_AGENT");
    let typescript = run(&mut ts_command, &ts_env);

    assert_eq!(typescript.code, rust.code, "exit code");
    assert_eq!(
        normalize_process_output(&typescript.stdout, &ts_fixture),
        normalize_process_output(&rust.stdout, &rust_fixture),
        "stdout"
    );
    let rust_env_file = std::fs::read_to_string(rust_fixture.work.join("env.txt")).unwrap_or_else(|error| {
        panic!(
            "rust env.txt: {error}\nstdout: {}\nstderr: {}",
            rust.stdout, rust.stderr
        )
    });
    let ts_env_file = std::fs::read_to_string(ts_fixture.work.join("env.txt")).unwrap_or_else(|error| {
        panic!(
            "typescript env.txt: {error}\nstdout: {}\nstderr: {}",
            typescript.stdout, typescript.stderr
        )
    });
    assert_eq!(ts_env_file, rust_env_file);
    assert_eq!(rust_env_file, "true||1");
}

/// `pi extensions` loads the first-party factories registered by `main.ts`.
/// The inventory is sorted by path, so this proves presence and the ported gap
/// list, not registration order (covered by `tests/main.rs`).
#[test]
fn extensions_command_lists_the_ported_first_party_extensions() {
    use pi_coding_agent::main_runner::FIRST_PARTY_EXTENSION_NAMES;

    let fixture = fixture(&source_lock());
    let output = run_rust(&fixture, &["extensions"]);
    assert_eq!(output.code, 0, "stderr: {}", output.stderr);
    assert!(
        output.stdout.contains("session-autoname"),
        "missing session-autoname: {}",
        output.stdout
    );
    let expected: Vec<&str> = FIRST_PARTY_EXTENSION_NAMES.to_vec();
    assert!(
        output
            .stdout
            .contains(&format!("Loaded extensions ({})", expected.len()))
    );
    for name in expected {
        assert!(output.stdout.contains(name), "missing {name}:\n{}", output.stdout);
    }
}

/// Reads stderr lines until an `Error: ` line appears or `timeout` elapses, then kills the child.
fn first_error_line(mut command: Command, timeout: Duration) -> String {
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;

    command.stdout(Stdio::null()).stderr(Stdio::piped());
    let mut child = command.spawn().expect("process must spawn");
    let stderr = child.stderr.take().expect("piped stderr");
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if sender.send(line).is_err() {
                return;
            }
        }
    });
    let deadline = std::time::Instant::now() + timeout;
    let mut found = String::new();
    while let Some(remaining) = deadline.checked_duration_since(std::time::Instant::now()) {
        match receiver.recv_timeout(remaining) {
            Ok(line) if line.starts_with("Error: ") => {
                found = line;
                break;
            }
            Ok(_) => continue,
            Err(_) => break,
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    found
}

/// `pi -p hi` with `PI_PACKAGE_DIR` pointing at a package without built-in
/// themes: TypeScript's `initTheme` falls back to dark, the fallback re-throws,
/// and the CLI error handler reports it. The pinned CLI leaves the event loop
/// alive after the error (source discrepancy), so only its stderr line and the
/// intended exit code (1, set by `runCliActionOrReportError`) are comparable.
#[test]
fn missing_builtin_theme_differential() {
    let lock = source_lock();
    verify_source_head(&lock);

    let rust_fixture = fixture(&lock);
    std::fs::remove_dir_all(rust_fixture.package_dir.join("theme")).expect("remove theme dir");
    let rust = run_rust(&rust_fixture, &["-p", "hi"]);

    let ts_package = rust_fixture._temp.path().join("ts-package");
    std::fs::create_dir_all(ts_package.join("dist/modes/interactive/theme")).expect("ts theme dir");
    let ts_fixture = fixture(&lock);
    let package_dir = lock.worktree.join("packages/coding-agent");
    let mut command = Command::new(lock.worktree.join("node_modules/.bin/tsx"));
    command
        .arg("--tsconfig")
        .arg(lock.worktree.join("tsconfig.json"))
        .arg(package_dir.join("src/cli.ts"))
        .args(["-p", "hi"])
        .current_dir(&ts_fixture.work);
    let mut environment = environment(&ts_fixture, &ts_package);
    for (name, value) in &mut environment {
        command.env(name, value);
    }
    let ts_line = first_error_line(command, Duration::from_secs(120));

    let normalize = |line: &str, theme_dir: &Path| line.replace(&format!("{}/", theme_dir.display()), "<theme>/");
    let rust_line = rust
        .stderr
        .lines()
        .find(|line| line.starts_with("Error: "))
        .expect("rust prints the theme error");
    assert_eq!(
        normalize(rust_line, &rust_fixture.package_dir.join("theme")),
        normalize(&ts_line, &ts_package.join("dist/modes/interactive/theme")),
        "theme init error must match the pinned CLI"
    );
    assert_eq!(rust.code, 1, "Rust exits with the TypeScript exit code");
}

/// `pi -p hi` with a corrupt built-in theme: `getBuiltinThemes` throws a V8
/// `SyntaxError`, the `initTheme` fallback re-throws, and the CLI handler prints
/// `Error: <message>`.
#[test]
fn invalid_builtin_theme_differential() {
    let lock = source_lock();
    verify_source_head(&lock);

    let rust_fixture = fixture(&lock);
    std::fs::write(rust_fixture.package_dir.join("theme/dark.json"), "{ not json").expect("corrupt theme");
    let rust = run_rust(&rust_fixture, &["-p", "hi"]);

    let ts_package = rust_fixture._temp.path().join("ts-bad-package");
    let ts_theme_dir = ts_package.join("dist/modes/interactive/theme");
    std::fs::create_dir_all(&ts_theme_dir).expect("ts theme dir");
    std::fs::write(ts_theme_dir.join("dark.json"), "{ not json").expect("corrupt theme");
    let ts_fixture = fixture(&lock);
    let package_dir = lock.worktree.join("packages/coding-agent");
    let mut command = Command::new(lock.worktree.join("node_modules/.bin/tsx"));
    command
        .arg("--tsconfig")
        .arg(lock.worktree.join("tsconfig.json"))
        .arg(package_dir.join("src/cli.ts"))
        .args(["-p", "hi"])
        .current_dir(&ts_fixture.work);
    for (name, value) in environment(&ts_fixture, &ts_package) {
        command.env(name, value);
    }
    let ts_line = first_error_line(command, Duration::from_secs(120));

    let rust_line = rust
        .stderr
        .lines()
        .find(|line| line.starts_with("Error: "))
        .expect("rust prints the theme error");
    assert_eq!(rust_line, ts_line, "theme parse error must match the pinned CLI");
    assert_eq!(rust.code, 1, "Rust exits with the TypeScript exit code");
}
