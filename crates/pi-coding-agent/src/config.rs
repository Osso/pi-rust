//! Port of `packages/coding-agent/src/config.ts`.
//!
//! Runtime mapping (approved 2026-10-18): the Rust port is a native executable, so it behaves like
//! the TypeScript compiled-binary install everywhere the source branches on `isBunBinary`. The
//! package directory is `PI_PACKAGE_DIR` when set, otherwise the executable's directory; package
//! assets (`theme/`, `export-html/`, `assets/`) and `package.json` live directly in it, and
//! self-update is unavailable because the binary is not managed by a package manager. The
//! package-manager branches stay ported as pure functions driven by an [`InstallContext`] so they
//! remain testable and differentially provable.
//!
//! Embedded assets (approved deviation 2026-10-18): without `PI_PACKAGE_DIR`, `package.json` and
//! the built-in `dark`/`light` themes come from copies of the pinned source compiled into the
//! executable instead of files beside it, so a bare binary starts. `PI_PACKAGE_DIR` restores the
//! disk reads.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde_json::Value;

use crate::utils::child_process::node_spawn_error_message;
use crate::utils::paths::{PathInputOptions, node_dirname, node_join, node_resolve, normalize_path, process_cwd};
use crate::utils::uncaught::{ERROR, throw_uncaught};
use pi_js_compat::js_format;
use pi_js_compat::js_json::{self, JsString, encode_node_utf8};
use pi_js_compat::node_error::{NativeErrorClass, NativeNodeError};
use pi_js_compat::string::is_js_whitespace_unit;

const DEFAULT_PACKAGE_NAME: &str = "@earendil-works/pi-coding-agent";
const DEFAULT_VERSION: &str = "0.0.0";
const DEFAULT_APP_NAME: &str = "pi";
const DEFAULT_CONFIG_DIR_NAME: &str = ".pi";
const DEFAULT_SHARE_VIEWER_URL: &str = "https://pi.dev/session/";
const RELEASES_URL: &str = "https://github.com/earendil-works/pi-mono/releases/latest";
const EMBEDDED_PACKAGE_JSON: &str = include_str!("../package.json");
const EMBEDDED_DARK_THEME: &str = include_str!("../theme/dark.json");
const EMBEDDED_LIGHT_THEME: &str = include_str!("../theme/light.json");

/// Failure while reading the package configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigError {
    label: &'static str,
    message: JsString,
    native: Option<Box<NativeNodeError>>,
}

impl ConfigError {
    pub fn message(&self) -> &JsString {
        &self.message
    }

    pub fn native_error(&self) -> Option<&NativeNodeError> {
        self.native.as_deref()
    }

    pub fn into_native_error(self) -> NativeNodeError {
        match self.native {
            Some(error) => *error,
            None => NativeNodeError::new(self.label, self.message),
        }
    }

    /// Only source callback sites without a catch use this; getters remain fallible.
    pub fn raise_uncaught(self) -> ! {
        std::panic::panic_any(self.into_native_error())
    }

    fn new(message: impl Into<JsString>) -> Self {
        Self {
            label: ERROR,
            message: message.into(),
            native: None,
        }
    }

    fn syntax(message: impl Into<JsString>) -> Self {
        Self::from(NativeNodeError::from_constructor(
            NativeErrorClass::SyntaxError,
            message,
        ))
    }

