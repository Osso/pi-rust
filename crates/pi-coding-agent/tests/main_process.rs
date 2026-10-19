//! Process differentials for the `pi` binary against the pinned `cli.ts`.
//!
//! Only startup paths that do not depend on still-unported bundled extensions or
//! network providers are compared; `--help`, `tools`, and `extensions` diverge
//! until the pending first-party factories land.

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
        r#"{"name":"@earendil-works/pi-coding-agent","version":"0.84.1"}"#,
    )
    .expect("package.json");
    copy_directory(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("theme"),
        &package_dir.join("theme"),
    );
    let _ = lock;
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
