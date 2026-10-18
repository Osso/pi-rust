//! Port of `packages/coding-agent/src/config.ts`.
//!
//! Runtime mapping (approved 2026-10-18): the Rust port is a native executable, so it behaves like
//! the TypeScript compiled-binary install everywhere the source branches on `isBunBinary`. The
//! package directory is `PI_PACKAGE_DIR` when set, otherwise the executable's directory; package
//! assets (`theme/`, `export-html/`, `assets/`) and `package.json` live directly in it, and
//! self-update is unavailable because the binary is not managed by a package manager. The
//! package-manager branches stay ported as pure functions driven by an [`InstallContext`] so they
//! remain testable and differentially provable.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::Deserialize;

use crate::utils::js_json;
use crate::utils::paths::{PathInputOptions, normalize_path};

const DEFAULT_PACKAGE_NAME: &str = "@earendil-works/pi-coding-agent";
const DEFAULT_VERSION: &str = "0.0.0";
const DEFAULT_APP_NAME: &str = "pi";
const DEFAULT_CONFIG_DIR_NAME: &str = ".pi";
const DEFAULT_SHARE_VIEWER_URL: &str = "https://pi.dev/session/";
const RELEASES_URL: &str = "https://github.com/earendil-works/pi-mono/releases/latest";

/// Failure while reading the package configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    message: String,
}

impl ConfigError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ConfigError {}

#[derive(Debug, Deserialize)]
struct PackageJson {
    name: Option<String>,
    version: Option<String>,
    #[serde(rename = "piConfig")]
    pi_config: Option<PiConfig>,
}

#[derive(Debug, Deserialize)]
struct PiConfig {
    name: Option<String>,
    #[serde(rename = "configDir")]
    config_dir: Option<String>,
}

/// Package identity derived from `package.json` (`piConfig` overrides the app name and config dir).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageConfig {
    pub package_name: String,
    pub version: String,
    pub app_name: String,
    pub app_title: String,
    pub config_dir_name: String,
}

impl PackageConfig {
    /// Reads `package.json` from `package_dir`; a missing file yields the built-in defaults.
    pub fn load(package_dir: &Path) -> Result<Self, ConfigError> {
        let path = package_dir.join("package.json");
        let content = match std::fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::defaults());
            }
            Err(error) => return Err(ConfigError::new(error.to_string())),
        };
        let value = js_json::parse(&content).map_err(|error| ConfigError::new(error.to_string()))?;
        let parsed: PackageJson = serde_json::from_value(value).map_err(|error| ConfigError::new(error.to_string()))?;
        Ok(Self::from_package_json(&parsed))
    }

    fn from_package_json(package: &PackageJson) -> Self {
        let pi_config_name = package.pi_config.as_ref().and_then(|config| config.name.clone());
        let app_name = pi_config_name.clone().unwrap_or_else(|| DEFAULT_APP_NAME.to_owned());
        let app_title = if pi_config_name.is_some() {
            app_name.clone()
        } else {
            "π".to_owned()
        };
        Self {
            package_name: package.name.clone().unwrap_or_else(|| DEFAULT_PACKAGE_NAME.to_owned()),
            version: package.version.clone().unwrap_or_else(|| DEFAULT_VERSION.to_owned()),
            app_name,
            app_title,
            config_dir_name: package
                .pi_config
                .as_ref()
                .and_then(|config| config.config_dir.clone())
                .unwrap_or_else(|| DEFAULT_CONFIG_DIR_NAME.to_owned()),
        }
    }

    fn defaults() -> Self {
        Self::from_package_json(&PackageJson {
            name: None,
            version: None,
            pi_config: None,
        })
    }
}

static PACKAGE_CONFIG: OnceLock<PackageConfig> = OnceLock::new();

/// The process-wide package constants, read once like the TypeScript module constants.
pub fn package_config() -> &'static PackageConfig {
    PACKAGE_CONFIG.get_or_init(|| {
        PackageConfig::load(&get_package_dir()).unwrap_or_else(|error| {
            panic!("Failed to load package.json: {error}");
        })
    })
}