    /// Node error name used by the process-level uncaught handler.
    pub fn label(&self) -> &'static str {
        self.label
    }
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&pi_js_compat::js_json::encode_node_utf8(&self.message))
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.native_error().map(|error| error as &dyn std::error::Error)
    }
}
impl From<NativeNodeError> for ConfigError {
    fn from(error: NativeNodeError) -> Self {
        Self {
            label: error.constructor_class().name(),
            message: error.message.clone(),
            native: Some(Box::new(error)),
        }
    }
}
impl From<crate::utils::paths::PathError> for pi_ai::request::CallbackError {
    fn from(error: crate::utils::paths::PathError) -> Self {
        ConfigError::from(error).into()
    }
}
impl From<ConfigError> for pi_ai::request::CallbackError {
    fn from(error: ConfigError) -> Self {
        let callback = match error.native_error() {
            Some(native) => Self::from_rejection(native.clone().into()),
            None => Self::new(error.message().clone()),
        };
        callback.with_original(std::sync::Arc::new(error))
    }
}
impl From<crate::utils::paths::PathError> for ConfigError {
    fn from(error: crate::utils::paths::PathError) -> Self {
        match error.native_error() {
            Some(native) => Self::from(native.clone()),
            None => Self::new(error.message().clone()),
        }
    }
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
            Err(error) => {
                return Err(ConfigError::from(NativeNodeError::from(
                    pi_js_compat::node_fs::node_read_error_record(
                        &error,
                        &pi_js_compat::node_fs::decode_node_path(path.as_os_str()),
                        pi_js_compat::node_fs::NodeReadApi::ReadFileSync,
                    ),
                )));
            }
        };
        Self::parse(&content)
    }

    fn parse(content: &str) -> Result<Self, ConfigError> {
        let value = js_json::parse(content).map_err(|error| ConfigError::syntax(error.to_string()))?;
        Ok(Self::from_value(&value))
    }

    /// `pkg.name || default` etc.: only string fields are meaningful for the typed port.
    fn from_value(value: &Value) -> Self {
        let package = value.as_object();
        let field = |name: &str| package.and_then(|package| package.get(name)).and_then(Value::as_str);
        let pi_config = package
            .and_then(|package| package.get("piConfig"))
            .and_then(Value::as_object);
        let pi_config_field = |name: &str| pi_config.and_then(|config| config.get(name)).and_then(Value::as_str);
        let pi_config_name = pi_config_field("name").filter(|name| !name.is_empty());
        let app_name = pi_config_name.unwrap_or(DEFAULT_APP_NAME).to_owned();
        let app_title = if pi_config_name.is_some() {
            app_name.clone()
        } else {
            "π".to_owned()
        };
        Self {
            package_name: field("name")
                .filter(|name| !name.is_empty())
                .unwrap_or(DEFAULT_PACKAGE_NAME)
                .to_owned(),
            version: field("version")
                .filter(|version| !version.is_empty())
                .unwrap_or(DEFAULT_VERSION)
                .to_owned(),
            app_name,
            app_title,
            config_dir_name: pi_config_field("configDir")
                .filter(|directory| !directory.is_empty())
                .unwrap_or(DEFAULT_CONFIG_DIR_NAME)
                .to_owned(),
        }
    }

    fn defaults() -> Self {
        Self::from_value(&Value::Null)
    }
}

static PACKAGE_CONFIG: OnceLock<Result<PackageConfig, ConfigError>> = OnceLock::new();

/// The process-wide package constants, read once like the TypeScript module constants.
///
/// TypeScript throws during module evaluation when `PI_PACKAGE_DIR/package.json`
/// cannot be read or parsed; nothing catches it, so it reaches the process-level
/// handler.
/// `VERSION`: the embedded package version.
pub fn version() -> &'static str {
    &package_config().version
}

/// `CONFIG_DIR_NAME`: the embedded package config directory name.
pub fn config_dir_name() -> &'static str {
    &package_config().config_dir_name
}

pub fn package_config() -> &'static PackageConfig {
    try_package_config().unwrap_or_else(|error| throw_uncaught(error.label(), error.to_string()))
}

/// Fallible [`package_config`] for callers that can propagate the module-load failure.
pub fn try_package_config() -> Result<&'static PackageConfig, &'static ConfigError> {
    PACKAGE_CONFIG
        .get_or_init(|| match package_dir_override()? {
            Some(package_dir) => PackageConfig::load(&package_dir),
            None => PackageConfig::parse(EMBEDDED_PACKAGE_JSON),
        })
        .as_ref()
}

