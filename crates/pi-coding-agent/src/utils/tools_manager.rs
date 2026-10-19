//! Port of `packages/coding-agent/src/utils/tools-manager.ts`.
//!
//! Managed binaries (`fd`, `rg`) are downloaded into the agent's `bin/` directory. HTTP goes through
//! the injected `Fetch` contract; archives are extracted with the system `tar`/`unzip`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use pi_ai::request::{Fetch, HttpRequest};
use serde_json::Value;

use crate::config::{get_bin_dir, package_config};
use crate::utils::chalk::{Chalk, Level};
use crate::utils::management_http::{FetchRetryOptions, fetch_with_retry};

const NETWORK_TIMEOUT_MS: u64 = 10_000;
const DOWNLOAD_TIMEOUT_MS: u64 = 120_000;

/// Download configuration for a managed tool.
#[derive(Debug, Clone)]
pub struct ToolConfig {
    pub name: &'static str,
    pub repo: &'static str,
    pub binary_name: &'static str,
    pub system_binary_names: &'static [&'static str],
    pub tag_prefix: &'static str,
}

/// Managed tools keyed by id.
pub fn tool_config(tool: &str) -> Option<&'static ToolConfig> {
    static FD: ToolConfig = ToolConfig {
        name: "fd",
        repo: "sharkdp/fd",
        binary_name: "fd",
        system_binary_names: &["fd", "fdfind"],
        tag_prefix: "v",
    };
    static RG: ToolConfig = ToolConfig {
        name: "ripgrep",
        repo: "BurntSushi/ripgrep",
        binary_name: "rg",
        system_binary_names: &[],
        tag_prefix: "",
    };
    match tool {
        "fd" => Some(&FD),
        "rg" => Some(&RG),
        _ => None,
    }
}

/// `getAssetName`: the release asset for a platform/architecture pair.
pub fn asset_name(tool: &str, version: &str, platform: &str, architecture: &str) -> Option<String> {
    let arm = architecture == "arm64";
    match (tool, platform) {
        ("fd", "darwin") => Some(format!(
            "fd-v{version}-{}-apple-darwin.tar.gz",
            if arm { "aarch64" } else { "x86_64" }
        )),
        ("fd", "linux") => Some(format!(
            "fd-v{version}-{}-unknown-linux-gnu.tar.gz",
            if arm { "aarch64" } else { "x86_64" }
        )),
        ("fd", "win32") => Some(format!(
            "fd-v{version}-{}-pc-windows-msvc.zip",
            if arm { "aarch64" } else { "x86_64" }
        )),
        ("rg", "darwin") => Some(format!(
            "ripgrep-{version}-{}-apple-darwin.tar.gz",
            if arm { "aarch64" } else { "x86_64" }
        )),
        ("rg", "linux") if arm => Some(format!("ripgrep-{version}-aarch64-unknown-linux-gnu.tar.gz")),
        ("rg", "linux") => Some(format!("ripgrep-{version}-x86_64-unknown-linux-musl.tar.gz")),
        ("rg", "win32") => Some(format!(
            "ripgrep-{version}-{}-pc-windows-msvc.zip",
            if arm { "aarch64" } else { "x86_64" }
        )),
        _ => None,
    }
}

/// Termux package names for the Android fallback message.
fn termux_package(tool: &str) -> &str {
    match tool {
        "fd" => "fd",
        _ => "ripgrep",
    }
}

/// Failure while installing a tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolsError {
    message: String,
}

impl std::fmt::Display for ToolsError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ToolsError {}

fn error(message: impl Into<String>) -> ToolsError {
    ToolsError {
        message: message.into(),
    }
}

/// The tools manager. `for_process` mirrors the module-level globals; other constructors inject the
/// platform, `PATH`, offline flag, and output sink for tests.
pub struct ToolsManager {
    bin_dir: PathBuf,
    fetch: Arc<dyn Fetch>,
    platform: String,
    architecture: String,
    path_env: Option<String>,
    offline: bool,
    silent: bool,
    chalk: Chalk,
    output: Arc<Mutex<Vec<String>>>,
}