fn default_path_options() -> PathInputOptions {
    PathInputOptions::default()
}

fn normalize(input: &str) -> String {
    normalize_path(input, &default_path_options()).unwrap_or_else(|_| input.to_owned())
}

fn home_directory() -> PathBuf {
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .unwrap_or_default()
}

fn join(base: &str, segment: &str) -> String {
    PathBuf::from(base).join(segment).display().to_string()
}

// =============================================================================
// Package Asset Paths
// =============================================================================

/// The package directory: `PI_PACKAGE_DIR`, otherwise the executable's directory.
pub fn get_package_dir() -> PathBuf {
    if let Some(directory) = std::env::var_os("PI_PACKAGE_DIR").filter(|value| !value.is_empty()) {
        return PathBuf::from(normalize(&directory.to_string_lossy()));
    }
    std::env::current_exe()
        .ok()
        .and_then(|executable| executable.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Themes shipped next to the executable.
pub fn get_themes_dir() -> PathBuf {
    get_package_dir().join("theme")
}

/// HTML export templates shipped next to the executable.
pub fn get_export_template_dir() -> PathBuf {
    get_package_dir().join("export-html")
}

pub fn get_package_json_path() -> PathBuf {
    get_package_dir().join("package.json")
}

pub fn get_readme_path() -> PathBuf {
    absolute(get_package_dir().join("README.md"))
}

pub fn get_docs_path() -> PathBuf {
    absolute(get_package_dir().join("docs"))
}

pub fn get_examples_path() -> PathBuf {
    absolute(get_package_dir().join("examples"))
}

pub fn get_changelog_path() -> PathBuf {
    absolute(get_package_dir().join("CHANGELOG.md"))
}

/// Interactive assets shipped next to the executable.
pub fn get_interactive_assets_dir() -> PathBuf {
    get_package_dir().join("assets")
}

pub fn get_bundled_interactive_asset_path(name: &str) -> PathBuf {
    get_interactive_assets_dir().join(name)
}

fn absolute(path: PathBuf) -> PathBuf {
    PathBuf::from(normalize(&path.display().to_string()))
}

pub fn expand_tilde_path(path: &str) -> String {
    normalize(path)
}

/// Share viewer URL for a gist id; `PI_SHARE_VIEWER_URL` overrides the base.
pub fn get_share_viewer_url(gist_id: &str) -> String {
    let base_url = std::env::var("PI_SHARE_VIEWER_URL")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_SHARE_VIEWER_URL.to_owned());
    format!("{base_url}#{gist_id}")
}

// =============================================================================
// User Config Paths
// =============================================================================

pub fn env_agent_dir() -> String {
    format!("{}_CODING_AGENT_DIR", package_config().app_name.to_uppercase())
}

pub fn env_session_dir() -> String {
    format!("{}_CODING_AGENT_SESSION_DIR", package_config().app_name.to_uppercase())
}

pub fn env_state_dir() -> String {
    format!("{}_CODING_AGENT_STATE_DIR", package_config().app_name.to_uppercase())
}

fn environment_value(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

/// The XDG config root for Pi (`~/.config/pi/`).
pub fn get_user_config_root() -> PathBuf {
    let config_home = match environment_value("XDG_CONFIG_HOME") {
        Some(directory) => expand_tilde_path(&directory),
        None => join(&home_directory().display().to_string(), ".config"),
    };
    PathBuf::from(join(&config_home, &package_config().app_name))
}

/// The XDG state root for Pi (`~/.local/state/pi/`).
pub fn get_user_state_root() -> PathBuf {
    if let Some(directory) = environment_value(&env_state_dir()) {
        return PathBuf::from(expand_tilde_path(&directory));
    }
    let state_home = match environment_value("XDG_STATE_HOME") {
        Some(directory) => expand_tilde_path(&directory),
        None => join(&join(&home_directory().display().to_string(), ".local"), "state"),
    };
    PathBuf::from(join(&state_home, &package_config().app_name))
}

/// The legacy agent directory (`~/.pi/agent/`).
pub fn get_legacy_agent_dir() -> PathBuf {
    PathBuf::from(join(
        &home_directory().display().to_string(),
        &package_config().config_dir_name,
    ))
    .join("agent")
}

/// The agent directory (`~/.config/pi/agent/`).
pub fn get_agent_dir() -> PathBuf {
    if let Some(directory) = environment_value(&env_agent_dir()) {
        return PathBuf::from(expand_tilde_path(&directory));
    }
    get_user_config_root().join("agent")
}

pub fn get_custom_themes_dir() -> PathBuf {
    get_agent_dir().join("themes")
}

pub fn get_models_path() -> PathBuf {
    get_agent_dir().join("models.json")
}

pub fn get_auth_path() -> PathBuf {
    get_agent_dir().join("auth.json")
}

pub fn get_settings_path() -> PathBuf {
    get_agent_dir().join("settings.json")
}

pub fn get_tools_dir() -> PathBuf {
    get_agent_dir().join("tools")
}

/// Managed binaries directory (fd, rg).
pub fn get_bin_dir() -> PathBuf {
    get_agent_dir().join("bin")
}

pub fn get_prompts_dir() -> PathBuf {
    get_agent_dir().join("prompts")
}

pub fn get_prompt_history_path() -> PathBuf {
    get_agent_dir().join("prompt-history.json")
}

pub fn get_sessions_dir() -> PathBuf {
    get_agent_dir().join("sessions")
}

pub fn get_debug_log_path() -> PathBuf {
    get_agent_dir().join(format!("{}-debug.log", package_config().app_name))
}

// =============================================================================
// Install Method Detection
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallMethod {
    BunBinary,
    Npm,
    Pnpm,
    Yarn,
    Bun,
    Unknown,
}

impl InstallMethod {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BunBinary => "bun-binary",
            Self::Npm => "npm",
            Self::Pnpm => "pnpm",
            Self::Yarn => "yarn",
            Self::Bun => "bun",
            Self::Unknown => "unknown",
        }
    }
}