/// The built-in theme source compiled into the executable, used when `PI_PACKAGE_DIR` is unset.
pub fn embedded_builtin_theme(name: &str) -> Option<&'static str> {
    match name {
        "dark" => Some(EMBEDDED_DARK_THEME),
        "light" => Some(EMBEDDED_LIGHT_THEME),
        _ => None,
    }
}

fn default_path_options() -> PathInputOptions {
    PathInputOptions::default()
}

/// `normalizePath` for configuration paths that are only used as filesystem paths; its
/// `fileURLToPath` failure is thrown, not replaced by the raw input.
fn normalize(input: &str) -> Result<String, ConfigError> {
    normalize_path(input, &default_path_options())
        .map(|path| encode_node_utf8(&path))
        .map_err(ConfigError::from)
}

fn home_directory() -> Result<PathBuf, ConfigError> {
    pi_js_compat::node_os::homedir()
        .map(|home| PathBuf::from(encode_node_utf8(&home)))
        .map_err(ConfigError::from)
}

fn join(base: &str, segment: &str) -> String {
    PathBuf::from(base).join(segment).display().to_string()
}

// =============================================================================
// Package Asset Paths
// =============================================================================

/// The package directory: `PI_PACKAGE_DIR`, otherwise the executable's directory.
pub fn get_package_dir() -> Result<PathBuf, ConfigError> {
    if let Some(directory) = package_dir_override()? {
        return Ok(directory);
    }
    let executable = std::env::current_exe().map_err(|error| ConfigError::new(error.to_string()))?;
    executable
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| ConfigError::new("Executable path has no package directory"))
}

/// `PI_PACKAGE_DIR`, when set and non-empty; normalization errors remain catchable.
pub fn package_dir_override() -> Result<Option<PathBuf>, ConfigError> {
    pi_js_compat::process_env::get("PI_PACKAGE_DIR")
        .filter(|value| !value.is_empty())
        .map(|directory| {
            normalize_path(directory, &default_path_options())
                .map(|path| PathBuf::from(encode_node_utf8(&path)))
                .map_err(ConfigError::from)
        })
        .transpose()
}

/// Themes shipped next to the executable.
pub fn get_themes_dir() -> Result<PathBuf, ConfigError> {
    Ok(get_package_dir()?.join("theme"))
}

/// HTML export templates shipped next to the executable.
pub fn get_export_template_dir() -> Result<PathBuf, ConfigError> {
    Ok(get_package_dir()?.join("export-html"))
}

pub fn get_package_json_path() -> Result<PathBuf, ConfigError> {
    Ok(get_package_dir()?.join("package.json"))
}

pub fn get_readme_path() -> Result<PathBuf, ConfigError> {
    absolute(get_package_dir()?.join("README.md"))
}

pub fn get_docs_path() -> Result<PathBuf, ConfigError> {
    absolute(get_package_dir()?.join("docs"))
}

pub fn get_examples_path() -> Result<PathBuf, ConfigError> {
    absolute(get_package_dir()?.join("examples"))
}

pub fn get_changelog_path() -> Result<PathBuf, ConfigError> {
    absolute(get_package_dir()?.join("CHANGELOG.md"))
}

/// Interactive assets shipped next to the executable.
pub fn get_interactive_assets_dir() -> Result<PathBuf, ConfigError> {
    Ok(get_package_dir()?.join("assets"))
}

pub fn get_bundled_interactive_asset_path(name: &str) -> Result<PathBuf, ConfigError> {
    Ok(get_interactive_assets_dir()?.join(name))
}

fn absolute(path: PathBuf) -> Result<PathBuf, ConfigError> {
    let path = pi_js_compat::node_fs::decode_node_path(path.as_os_str());
    node_resolve("", path)
        .map(|path| PathBuf::from(encode_node_utf8(&path)))
        .map_err(ConfigError::from)
}

pub fn expand_tilde_path(path: &str) -> Result<String, ConfigError> {
    normalize(path)
}

