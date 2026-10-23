//! Behavioral tests for `utils/tools-manager.ts`: asset naming, tool discovery, offline handling, and
//! a full download/extract/install cycle against an injected `Fetch` with a real tar.gz archive.

#[path = "support/executable_script.rs"]
mod executable_script;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse};
use pi_coding_agent::utils::tools_manager::{ToolsManager, asset_name};
use pi_js_compat::abort::AbortSignal;
use serde_json::json;

struct StubBody(Vec<u8>);

impl HttpBody for StubBody {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        let chunk = std::mem::take(&mut self.0);
        Box::pin(async move { (!chunk.is_empty()).then_some(Ok(chunk)) })
    }
}

#[derive(Default)]
struct StubFetch {
    responses: Mutex<Vec<(u16, Vec<u8>)>>,
    requests: Mutex<Vec<HttpRequest>>,
}

impl StubFetch {
    fn new(responses: Vec<(u16, Vec<u8>)>) -> Self {
        Self {
            responses: Mutex::new(responses),
            requests: Mutex::new(Vec::new()),
        }
    }
}

impl Fetch for StubFetch {
    fn fetch(
        &self,
        request: HttpRequest,
        _signal: Option<AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        self.requests.lock().unwrap().push(request);
        let (status, body) = self.responses.lock().unwrap().remove(0);
        Box::pin(async move {
            Ok(HttpResponse {
                status,
                status_text: String::new(),
                headers: Vec::new(),
                body: Box::new(StubBody(body)),
            })
        })
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("pi-tools-{name}-{}-{unique}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

#[test]
fn asset_names_match_the_release_layouts() {
    assert_eq!(
        asset_name("fd", "10.2.0", "linux", "x64").as_deref(),
        Some("fd-v10.2.0-x86_64-unknown-linux-gnu.tar.gz")
    );
    assert_eq!(
        asset_name("fd", "10.2.0", "linux", "arm64").as_deref(),
        Some("fd-v10.2.0-aarch64-unknown-linux-gnu.tar.gz")
    );
    assert_eq!(
        asset_name("fd", "10.2.0", "darwin", "x64").as_deref(),
        Some("fd-v10.2.0-x86_64-apple-darwin.tar.gz")
    );
    assert_eq!(
        asset_name("fd", "10.2.0", "win32", "arm64").as_deref(),
        Some("fd-v10.2.0-aarch64-pc-windows-msvc.zip")
    );
    assert_eq!(
        asset_name("rg", "14.1.1", "linux", "x64").as_deref(),
        Some("ripgrep-14.1.1-x86_64-unknown-linux-musl.tar.gz")
    );
    assert_eq!(
        asset_name("rg", "14.1.1", "linux", "arm64").as_deref(),
        Some("ripgrep-14.1.1-aarch64-unknown-linux-gnu.tar.gz")
    );
    assert_eq!(
        asset_name("rg", "14.1.1", "darwin", "x64").as_deref(),
        Some("ripgrep-14.1.1-x86_64-apple-darwin.tar.gz")
    );
    assert_eq!(
        asset_name("rg", "14.1.1", "win32", "x64").as_deref(),
        Some("ripgrep-14.1.1-x86_64-pc-windows-msvc.zip")
    );
    assert_eq!(asset_name("rg", "1.0.0", "freebsd", "x64"), None);
    assert_eq!(asset_name("unknown", "1.0.0", "linux", "x64"), None);
}

#[test]
fn managed_binaries_win_over_system_binaries() {
    let bin_dir = temp_dir("bin");
    let system_path = temp_dir("system-path");
    let managed = bin_dir.join("rg");
    fs::write(&managed, "managed").unwrap();
    write_executable(&system_path.join("rg"), "system");

    let manager = ToolsManager::with_options(
        bin_dir,
        std::sync::Arc::new(StubFetch::new(Vec::new())),
        "linux",
        "x64",
        Some(system_path.display().to_string()),
        false,
    );
    assert_eq!(manager.get_tool_path("rg"), Some(managed.display().to_string()));
    assert_eq!(manager.get_tool_path("unknown"), None);
}

#[test]
fn system_binaries_are_found_on_path_including_the_fd_alias() {
    let bin_dir = temp_dir("bin-alias");
    let system_path = temp_dir("system-alias");
    write_executable(&system_path.join("fdfind"), "system");
    let manager = ToolsManager::with_options(
        bin_dir,
        std::sync::Arc::new(StubFetch::new(Vec::new())),
        "linux",
        "x64",
        Some(system_path.display().to_string()),
        false,
    );
    assert_eq!(manager.get_tool_path("fd"), Some("fdfind".to_owned()));

    let empty = temp_dir("empty-path");
    let manager = ToolsManager::with_options(
        temp_dir("bin-none"),
        std::sync::Arc::new(StubFetch::new(Vec::new())),
        "linux",
        "x64",
        Some(empty.display().to_string()),
        false,
    );
    assert_eq!(manager.get_tool_path("rg"), None);
}

#[test]
fn offline_mode_reports_and_never_fetches() {
    let fetch = std::sync::Arc::new(StubFetch::new(Vec::new()));
    let mut manager = ToolsManager::with_options(
        temp_dir("offline"),
        fetch.clone(),
        "linux",
        "x64",
        Some(temp_dir("offline-path").display().to_string()),
        true,
    );
    assert_eq!(runtime().block_on(manager.ensure_tool("rg")), None);
    assert!(fetch.requests.lock().unwrap().is_empty());
    assert_eq!(
        manager.output(),
        ["ripgrep not found. Offline mode enabled, skipping download."]
    );
}

#[test]
fn android_reports_the_termux_package() {
    let mut manager = ToolsManager::with_options(
        temp_dir("android"),
        std::sync::Arc::new(StubFetch::new(Vec::new())),
        "android",
        "arm64",
        Some(temp_dir("android-path").display().to_string()),
        false,
    );
    assert_eq!(runtime().block_on(manager.ensure_tool("fd")), None);
    assert_eq!(manager.output(), ["fd not found. Install with: pkg install fd"]);
}

#[test]
fn downloads_extracts_and_installs_a_managed_binary() {
    let workspace = temp_dir("download");
    let bin_dir = workspace.join("bin");
    let archive = workspace.join("fd-v1.0.0-x86_64-unknown-linux-gnu.tar.gz");
    build_tar_gz(
        &workspace,
        "fd-v1.0.0-x86_64-unknown-linux-gnu",
        "fd",
        "#!/bin/sh\necho fd\n",
    );
    let archive_bytes = fs::read(&archive).unwrap();
    fs::remove_file(&archive).unwrap();

    let fetch = std::sync::Arc::new(StubFetch::new(vec![
        (200, br#"{"tag_name":"v1.0.0"}"#.to_vec()),
        (200, archive_bytes),
    ]));
    let mut manager = ToolsManager::with_options(
        bin_dir.clone(),
        fetch.clone(),
        "linux",
        "x64",
        Some(temp_dir("download-path").display().to_string()),
        false,
    );
    let installed = runtime().block_on(manager.ensure_tool("fd")).expect("fd installs");
    assert_eq!(installed, bin_dir.join("fd").display().to_string());
    assert!(Path::new(&installed).exists());
    assert_eq!(fs::read_to_string(&installed).unwrap(), "#!/bin/sh\necho fd\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&installed).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o755);
    }
    let remaining: Vec<String> = fs::read_dir(&bin_dir)
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(remaining, ["fd"], "archive and extraction directory are cleaned up");
    let requests = fetch.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0].url,
        "https://api.github.com/repos/sharkdp/fd/releases/latest"
    );
    assert_eq!(
        requests[1].url,
        "https://github.com/sharkdp/fd/releases/download/v1.0.0/fd-v1.0.0-x86_64-unknown-linux-gnu.tar.gz"
    );
    assert_eq!(manager.output().len(), 2);
}

#[test]
fn api_and_download_failures_report_their_status() {
    let fetch = std::sync::Arc::new(StubFetch::new(vec![
        (500, Vec::new()),
        (500, Vec::new()),
        (500, Vec::new()),
    ]));
    let manager = ToolsManager::with_options(
        temp_dir("api-error"),
        fetch,
        "linux",
        "x64",
        Some(temp_dir("api-error-path").display().to_string()),
        false,
    );
    let failure = runtime().block_on(manager.download_tool("rg")).unwrap_err();
    assert_eq!(failure.to_string(), "GitHub API error: 500");

    let fetch = std::sync::Arc::new(StubFetch::new(vec![
        (200, br#"{"tag_name":"14.1.1"}"#.to_vec()),
        (404, Vec::new()),
    ]));
    let manager = ToolsManager::with_options(
        temp_dir("download-error"),
        fetch,
        "linux",
        "x64",
        Some(temp_dir("download-error-path").display().to_string()),
        false,
    );
    let failure = runtime().block_on(manager.download_tool("rg")).unwrap_err();
    assert_eq!(failure.to_string(), "Failed to download: 404");
}

#[test]
fn archives_without_the_binary_report_the_expected_name() {
    let workspace = temp_dir("missing-binary");
    let archive = workspace.join("fd-v1.0.0-x86_64-unknown-linux-gnu.tar.gz");
    build_tar_gz(&workspace, "fd-v1.0.0-x86_64-unknown-linux-gnu", "other", "x");
    let archive_bytes = fs::read(&archive).unwrap();
    fs::remove_file(&archive).unwrap();
    let fetch = std::sync::Arc::new(StubFetch::new(vec![
        (200, br#"{"tag_name":"v1.0.0"}"#.to_vec()),
        (200, archive_bytes),
    ]));
    let bin_dir = workspace.join("bin");
    let manager = ToolsManager::with_options(
        bin_dir.clone(),
        fetch,
        "linux",
        "x64",
        Some(temp_dir("missing-binary-path").display().to_string()),
        false,
    );
    let failure = runtime().block_on(manager.download_tool("fd")).unwrap_err();
    assert!(
        failure
            .to_string()
            .starts_with("Binary not found in archive: expected fd under "),
        "error: {failure}"
    );
    let remaining: Vec<String> = fs::read_dir(&bin_dir)
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(remaining.is_empty(), "leftovers: {remaining:?}");
}

#[test]
fn unsupported_platforms_report_the_pair() {
    let fetch = std::sync::Arc::new(StubFetch::new(vec![(200, br#"{"tag_name":"1.0.0"}"#.to_vec())]));
    let manager = ToolsManager::with_options(
        temp_dir("unsupported"),
        fetch,
        "freebsd",
        "x64",
        Some(temp_dir("unsupported-path").display().to_string()),
        false,
    );
    let failure = runtime().block_on(manager.download_tool("rg")).unwrap_err();
    assert_eq!(failure.to_string(), "Unsupported platform: freebsd/x64");
}

fn write_executable(path: &Path, content: &str) {
    executable_script::write_executable_script(path, content);
}

/// Builds `<dir>/<asset>.tar.gz` containing `<top>/<binary>`.
fn build_tar_gz(dir: &Path, top: &str, binary: &str, content: &str) {
    let stage = dir.join("stage");
    let package = stage.join(top);
    fs::create_dir_all(&package).unwrap();
    fs::write(package.join(binary), content).unwrap();
    let status = std::process::Command::new("tar")
        .arg("czf")
        .arg(dir.join(format!("{top}.tar.gz")))
        .arg("-C")
        .arg(&stage)
        .arg(top)
        .status()
        .expect("tar must run");
    assert!(status.success(), "tar failed");
    fs::remove_dir_all(&stage).unwrap();
}

#[test]
fn unknown_tools_are_rejected() {
    let manager = ToolsManager::with_options(
        temp_dir("unknown-tool"),
        std::sync::Arc::new(StubFetch::new(Vec::new())),
        "linux",
        "x64",
        None,
        false,
    );
    assert_eq!(
        runtime().block_on(manager.download_tool("jq")).unwrap_err().to_string(),
        "Unknown tool: jq"
    );
}

#[test]
fn json_version_payloads_without_a_tag_are_tolerated() {
    let fetch = std::sync::Arc::new(StubFetch::new(vec![(200, json!({}).to_string().into_bytes())]));
    let manager = ToolsManager::with_options(temp_dir("no-tag"), fetch, "freebsd", "x64", None, false);
    let failure = runtime().block_on(manager.download_tool("rg")).unwrap_err();
    assert_eq!(failure.to_string(), "Unsupported platform: freebsd/x64");
}