impl ToolsManager {
    pub fn for_process(fetch: Arc<dyn Fetch>) -> Self {
        Self {
            bin_dir: get_bin_dir(),
            fetch,
            platform: pi_ai_platform(),
            architecture: pi_ai_arch(),
            path_env: None,
            offline: pi_js_compat::process_env::get("PI_OFFLINE").is_some_and(|value| is_offline_value(&value)),
            silent: false,
            chalk: Chalk::stdout(),
            output: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn with_options(
        bin_dir: PathBuf,
        fetch: Arc<dyn Fetch>,
        platform: &str,
        architecture: &str,
        path_env: Option<String>,
        offline: bool,
    ) -> Self {
        Self {
            bin_dir,
            fetch,
            platform: platform.to_owned(),
            architecture: architecture.to_owned(),
            path_env,
            offline,
            silent: false,
            chalk: Chalk::new(Level::Plain),
            output: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn set_silent(&mut self, silent: bool) {
        self.silent = silent;
    }

    pub fn output(&self) -> Vec<String> {
        self.output
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn log(&self, line: String) {
        if self.silent {
            return;
        }
        self.output
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(line);
    }

    fn command_exists(&self, command: &str) -> bool {
        let mut builder = std::process::Command::new(command);
        builder
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        if let Some(path) = &self.path_env {
            builder.env("PATH", path);
        }
        builder.status().is_ok()
    }

    /// `getToolPath`: the managed binary, then a system binary on `PATH`.
    pub fn get_tool_path(&self, tool: &str) -> Option<String> {
        let config = tool_config(tool)?;
        let extension = if self.platform == "win32" { ".exe" } else { "" };
        let local_path = self.bin_dir.join(format!("{}{extension}", config.binary_name));
        if local_path.exists() {
            return Some(local_path.display().to_string());
        }
        let system_names: &[&str] = if config.system_binary_names.is_empty() {
            &[config.binary_name]
        } else {
            config.system_binary_names
        };
        system_names
            .iter()
            .find(|name| self.command_exists(name))
            .map(|name| (*name).to_owned())
    }

    /// `ensureTool`: existing binary, offline/Android message, or download.
    pub async fn ensure_tool(&mut self, tool: &str) -> Option<String> {
        if let Some(path) = self.get_tool_path(tool) {
            return Some(path);
        }
        let config = tool_config(tool)?;

        if self.offline {
            self.log(self.chalk.yellow(&format!(
                "{} not found. Offline mode enabled, skipping download.",
                config.name
            )));
            return None;
        }
        if self.platform == "android" {
            self.log(self.chalk.yellow(&format!(
                "{} not found. Install with: pkg install {}",
                config.name,
                termux_package(tool)
            )));
            return None;
        }

        self.log(self.chalk.dim(&format!("{} not found. Downloading...", config.name)));
        match self.download_tool(tool).await {
            Ok(path) => {
                self.log(self.chalk.dim(&format!("{} installed to {path}", config.name)));
                Some(path)
            }
            Err(failure) => {
                self.log(
                    self.chalk
                        .yellow(&format!("Failed to download {}: {}", config.name, failure)),
                );
                None
            }
        }
    }

    /// `downloadTool`: resolve the release, download, extract, install, and clean up.
    pub async fn download_tool(&self, tool: &str) -> Result<String, ToolsError> {
        let config = tool_config(tool).ok_or_else(|| error(format!("Unknown tool: {tool}")))?;
        let mut version = self.get_latest_version(config.repo).await?;
        if tool == "fd" && self.platform == "darwin" && self.architecture == "x64" {
            version = "10.3.0".to_owned();
        }
        let asset = asset_name(tool, &version, &self.platform, &self.architecture)
            .ok_or_else(|| error(format!("Unsupported platform: {}/{}", self.platform, self.architecture)))?;
        std::fs::create_dir_all(&self.bin_dir)
            .map_err(|failure| error(format!("Failed to create {}: {failure}", self.bin_dir.display())))?;

        let download_url = format!(
            "https://github.com/{}/releases/download/{}{version}/{asset}",
            config.repo, config.tag_prefix
        );
        let archive_path = self.bin_dir.join(&asset);
        let binary_extension = if self.platform == "win32" { ".exe" } else { "" };
        let binary_path = self.bin_dir.join(format!("{}{binary_extension}", config.binary_name));
        self.download_file(&download_url, &archive_path).await?;

        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let extract_dir = self.bin_dir.join(format!(
            "extract_tmp_{}_{}_{}_{unique}",
            config.binary_name,
            std::process::id(),
            pi_ai::request::now_millis()
        ));
        std::fs::create_dir_all(&extract_dir)
            .map_err(|failure| error(format!("Failed to create {}: {failure}", extract_dir.display())))?;

        let result = self
            .install_from_archive(&asset, &archive_path, &extract_dir, config, binary_extension)
            .and_then(|extracted| {
                std::fs::rename(&extracted, &binary_path)
                    .map_err(|failure| error(format!("Failed to install {}: {failure}", binary_path.display())))
            });
        let _ = std::fs::remove_file(&archive_path);
        let _ = std::fs::remove_dir_all(&extract_dir);
        result?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&binary_path, std::fs::Permissions::from_mode(0o755));
        }
        Ok(binary_path.display().to_string())
    }

    fn install_from_archive(
        &self,
        asset: &str,
        archive_path: &Path,
        extract_dir: &Path,
        config: &ToolConfig,
        binary_extension: &str,
    ) -> Result<PathBuf, ToolsError> {
        if asset.ends_with(".tar.gz") {
            self.extract_tar_gz(archive_path, extract_dir, asset)?;
        } else if asset.ends_with(".zip") {
            self.extract_zip(archive_path, extract_dir, asset)?;
        } else {
            return Err(error(format!("Unsupported archive format: {asset}")));
        }

        let binary_file_name = format!("{}{binary_extension}", config.binary_name);
        let extracted_dir = extract_dir.join(asset.replace(".tar.gz", "").replace(".zip", ""));
        for candidate in [
            extracted_dir.join(&binary_file_name),
            extract_dir.join(&binary_file_name),
        ] {
            if candidate.exists() {
                return Ok(candidate);
            }
        }
        find_binary_recursively(extract_dir, &binary_file_name).ok_or_else(|| {
            error(format!(
                "Binary not found in archive: expected {binary_file_name} under {}",
                extract_dir.display()
            ))
        })
    }

    fn extract_tar_gz(&self, archive_path: &Path, extract_dir: &Path, asset: &str) -> Result<(), ToolsError> {
        if let Some(failure) = self.run_extraction_command(
            "tar",
            &[
                "xzf".to_owned(),
                archive_path.display().to_string(),
                "-C".to_owned(),
                extract_dir.display().to_string(),
            ],
        ) {
            return Err(error(format!("Failed to extract {asset}: {failure}")));
        }
        Ok(())
    }

    fn extract_zip(&self, archive_path: &Path, extract_dir: &Path, asset: &str) -> Result<(), ToolsError> {
        let failures = if self.platform == "win32" {
            self.extract_zip_on_windows(archive_path, extract_dir)
        } else {
            self.extract_zip_on_unix(archive_path, extract_dir)
        };
        match failures {
            None => Ok(()),
            Some(failures) => Err(error(format!("Failed to extract {asset}: {failures}"))),
        }
    }

    fn extract_zip_on_windows(&self, archive_path: &Path, extract_dir: &Path) -> Option<String> {
        let mut failures = Vec::new();
        let tar_command = self.windows_tar_command();
        let tar_failure = self.run_extraction_command(
            &tar_command,
            &[
                "xf".to_owned(),
                archive_path.display().to_string(),
                "-C".to_owned(),
                extract_dir.display().to_string(),
            ],
        );
        failures.push(tar_failure?);

        let powershell_failure = self.run_extraction_command(
            "powershell.exe",
            &powershell_expand_archive_args(archive_path, extract_dir),
        );
        failures.push(powershell_failure?);
        Some(failures.join("; "))
    }

    fn extract_zip_on_unix(&self, archive_path: &Path, extract_dir: &Path) -> Option<String> {
        let mut failures = Vec::new();
        let unzip_failure = self.run_extraction_command(
            "unzip",
            &[
                "-q".to_owned(),
                archive_path.display().to_string(),
                "-d".to_owned(),
                extract_dir.display().to_string(),
            ],
        );
        failures.push(unzip_failure?);

        let tar_failure = self.run_extraction_command(
            "tar",
            &[
                "xf".to_owned(),
                archive_path.display().to_string(),
                "-C".to_owned(),
                extract_dir.display().to_string(),
            ],
        );
        failures.push(tar_failure?);
        Some(failures.join("; "))
    }

    fn windows_tar_command(&self) -> String {
        for variable in ["SystemRoot", "WINDIR"] {
            if let Ok(system_root) = std::env::var(variable)
                && !system_root.is_empty()
            {
                let system_tar = Path::new(&system_root).join("System32").join("tar.exe");
                if system_tar.exists() {
                    return system_tar.display().to_string();
                }
            }
        }
        "tar.exe".to_owned()
    }

    fn run_extraction_command(&self, command: &str, args: &[String]) -> Option<String> {
        let output = std::process::Command::new(command).args(args).output();
        match output {
            Ok(output) if output.status.success() => None,
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
                let reason = if !stderr.is_empty() {
                    stderr
                } else if !stdout.is_empty() {
                    stdout
                } else {
                    match output.status.code() {
                        Some(code) => format!("exit status {code}"),
                        None => "exit status unknown".to_owned(),
                    }
                };
                Some(format!("{command}: {reason}"))
            }
            Err(failure) => Some(format!("{command}: {failure}")),
        }
    }

    async fn get_latest_version(&self, repo: &str) -> Result<String, ToolsError> {
        let response = fetch_with_retry(
            self.fetch.as_ref(),
            HttpRequest {
                method: "GET".to_owned(),
                url: format!("https://api.github.com/repos/{repo}/releases/latest"),
                headers: vec![(
                    "user-agent".to_owned(),
                    format!("{}-coding-agent", package_config().app_name),
                )],
                body: None,
            },
            None,
            &FetchRetryOptions {
                max_retries: None,
                retry_on_status: true,
                timeout_ms: Some(NETWORK_TIMEOUT_MS),
            },
        )
        .await
        .map_err(|failure| error(failure.to_string()))?;
        if !(200..300).contains(&response.status) {
            return Err(error(format!("GitHub API error: {}", response.status)));
        }
        let body = read_body(response.body)
            .await
            .ok_or_else(|| error("No response body"))?;
        let data: Value = serde_json::from_str(&body).map_err(|failure| error(failure.to_string()))?;
        let tag = data.get("tag_name").and_then(Value::as_str).unwrap_or_default();
        Ok(tag.trim_start_matches('v').to_owned())
    }

    async fn download_file(&self, url: &str, destination: &Path) -> Result<(), ToolsError> {
        let response = fetch_with_retry(
            self.fetch.as_ref(),
            HttpRequest {
                method: "GET".to_owned(),
                url: url.to_owned(),
                headers: Vec::new(),
                body: None,
            },
            None,
            &FetchRetryOptions {
                max_retries: None,
                retry_on_status: true,
                timeout_ms: Some(DOWNLOAD_TIMEOUT_MS),
            },
        )
        .await
        .map_err(|failure| error(failure.to_string()))?;
        if !(200..300).contains(&response.status) {
            return Err(error(format!("Failed to download: {}", response.status)));
        }
        let mut file = std::fs::File::create(destination)
            .map_err(|failure| error(format!("Failed to create {}: {failure}", destination.display())))?;
        let mut body = response.body;
        while let Some(chunk) = body.next_chunk().await {
            let chunk = chunk.map_err(|failure| error(failure.to_string()))?;
            file.write_all(&chunk)
                .map_err(|failure| error(format!("Failed to write archive: {failure}")))?;
        }
        Ok(())
    }
}

fn is_offline_value(value: &str) -> bool {
    value == "1" || value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("yes")
}

fn pi_ai_platform() -> String {
    crate::utils::pi_user_agent::node_platform_name(std::env::consts::OS).to_owned()
}

fn pi_ai_arch() -> String {
    crate::utils::pi_user_agent::node_arch_name(std::env::consts::ARCH).to_owned()
}

async fn read_body(mut body: Box<dyn pi_ai::request::HttpBody>) -> Option<String> {
    let mut bytes = Vec::new();
    while let Some(chunk) = body.next_chunk().await {
        bytes.extend_from_slice(&chunk.ok()?);
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

fn powershell_expand_archive_args(archive_path: &Path, extract_dir: &Path) -> Vec<String> {
    const SCRIPT: &str = "& { param($archive, $destination) $ErrorActionPreference = 'Stop'; Expand-Archive -LiteralPath $archive -DestinationPath $destination -Force }";
    vec![
        "-NoLogo".to_owned(),
        "-NoProfile".to_owned(),
        "-NonInteractive".to_owned(),
        "-ExecutionPolicy".to_owned(),
        "Bypass".to_owned(),
        "-Command".to_owned(),
        SCRIPT.to_owned(),
        archive_path.display().to_string(),
        extract_dir.display().to_string(),
    ]
}

fn find_binary_recursively(root_dir: &Path, binary_file_name: &str) -> Option<PathBuf> {
    let mut stack = vec![root_dir.to_path_buf()];
    while let Some(current_dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current_dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && entry.file_name().to_string_lossy() == binary_file_name {
                return Some(path);
            }
            if path.is_dir() {
                stack.push(path);
            }
        }
    }
    None
}