/// One command of a self-update (an uninstall followed by an install when the package was renamed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfUpdateCommandStep {
    pub command: String,
    pub args: Vec<String>,
    pub display: String,
}

/// A self-update command, with `steps` when the package name changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfUpdateCommand {
    pub command: String,
    pub args: Vec<String>,
    pub display: String,
    pub steps: Option<Vec<SelfUpdateCommandStep>>,
}

/// `SelfUpdatePackageTarget`: a package name, optionally with an exact install spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfUpdatePackageTarget {
    pub package_name: String,
    pub install_spec: Option<String>,
}

impl SelfUpdatePackageTarget {
    pub fn new(package_name: impl Into<String>) -> Self {
        Self {
            package_name: package_name.into(),
            install_spec: None,
        }
    }

    pub fn with_install_spec(package_name: impl Into<String>, install_spec: impl Into<String>) -> Self {
        Self {
            package_name: package_name.into(),
            install_spec: Some(install_spec.into()),
        }
    }

    fn resolved(&self) -> (String, String) {
        (
            self.package_name.clone(),
            self.install_spec.clone().unwrap_or_else(|| self.package_name.clone()),
        )
    }
}

impl From<&str> for SelfUpdatePackageTarget {
    fn from(package_name: &str) -> Self {
        Self::new(package_name)
    }
}

/// Runtime facts the install-method detection branches on. The TypeScript module reads these from
/// `import.meta.url`, `process.execPath`, `process.argv`, `process.platform`, and the environment.
#[derive(Debug, Clone)]
pub struct InstallContext {
    pub package_dir: PathBuf,
    pub exec_path: String,
    /// `process.argv[1]`: the script Node was asked to run.
    pub entrypoint: Option<String>,
    /// The compiled-binary runtime (`isBunBinary` in TypeScript).
    pub is_bun_binary: bool,
    /// A Bun runtime running TypeScript sources.
    pub is_bun_runtime: bool,
    pub platform: String,
    pub home: PathBuf,
    /// `PATH` used for package-manager probes; `None` inherits the process environment.
    pub path_env: Option<String>,
    /// Additional environment values (`PNPM_HOME`, `ProgramFiles`, `ProgramFiles(x86)`).
    pub env: BTreeMap<String, String>,
}