/// Share viewer URL for a gist id; `PI_SHARE_VIEWER_URL` overrides the base.
pub fn get_share_viewer_url(gist_id: &str) -> String {
    let base_url = pi_js_compat::process_env::get("PI_SHARE_VIEWER_URL")
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
    pi_js_compat::process_env::get(name).filter(|value| !value.is_empty())
}

/// The XDG config root for Pi (`~/.config/pi/`).
pub fn get_user_config_root() -> Result<PathBuf, ConfigError> {
    let config_home = match environment_value("XDG_CONFIG_HOME") {
        Some(directory) => expand_tilde_path(&directory)?,
        None => join(&home_directory()?.display().to_string(), ".config"),
    };
    Ok(PathBuf::from(join(&config_home, &package_config().app_name)))
}

/// The XDG state root for Pi (`~/.local/state/pi/`).
pub fn get_user_state_root() -> Result<PathBuf, ConfigError> {
    if let Some(directory) = environment_value(&env_state_dir()) {
        return Ok(PathBuf::from(expand_tilde_path(&directory)?));
    }
    let state_home = match environment_value("XDG_STATE_HOME") {
        Some(directory) => expand_tilde_path(&directory)?,
        None => join(&join(&home_directory()?.display().to_string(), ".local"), "state"),
    };
    Ok(PathBuf::from(join(&state_home, &package_config().app_name)))
}

/// The legacy agent directory (`~/.pi/agent/`).
pub fn get_legacy_agent_dir() -> Result<PathBuf, ConfigError> {
    Ok(PathBuf::from(join(
        &home_directory()?.display().to_string(),
        &package_config().config_dir_name,
    ))
    .join("agent"))
}

/// The agent directory (`~/.config/pi/agent/`).
pub fn get_agent_dir() -> Result<PathBuf, ConfigError> {
    if let Some(directory) = environment_value(&env_agent_dir()) {
        return Ok(PathBuf::from(expand_tilde_path(&directory)?));
    }
    Ok(get_user_config_root()?.join("agent"))
}

pub fn get_custom_themes_dir() -> Result<PathBuf, ConfigError> {
    Ok(get_agent_dir()?.join("themes"))
}
pub fn get_models_path() -> Result<PathBuf, ConfigError> {
    Ok(get_agent_dir()?.join("models.json"))
}
pub fn get_auth_path() -> Result<PathBuf, ConfigError> {
    Ok(get_agent_dir()?.join("auth.json"))
}
pub fn get_settings_path() -> Result<PathBuf, ConfigError> {
    Ok(get_agent_dir()?.join("settings.json"))
}
pub fn get_tools_dir() -> Result<PathBuf, ConfigError> {
    Ok(get_agent_dir()?.join("tools"))
}
/// Managed binaries directory (fd, rg).
pub fn get_bin_dir() -> Result<PathBuf, ConfigError> {
    Ok(get_agent_dir()?.join("bin"))
}
pub fn get_prompts_dir() -> Result<PathBuf, ConfigError> {
    Ok(get_agent_dir()?.join("prompts"))
}
pub fn get_prompt_history_path() -> Result<PathBuf, ConfigError> {
    Ok(get_agent_dir()?.join("prompt-history.json"))
}
pub fn get_sessions_dir() -> Result<PathBuf, ConfigError> {
    Ok(get_agent_dir()?.join("sessions"))
}
pub fn get_debug_log_path() -> Result<PathBuf, ConfigError> {
    Ok(get_agent_dir()?.join(format!("{}-debug.log", package_config().app_name)))
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
    pub command: JsString,
    pub args: Vec<JsString>,
    pub display: JsString,
}

/// A self-update command, with `steps` when the package name changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfUpdateCommand {
    pub command: JsString,
    pub args: Vec<JsString>,
    pub display: JsString,
    pub steps: Option<Vec<SelfUpdateCommandStep>>,
}