impl InstallContext {
    /// Runtime facts of this process. The Rust binary is always a compiled binary.
    pub fn for_process() -> Self {
        let env = std::env::vars().collect();
        Self {
            package_dir: get_package_dir(),
            exec_path: std::env::current_exe()
                .map(|executable| executable.display().to_string())
                .unwrap_or_default(),
            entrypoint: std::env::args_os().nth(1).map(|arg| arg.to_string_lossy().into_owned()),
            is_bun_binary: true,
            is_bun_runtime: false,
            platform: crate::utils::pi_user_agent::node_platform_name(std::env::consts::OS).to_owned(),
            home: home_directory(),
            path_env: None,
            env,
        }
    }

    fn environment(&self, name: &str) -> Option<&str> {
        self.env.get(name).map(String::as_str).filter(|value| !value.is_empty())
    }

    fn package_dir_string(&self) -> String {
        self.package_dir.display().to_string()
    }

    fn is_windows(&self) -> bool {
        self.platform == "win32"
    }
}

/// `detectInstallMethod`.
pub fn detect_install_method(context: &InstallContext) -> InstallMethod {
    if context.is_bun_binary {
        return InstallMethod::BunBinary;
    }

    let resolved_path = format!("{}\0{}", context.package_dir_string(), context.exec_path)
        .to_lowercase()
        .replace('\\', "/");

    if resolved_path.contains("/pnpm/") || resolved_path.contains("/.pnpm/") {
        return InstallMethod::Pnpm;
    }
    if resolved_path.contains("/yarn/") || resolved_path.contains("/.yarn/") {
        return InstallMethod::Yarn;
    }
    if context.is_bun_runtime || resolved_path.contains("/install/global/node_modules/") {
        return InstallMethod::Bun;
    }
    if resolved_path.contains("/npm/") || resolved_path.contains("/node_modules/") {
        return InstallMethod::Npm;
    }
    InstallMethod::Unknown
}

/// `getUpdateInstruction`: the command when one can be built, otherwise the unavailable instruction.
pub fn get_update_instruction(context: &InstallContext, package_name: &str) -> String {
    let method = detect_install_method(context);
    let target = SelfUpdatePackageTarget::new(package_name);
    match get_self_update_command_for_method(context, method, package_name, &target, None) {
        Some(command) => format!("Run: {}", command.display),
        None => get_self_update_unavailable_instruction(context, package_name, None, None),
    }
}

/// `getSelfUpdateUnavailableInstruction`.
pub fn get_self_update_unavailable_instruction(
    context: &InstallContext,
    package_name: &str,
    npm_command: Option<&[String]>,
    target: Option<&SelfUpdatePackageTarget>,
) -> String {
    let method = detect_install_method(context);
    let target = target
        .cloned()
        .unwrap_or_else(|| SelfUpdatePackageTarget::new(package_name));
    let (_, install_spec) = target.resolved();
    if method == InstallMethod::BunBinary {
        return format!("Download from: {RELEASES_URL}");
    }
    match get_self_update_command_for_method(context, method, package_name, &target, npm_command) {
        Some(command) => {
            if is_managed_by_global_package_manager(context, method, npm_command)
                && !is_self_update_path_writable(context)
            {
                format!(
                    "This installation is managed by a global {} install, but the install path is not writable. Update it yourself with: {}",
                    method.as_str(),
                    command.display
                )
            } else {
                format!(
                    "This installation is not managed by a global {} install. Update it with the package manager, wrapper, or source checkout that provides it.",
                    method.as_str()
                )
            }
        }
        None => format!(
            "Update {install_spec} using the package manager, wrapper, or source checkout that provides this installation."
        ),
    }
}

/// `getSelfUpdateCommand`: a command only when the install is managed by a global package manager and writable.
pub fn get_self_update_command(
    context: &InstallContext,
    package_name: &str,
    npm_command: Option<&[String]>,
    target: Option<&SelfUpdatePackageTarget>,
) -> Option<SelfUpdateCommand> {
    let method = detect_install_method(context);
    let target = target
        .cloned()
        .unwrap_or_else(|| SelfUpdatePackageTarget::new(package_name));
    let command = get_self_update_command_for_method(context, method, package_name, &target, npm_command)?;
    if !is_managed_by_global_package_manager(context, method, npm_command) || !is_self_update_path_writable(context) {
        return None;
    }
    Some(command)
}

fn get_self_update_command_for_method(
    context: &InstallContext,
    method: InstallMethod,
    installed_package_name: &str,
    target: &SelfUpdatePackageTarget,
    npm_command: Option<&[String]>,
) -> Option<SelfUpdateCommand> {
    let (package_name, install_spec) = target.resolved();
    let renamed = package_name != installed_package_name;
    match method {
        InstallMethod::BunBinary | InstallMethod::Unknown => None,
        InstallMethod::Pnpm => {
            let resolved_root = read_command_output(context, "pnpm", &["root".to_owned(), "-g".to_owned()], false)
                .ok()
                .flatten();
            let match_path = if resolved_root.is_some() {
                None
            } else {
                pnpm_global_path(&context.package_dir_string())
            };
            let bin_dir_args = match match_path {
                Some(path) => {
                    let home = context
                        .environment("PNPM_HOME")
                        .map(str::to_owned)
                        .unwrap_or_else(|| host_dirname(&host_dirname(&path)));
                    vec![format!("--config.global-bin-dir={home}")]
                }
                None => Vec::new(),
            };
            let mut install_args = vec![
                "install".to_owned(),
                "-g".to_owned(),
                "--ignore-scripts".to_owned(),
                "--config.minimumReleaseAge=0".to_owned(),
            ];
            install_args.extend(bin_dir_args.iter().cloned());
            install_args.push(install_spec.clone());
            let install_step = make_step_owned("pnpm", install_args);
            let uninstall_step = renamed.then(|| {
                let mut uninstall_args = vec!["remove".to_owned(), "-g".to_owned()];
                uninstall_args.extend(bin_dir_args.iter().cloned());
                uninstall_args.push(installed_package_name.to_owned());
                make_step_owned("pnpm", uninstall_args)
            });
            Some(make_command(install_step, uninstall_step))
        }
        InstallMethod::Yarn => {
            let install_step = make_step("yarn", vec!["global", "add", "--ignore-scripts", install_spec.as_str()]);
            let uninstall_step = renamed.then(|| make_step("yarn", vec!["global", "remove", installed_package_name]));
            Some(make_command(install_step, uninstall_step))
        }
        InstallMethod::Bun => {
            let install_step = make_step(
                "bun",
                vec![
                    "install",
                    "-g",
                    "--ignore-scripts",
                    "--minimum-release-age=0",
                    install_spec.as_str(),
                ],
            );
            let uninstall_step = renamed.then(|| make_step("bun", vec!["uninstall", "-g", installed_package_name]));
            Some(make_command(install_step, uninstall_step))
        }
        InstallMethod::Npm => {
            let configured = npm_command.is_some_and(|args| !args.is_empty());
            let (command, npm_args) = match npm_command.and_then(|args| args.split_first()) {
                Some((command, rest)) => (command.clone(), rest.to_vec()),
                None => ("npm".to_owned(), Vec::new()),
            };
            let inferred = if configured {
                None
            } else {
                get_inferred_npm_install(context)
            };
            let mut prefix_args = npm_args;
            if let Some((_, prefix)) = inferred {
                prefix_args.push("--prefix".to_owned());
                prefix_args.push(prefix);
            }
            let mut install_args = prefix_args.clone();
            install_args.extend([
                "install".to_owned(),
                "-g".to_owned(),
                "--ignore-scripts".to_owned(),
                "--min-release-age=0".to_owned(),
                install_spec.clone(),
            ]);
            let install_step = make_step_owned(&command, install_args);
            let uninstall_step = renamed.then(|| {
                let mut uninstall_args = prefix_args.clone();
                uninstall_args.extend([
                    "uninstall".to_owned(),
                    "-g".to_owned(),
                    installed_package_name.to_owned(),
                ]);
                make_step_owned(&command, uninstall_args)
            });
            Some(make_command(install_step, uninstall_step))
        }
    }
}