/// `SelfUpdatePackageTarget`: a package name, optionally with an exact install spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfUpdatePackageTarget {
    pub package_name: JsString,
    pub install_spec: Option<JsString>,
}

impl SelfUpdatePackageTarget {
    pub fn new(package_name: impl Into<JsString>) -> Self {
        Self {
            package_name: package_name.into(),
            install_spec: None,
        }
    }

    pub fn with_install_spec(package_name: impl Into<JsString>, install_spec: impl Into<JsString>) -> Self {
        Self {
            package_name: package_name.into(),
            install_spec: Some(install_spec.into()),
        }
    }

    /// `installSpec ?? packageName`.
    fn install_spec(&self) -> &JsString {
        self.install_spec.as_ref().unwrap_or(&self.package_name)
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
    pub fn for_process() -> Result<Self, ConfigError> {
        let env = pi_js_compat::process_env::vars().into_iter().collect();
        Ok(Self {
            package_dir: get_package_dir()?,
            exec_path: std::env::current_exe()
                .map(|executable| executable.display().to_string())
                .unwrap_or_default(),
            entrypoint: std::env::args_os().nth(1).map(|arg| arg.to_string_lossy().into_owned()),
            is_bun_binary: true,
            is_bun_runtime: false,
            platform: crate::utils::pi_user_agent::node_platform_name(std::env::consts::OS).to_owned(),
            home: home_directory()?,
            path_env: None,
            env,
        })
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
pub fn get_update_instruction(context: &InstallContext, package_name: &JsString) -> Result<JsString, ConfigError> {
    let method = detect_install_method(context);
    let target = SelfUpdatePackageTarget::new(package_name.clone());
    match get_self_update_command_for_method(context, method, package_name, &target, None) {
        Some(command) => Ok(js_format!("Run: {}", command.display)),
        None => get_self_update_unavailable_instruction(context, package_name, None, None),
    }
}

/// `getSelfUpdateUnavailableInstruction`.
pub fn get_self_update_unavailable_instruction(
    context: &InstallContext,
    package_name: &JsString,
    npm_command: Option<&[JsString]>,
    target: Option<&SelfUpdatePackageTarget>,
) -> Result<JsString, ConfigError> {
    let method = detect_install_method(context);
    let target = target
        .cloned()
        .unwrap_or_else(|| SelfUpdatePackageTarget::new(package_name.clone()));
    if method == InstallMethod::BunBinary {
        return Ok(js_format!("Download from: {}", RELEASES_URL));
    }
    let Some(command) = get_self_update_command_for_method(context, method, package_name, &target, npm_command) else {
        return Ok(js_format!(
            "Update {} using the package manager, wrapper, or source checkout that provides this installation.",
            target.install_spec()
        ));
    };
    Ok(
        if is_managed_by_global_package_manager(context, method, npm_command)? && !is_self_update_path_writable(context)
        {
            js_format!(
                "This installation is managed by a global {} install, but the install path is not writable. Update it yourself with: {}",
                method.as_str(),
                command.display
            )
        } else {
            js_format!(
                "This installation is not managed by a global {} install. Update it with the package manager, wrapper, or source checkout that provides it.",
                method.as_str()
            )
        },
    )
}

/// `getSelfUpdateCommand`: a command only when the install is managed by a global package manager and writable.
pub fn get_self_update_command(
    context: &InstallContext,
    package_name: &JsString,
    npm_command: Option<&[JsString]>,
    target: Option<&SelfUpdatePackageTarget>,
) -> Result<Option<SelfUpdateCommand>, ConfigError> {
    let method = detect_install_method(context);
    let target = target
        .cloned()
        .unwrap_or_else(|| SelfUpdatePackageTarget::new(package_name.clone()));
    let Some(command) = get_self_update_command_for_method(context, method, package_name, &target, npm_command) else {
        return Ok(None);
    };
    if !is_managed_by_global_package_manager(context, method, npm_command)? || !is_self_update_path_writable(context) {
        return Ok(None);
    }
    Ok(Some(command))
}

fn get_self_update_command_for_method(
    context: &InstallContext,
    method: InstallMethod,
    installed_package_name: &JsString,
    target: &SelfUpdatePackageTarget,
    npm_command: Option<&[JsString]>,
) -> Option<SelfUpdateCommand> {
    let renamed = target.package_name != *installed_package_name;
    let install_spec = target.install_spec().clone();
    let installed = installed_package_name.clone();
    let (install_step, uninstall_step) = match method {
        InstallMethod::BunBinary | InstallMethod::Unknown => return None,
        InstallMethod::Pnpm => {
            let bin_dir_args = pnpm_bin_dir_args(context);
            let install_args = [
                js_strings(&["install", "-g", "--ignore-scripts", "--config.minimumReleaseAge=0"]),
                bin_dir_args.clone(),
                vec![install_spec],
            ];
            let uninstall_args = [js_strings(&["remove", "-g"]), bin_dir_args, vec![installed]];
            (
                make_step("pnpm".into(), install_args.concat()),
                renamed.then(|| make_step("pnpm".into(), uninstall_args.concat())),
            )
        }
        InstallMethod::Yarn => (
            make_step(
                "yarn".into(),
                [js_strings(&["global", "add", "--ignore-scripts"]), vec![install_spec]].concat(),
            ),
            renamed.then(|| {
                make_step(
                    "yarn".into(),
                    [js_strings(&["global", "remove"]), vec![installed]].concat(),
                )
            }),
        ),
        InstallMethod::Bun => (
            make_step(
                "bun".into(),
                [
                    js_strings(&["install", "-g", "--ignore-scripts", "--minimum-release-age=0"]),
                    vec![install_spec],
                ]
                .concat(),
            ),
            renamed.then(|| {
                make_step(
                    "bun".into(),
                    [js_strings(&["uninstall", "-g"]), vec![installed]].concat(),
                )
            }),
        ),
        InstallMethod::Npm => {
            let (command, npm_args) = split_npm_command(npm_command);
            let inferred = if npm_command.is_some_and(|args| !args.is_empty()) {
                None
            } else {
                get_inferred_npm_install(context)
            };
            let mut prefix_args = npm_args;
            if let Some((_, prefix)) = inferred {
                prefix_args.extend(["--prefix".into(), JsString::from(prefix)]);
            }
            let install_args = [
                prefix_args.clone(),
                js_strings(&["install", "-g", "--ignore-scripts", "--min-release-age=0"]),
                vec![install_spec],
            ];
            let uninstall_args = [prefix_args, js_strings(&["uninstall", "-g"]), vec![installed]];
            (
                make_step(command.clone(), install_args.concat()),
                renamed.then(|| make_step(command, uninstall_args.concat())),
            )
        }
    };
    Some(make_command(install_step, uninstall_step))
}

fn js_strings(values: &[&str]) -> Vec<JsString> {
    values.iter().map(|value| JsString::from(*value)).collect()
}

/// `const [command = "npm", ...npmArgs] = npmCommand ?? []`.
fn split_npm_command(npm_command: Option<&[JsString]>) -> (JsString, Vec<JsString>) {
    match npm_command.and_then(|args| args.split_first()) {
        Some((command, rest)) => (command.clone(), rest.to_vec()),
        None => ("npm".into(), Vec::new()),
    }
}

/// `--config.global-bin-dir=...` when `pnpm root -g` fails and the package sits under a pnpm global store.
fn pnpm_bin_dir_args(context: &InstallContext) -> Vec<JsString> {
    let pnpm_root = read_command_output(context, &"pnpm".into(), &js_strings(&["root", "-g"]), false);
    if matches!(pnpm_root, Ok(Some(_))) {
        return Vec::new();
    }
    let Some(path) = pnpm_global_path(&context.package_dir_string()) else {
        return Vec::new();
    };
    let home = match context.environment("PNPM_HOME") {
        Some(home) => JsString::from(home),
        None => node_dirname(node_dirname(path)),
    };
    vec![js_format!("--config.global-bin-dir={}", home)]
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
            display: js_format!("{} && {}", uninstall.display, install_step.display),
            command: install_step.command.clone(),
            args: install_step.args.clone(),
            steps: Some(vec![uninstall, install_step]),
        },
    }
}

/// `[command, ...args].map((arg) => (/\s/.test(arg) ? `"${arg}"` : arg)).join(" ")`.
fn make_step(command: JsString, args: Vec<JsString>) -> SelfUpdateCommandStep {
    let display = std::iter::once(&command)
        .chain(&args)
        .map(|argument| {
            if argument.units().iter().copied().any(is_js_whitespace_unit) {
                js_format!("\"{}\"", argument)
            } else {
                argument.clone()
            }
        })
        .collect::<Vec<_>>();
    SelfUpdateCommandStep {
        command,
        args,
        display: JsString::join(&display, " "),
    }
}

/// `readCommandOutput`: package-manager probes inherit the install context's `PATH`.
/// Node encodes argv to UTF-8 at the spawn boundary and decodes stdout as UTF-8.
fn read_command_output(
    context: &InstallContext,
    command: &JsString,
    args: &[JsString],
    require_success: bool,
) -> Result<Option<JsString>, ConfigError> {
    let mut builder = std::process::Command::new(encode_node_utf8(command));
    builder
        .args(args.iter().map(encode_node_utf8))
        .stdin(std::process::Stdio::null());
    if let Some(path) = &context.path_env {
        builder.env("PATH", path);
    }
    let output = match builder.output() {
        Ok(output) => output,
        Err(error) if require_success => {
            let reason = node_spawn_error_message("spawnSync", command, &error);
            return Err(command_failure(command, args, reason));
        }
        Err(_) => return Ok(None),
    };
    if output.status.success() {
        let stdout = JsString::from(String::from_utf8_lossy(&output.stdout).as_ref()).trim();
        return Ok((!stdout.is_empty()).then_some(stdout));
    }
    if !require_success {
        return Ok(None);
    }
    let stderr = JsString::from(String::from_utf8_lossy(&output.stderr).as_ref()).trim();
    let reason = if !stderr.is_empty() {
        stderr
    } else {
        match output.status.code() {
            Some(code) => js_format!("exit code {}", code),
            None => "exit code unknown".into(),
        }
    };
    Err(command_failure(command, args, reason))
}

/// ``Failed to run ${[command, ...args].join(" ")}: ${reason}``.
fn command_failure(command: &JsString, args: &[JsString], reason: JsString) -> ConfigError {
    let words = [std::slice::from_ref(command), args].concat();
    ConfigError::new(js_format!("Failed to run {}: {}", JsString::join(&words, " "), reason))
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

/// `join(homedir(), ".bun", "install", "global", "node_modules")` plus the root beside `bun pm bin -g`.
fn bun_global_package_roots(
    context: &InstallContext,
    command: &JsString,
    npm_args: Vec<JsString>,
    require_success: bool,
) -> Result<Vec<JsString>, ConfigError> {
    let bun_bin = read_command_output(
        context,
        command,
        &[npm_args, js_strings(&["pm", "bin", "-g"])].concat(),
        require_success,
    )?;
    let mut roots = vec![node_join(
        context.home.display().to_string(),
        ".bun/install/global/node_modules",
    )];
    if let Some(bun_bin) = bun_bin {
        roots.push(node_join(node_dirname(bun_bin), "install/global/node_modules"));
    }
    Ok(roots)
}

fn npm_global_package_roots(
    context: &InstallContext,
    npm_command: Option<&[JsString]>,
) -> Result<Vec<JsString>, ConfigError> {
    let configured = npm_command.is_some_and(|args| !args.is_empty());
    let (command, npm_args) = split_npm_command(npm_command);
    if configured && command == "bun" {
        return bun_global_package_roots(context, &command, npm_args, true);
    }
    let root = read_command_output(
        context,
        &command,
        &[npm_args, js_strings(&["root", "-g"])].concat(),
        configured,
    )?;
    let inferred = if configured {
        None
    } else {
        get_inferred_npm_install(context)
    };
    Ok([root, inferred.map(|(root, _)| JsString::from(root))]
        .into_iter()
        .flatten()
        .collect())
}

fn get_global_package_roots(
    context: &InstallContext,
    method: InstallMethod,
    npm_command: Option<&[JsString]>,
) -> Result<Vec<JsString>, ConfigError> {
    match method {
        InstallMethod::Npm => npm_global_package_roots(context, npm_command),
        InstallMethod::Pnpm => {
            if let Some(root) = read_command_output(context, &"pnpm".into(), &js_strings(&["root", "-g"]), false)? {
                return Ok(vec![root.clone(), node_dirname(root)]);
            }
            Ok(pnpm_global_path(&context.package_dir_string())
                .map(JsString::from)
                .into_iter()
                .collect())
        }
        InstallMethod::Yarn => {
            let directory = read_command_output(context, &"yarn".into(), &js_strings(&["global", "dir"]), false)?;
            Ok(directory
                .map(|directory| vec![directory.clone(), node_join(directory, "node_modules")])
                .unwrap_or_default())
        }
        InstallMethod::Bun => bun_global_package_roots(context, &"bun".into(), Vec::new(), false),
        InstallMethod::BunBinary | InstallMethod::Unknown => Ok(Vec::new()),
    }
}

/// `isManagedByGlobalPackageManager`: the package directory lives under a global package root.
/// A required package-manager probe failure throws, as in the source.
fn is_managed_by_global_package_manager(
    context: &InstallContext,
    method: InstallMethod,
    npm_command: Option<&[JsString]>,
) -> Result<bool, ConfigError> {
    let package_dirs = [Some(context.package_dir_string()), get_entrypoint_package_dir(context)];
    let mut package_dir_candidates = Vec::new();
    for directory in package_dirs.into_iter().flatten() {
        package_dir_candidates.extend(path_comparison_candidates(&JsString::from(directory))?);
    }
    let separator = JsString::from(std::path::MAIN_SEPARATOR);
    for root in get_global_package_roots(context, method, npm_command)? {
        for normalized_root in path_comparison_candidates(&root)? {
            let root_prefix = if normalized_root.ends_with(separator.clone()) {
                normalized_root
            } else {
                normalized_root + separator.clone()
            };
            if package_dir_candidates
                .iter()
                .any(|package_dir| package_dir.starts_with(root_prefix.clone()))
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn path_comparison_candidates(path: &JsString) -> Result<Vec<JsString>, ConfigError> {
    let mut candidates = Vec::new();
    for resolve_symlinks in [false, true] {
        if let Some(candidate) = normalize_existing_path_for_comparison(path, resolve_symlinks)?
            && !candidates.contains(&candidate)
        {
            candidates.push(candidate);
        }
    }
    Ok(candidates)
}

/// `resolve(path)` (which reads `process.cwd()` and throws when it is gone), then `realpathSync`.
fn normalize_existing_path_for_comparison(
    path: &JsString,
    resolve_symlinks: bool,
) -> Result<Option<JsString>, ConfigError> {
    let resolved = process_cwd()
        .and_then(|cwd| node_resolve(cwd, path.clone()))
        .map_err(|error| ConfigError::new(error.message().clone()))?;
    let fs_path = encode_node_utf8(&resolved);
    if !Path::new(&fs_path).exists() {
        return Ok(None);
    }
    let mut normalized = resolved;
    if resolve_symlinks {
        let Ok(real) = std::fs::canonicalize(&fs_path) else {
            return Ok(None);
        };
        normalized = JsString::from(real.to_string_lossy().as_ref());
    }
    if cfg!(windows) {
        normalized = normalized.to_lowercase();
    }
    Ok(Some(normalized))
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