fn make_command(
    install_step: SelfUpdateCommandStep,
    uninstall_step: Option<SelfUpdateCommandStep>,
) -> SelfUpdateCommand {
    match uninstall_step {
        None => SelfUpdateCommand {
            command: install_step.command,
            args: install_step.args,
            display: install_step.display,
            steps: None,
        },
        Some(uninstall) => SelfUpdateCommand {
            display: format!("{} && {}", uninstall.display, install_step.display),
            command: install_step.command.clone(),
            args: install_step.args.clone(),
            steps: Some(vec![uninstall, install_step]),
        },
    }
}

fn make_step(command: &str, args: Vec<&str>) -> SelfUpdateCommandStep {
    make_step_owned(command, args.into_iter().map(str::to_owned).collect())
}

fn make_step_owned(command: &str, args: Vec<String>) -> SelfUpdateCommandStep {
    let display = std::iter::once(command)
        .chain(args.iter().map(String::as_str))
        .map(|argument| {
            if argument.contains(char::is_whitespace) {
                format!("\"{argument}\"")
            } else {
                argument.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    SelfUpdateCommandStep {
        command: command.to_owned(),
        args,
        display,
    }
}

/// `readCommandOutput`: package-manager probes inherit the install context's `PATH`.
fn read_command_output(
    context: &InstallContext,
    command: &str,
    args: &[String],
    require_success: bool,
) -> Result<Option<String>, ConfigError> {
    let mut builder = std::process::Command::new(command);
    builder.args(args).stdin(std::process::Stdio::null());
    if let Some(path) = &context.path_env {
        builder.env("PATH", path);
    }
    let output = builder.output();
    let result = match output {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if require_success {
                return Err(ConfigError::new(format!(
                    "Failed to run {}: spawn {} ENOENT",
                    display_command(command, args),
                    command
                )));
            }
            return Ok(None);
        }
        Err(error) => {
            if require_success {
                return Err(ConfigError::new(format!(
                    "Failed to run {}: {error}",
                    display_command(command, args)
                )));
            }
            return Ok(None);
        }
    };
    if result.status.success() {
        let stdout = String::from_utf8_lossy(&result.stdout).trim().to_owned();
        return Ok((!stdout.is_empty()).then_some(stdout));
    }
    if require_success {
        let stderr = String::from_utf8_lossy(&result.stderr).trim().to_owned();
        let reason = if stderr.is_empty() {
            match result.status.code() {
                Some(code) => format!("exit code {code}"),
                None => "exit code unknown".to_owned(),
            }
        } else {
            stderr
        };
        return Err(ConfigError::new(format!(
            "Failed to run {}: {reason}",
            display_command(command, args)
        )));
    }
    Ok(None)
}

fn display_command(command: &str, args: &[String]) -> String {
    std::iter::once(command)
        .chain(args.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" ")
}

/// `getInferredNpmInstall`: `<prefix>/lib/node_modules/<package>` implies `--prefix <prefix>`.
fn get_inferred_npm_install(context: &InstallContext) -> Option<(String, String)> {
    let package_dir = context.package_dir_string();
    let windows = context.is_windows() || package_dir.contains('\\');
    let flavor = PathFlavor { windows };
    let parent = flavor.dirname(&package_dir);
    let root =
        if flavor.basename(&parent).starts_with('@') && flavor.basename(&flavor.dirname(&parent)) == "node_modules" {
            flavor.dirname(&parent)
        } else if flavor.basename(&parent) == "node_modules" {
            parent
        } else {
            return None;
        };
    let root_parent = flavor.dirname(&root);
    if flavor.basename(&root_parent) != "lib" {
        return None;
    }
    Some((root, flavor.dirname(&root_parent)))
}

fn get_global_package_roots(
    context: &InstallContext,
    method: InstallMethod,
    npm_command: Option<&[String]>,
) -> Result<Vec<String>, ConfigError> {
    match method {
        InstallMethod::Npm => {
            let configured = npm_command.is_some_and(|args| !args.is_empty());
            let (command, npm_args) = match npm_command.and_then(|args| args.split_first()) {
                Some((command, rest)) => (command.clone(), rest.to_vec()),
                None => ("npm".to_owned(), Vec::new()),
            };
            if configured && command == "bun" {
                let bun_bin = read_command_output(
                    context,
                    &command,
                    &[
                        npm_args.clone(),
                        vec!["pm".to_owned(), "bin".to_owned(), "-g".to_owned()],
                    ]
                    .concat(),
                    true,
                )?;
                let mut roots = vec![join(
                    &context.home.display().to_string(),
                    ".bun/install/global/node_modules",
                )];
                if let Some(bun_bin) = bun_bin {
                    roots.push(join(&host_dirname(&bun_bin), "install/global/node_modules"));
                }
                return Ok(roots);
            }
            let root = read_command_output(
                context,
                &command,
                &[npm_args, vec!["root".to_owned(), "-g".to_owned()]].concat(),
                configured,
            )?;
            let inferred = if configured {
                None
            } else {
                get_inferred_npm_install(context)
            };
            Ok([root, inferred.map(|(root, _)| root)].into_iter().flatten().collect())
        }
        InstallMethod::Pnpm => {
            let root = read_command_output(context, "pnpm", &["root".to_owned(), "-g".to_owned()], false)?;
            if let Some(root) = root {
                return Ok(vec![root.clone(), host_dirname(&root)]);
            }
            Ok(pnpm_global_path(&context.package_dir_string()).into_iter().collect())
        }
        InstallMethod::Yarn => {
            let directory = read_command_output(context, "yarn", &["global".to_owned(), "dir".to_owned()], false)?;
            Ok(directory
                .map(|directory| vec![directory.clone(), join(&directory, "node_modules")])
                .unwrap_or_default())
        }
        InstallMethod::Bun => {
            let bun_bin = read_command_output(
                context,
                "bun",
                &["pm".to_owned(), "bin".to_owned(), "-g".to_owned()],
                false,
            )?;
            let mut roots = vec![join(
                &context.home.display().to_string(),
                ".bun/install/global/node_modules",
            )];
            if let Some(bun_bin) = bun_bin {
                roots.push(join(&host_dirname(&bun_bin), "install/global/node_modules"));
            }
            Ok(roots)
        }
        InstallMethod::BunBinary | InstallMethod::Unknown => Ok(Vec::new()),
    }
}

/// `isManagedByGlobalPackageManager`: the package directory lives under a global package root.
fn is_managed_by_global_package_manager(
    context: &InstallContext,
    method: InstallMethod,
    npm_command: Option<&[String]>,
) -> bool {
    let package_dirs: Vec<String> = [Some(context.package_dir_string()), get_entrypoint_package_dir(context)]
        .into_iter()
        .flatten()
        .collect();
    let package_dir_candidates: Vec<String> = package_dirs
        .iter()
        .flat_map(|directory| path_comparison_candidates(directory))
        .collect();
    let separator = std::path::MAIN_SEPARATOR;
    get_global_package_roots(context, method, npm_command)
        .unwrap_or_default()
        .into_iter()
        .any(|root| {
            path_comparison_candidates(&root).into_iter().any(|normalized_root| {
                let root_prefix = if normalized_root.ends_with(separator) {
                    normalized_root
                } else {
                    format!("{normalized_root}{separator}")
                };
                package_dir_candidates
                    .iter()
                    .any(|package_dir| package_dir.starts_with(&root_prefix))
            })
        })
}

fn path_comparison_candidates(path: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    for resolve_symlinks in [false, true] {
        if let Some(candidate) = normalize_existing_path_for_comparison(path, resolve_symlinks)
            && !candidates.contains(&candidate)
        {
            candidates.push(candidate);
        }
    }
    candidates
}

fn normalize_existing_path_for_comparison(path: &str, resolve_symlinks: bool) -> Option<String> {
    let resolved = host_resolve(path);
    if !Path::new(&resolved).exists() {
        return None;
    }
    let mut normalized = resolved;
    if resolve_symlinks {
        normalized = std::fs::canonicalize(&normalized).ok()?.display().to_string();
    }
    if cfg!(windows) {
        normalized = normalized.to_lowercase();
    }
    Some(normalized)
}

fn host_resolve(path: &str) -> String {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    crate::utils::paths::resolve_path(path, &cwd.display().to_string(), &default_path_options())
        .unwrap_or_else(|_| path.to_owned())
}

fn host_dirname(path: &str) -> String {
    PathFlavor { windows: false }.dirname(path)
}

/// `getEntrypointPackageDir`: walk up from `process.argv[1]` to the nearest `package.json`.
fn get_entrypoint_package_dir(context: &InstallContext) -> Option<String> {
    let entrypoint = context.entrypoint.as_deref()?;
    let flavor = PathFlavor {
        windows: context.is_windows() || entrypoint.contains('\\'),
    };
    let mut directory = flavor.dirname(entrypoint);
    while directory != flavor.dirname(&directory) {
        if Path::new(&flavor.join(&directory, "package.json")).exists() {
            return Some(directory);
        }
        directory = flavor.dirname(&directory);
    }
    None
}

/// `isSelfUpdatePathWritable`: both the package directory and its parent accept writes.
fn is_self_update_path_writable(context: &InstallContext) -> bool {
    let package_dir = context.package_dir.clone();
    let Some(parent) = package_dir.parent() else {
        return false;
    };
    is_writable(&package_dir) && is_writable(parent)
}

fn is_writable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        rustix::fs::access(path, rustix::fs::Access::WRITE_OK).is_ok()
    }
    #[cfg(not(unix))]
    {
        std::fs::metadata(path)
            .map(|metadata| !metadata.permissions().readonly())
            .unwrap_or(false)
    }
}

/// `dirname(dirname(packageDir))` when the package sits under `<...>/global/<name>/.pnpm/`.
fn pnpm_global_path(package_dir: &str) -> Option<String> {
    let regex = regex::Regex::new(r"^(.*[\\/]global[\\/][^\\/]+)[\\/]\.pnpm[\\/]").ok()?;
    regex
        .captures(package_dir)
        .and_then(|captures| captures.get(1))
        .map(|matched| matched.as_str().to_owned())
}

/// Minimal `path.posix` / `path.win32` helpers for the package-manager path shapes.
struct PathFlavor {
    windows: bool,
}

impl PathFlavor {
    fn separator(&self) -> char {
        if self.windows { '\\' } else { '/' }
    }

    fn normalize(&self, path: &str) -> String {
        if self.windows {
            path.replace('/', "\\")
        } else {
            path.to_owned()
        }
    }

    fn dirname(&self, path: &str) -> String {
        let path = self.normalize(path);
        let trimmed = path.trim_end_matches(self.separator());
        if trimmed.is_empty() {
            return self.separator().to_string();
        }
        if self.windows && trimmed.len() == 2 && trimmed.ends_with(':') {
            return format!("{trimmed}{}", self.separator());
        }
        if let Some(index) = trimmed.rfind(self.separator()) {
            let head = trimmed[..index].trim_end_matches(self.separator());
            if head.is_empty() {
                return self.separator().to_string();
            }
            if self.windows && head.len() == 2 && head.ends_with(':') {
                return format!("{head}{}", self.separator());
            }
            return head.to_owned();
        }
        ".".to_owned()
    }

    fn basename(&self, path: &str) -> String {
        let path = self.normalize(path);
        let trimmed = path.trim_end_matches(self.separator());
        match trimmed.rfind(self.separator()) {
            Some(index) => trimmed[index + self.separator().len_utf8()..].to_owned(),
            None => trimmed.to_owned(),
        }
    }

    fn join(&self, base: &str, segment: &str) -> String {
        let base = self.normalize(base);
        let segment = self.normalize(segment);
        if base.is_empty() {
            return segment;
        }
        if base.ends_with(self.separator()) {
            return format!("{base}{segment}");
        }
        format!("{base}{}{segment}", self.separator())
    }
}
