//! Settings storage and typed accessors (`core/settings-manager.ts`).
//!
//! Setters update effective settings immediately and enqueue immutable scoped write snapshots.
//! Shared mutation guards schedule a FIFO drain on release; `flush` synchronously settles earlier writes.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pi_agent_core::harness::ThinkingLevel;
use pi_ai::types::Transport;
use pi_ai::utils::random_uuid::random_uuid;
use pi_js_compat::js_json::{JsObject, JsString, JsValue as Value};
use pi_js_compat::number as js_number;
use serde::{Deserialize, Serialize};

use crate::config::{get_agent_dir, package_config};
use crate::core::http_dispatcher::{DEFAULT_HTTP_IDLE_TIMEOUT_MS, HttpIdleTimeout, parse_http_idle_timeout_ms};
use crate::core::permissions::policy::{ApprovalPolicy, normalize_approval_policy};
use crate::core::permissions::presets::{
    ApprovalPresetName, SandboxProfileName, approval_preset_for_policy, find_approval_preset, is_approval_preset_name,
    is_sandbox_profile_name,
};
use crate::utils::lockfile::{LockErrorCode, LockOptions, acquire_sync};
use crate::utils::paths::{PathInputOptions, normalize_path, resolve_path};
use pi_js_compat::js_json;

const SETTINGS_LOCK_ATTEMPTS: usize = 10;
const SETTINGS_LOCK_DELAY: Duration = Duration::from_millis(20);

/// A settings document. Insertion order is preserved for byte-identical files.
pub type Settings = JsObject;

/// The settings handle owned by `AgentSession` and shared with the interactive mode.
pub type SharedSettingsManager = Arc<Mutex<SettingsManager>>;

/// A shared mutation boundary. Releasing it schedules pending persistence after unlocking.
pub struct SettingsManagerGuard<'a> {
    handle: &'a SharedSettingsManager,
    guard: Option<std::sync::MutexGuard<'a, SettingsManager>>,
}

impl std::ops::Deref for SettingsManagerGuard<'_> {
    type Target = SettingsManager;

    fn deref(&self) -> &Self::Target {
        self.guard.as_deref().expect("settings guard held until drop")
    }
}

impl std::ops::DerefMut for SettingsManagerGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.guard.as_deref_mut().expect("settings guard held until drop")
    }
}

impl Drop for SettingsManagerGuard<'_> {
    fn drop(&mut self) {
        if self.pending_writes.is_empty() || self.drain_scheduled {
            return;
        }
        self.drain_scheduled = true;
        let scope = self.pending_writes[0].scope;
        drop(self.guard.take());
        spawn_settings_drain(self.handle, scope);
    }
}

/// Resets scheduling while still holding the mutex, including during worker unwinding.
struct SettingsDrainGuard<'a> {
    manager: std::sync::MutexGuard<'a, SettingsManager>,
}

impl Drop for SettingsDrainGuard<'_> {
    fn drop(&mut self) {
        self.manager.drain_scheduled = false;
    }
}

fn spawn_settings_drain(handle: &SharedSettingsManager, scope: SettingsScope) {
    let worker_handle = Arc::clone(handle);
    // Synchronous callers need no Tokio runtime. The task retains the manager until drained.
    let result = std::thread::Builder::new().spawn(move || {
        let manager = worker_handle.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut drain = SettingsDrainGuard { manager };
        drain.manager.flush();
    });
    if let Err(error) = result {
        let mut manager = handle.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        manager.drain_scheduled = false;
        manager.record_error(scope, format!("Failed to spawn settings drain worker: {error}"));
    }
}

/// Locks shared settings, recovering poisoning and scheduling queued writes when released.
pub fn lock_settings_manager(handle: &SharedSettingsManager) -> SettingsManagerGuard<'_> {
    SettingsManagerGuard {
        handle,
        guard: Some(handle.lock().unwrap_or_else(std::sync::PoisonError::into_inner)),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingsScope {
    Global,
    Project,
}

impl SettingsScope {
    /// Returns the exact TypeScript source literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Project => "project",
        }
    }
}

/// One recorded load or write failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsError {
    pub scope: SettingsScope,
    pub message: JsString,
}

/// A synchronous settings operation failure (thrown by the TypeScript source).
#[derive(Debug, Clone, PartialEq)]
pub struct SettingsFailure {
    message: JsString,
    config: Option<Box<crate::config::ConfigError>>,
}

impl SettingsFailure {
    /// Failure reported by a [`SettingsStorage`] implementation.
    pub fn new(message: impl Into<JsString>) -> Self {
        Self {
            message: message.into(),
            config: None,
        }
    }

    pub fn message(&self) -> &JsString {
        &self.message
    }
}

impl From<SettingsFailure> for pi_ai::request::CallbackError {
    fn from(error: SettingsFailure) -> Self {
        Self::from_error(error.message().clone(), error)
    }
}

impl std::fmt::Display for SettingsFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message.to_utf8_lossy())
    }
}

impl std::error::Error for SettingsFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.config.as_deref().map(|error| error as &dyn std::error::Error)
    }
}
impl From<crate::config::ConfigError> for SettingsFailure {
    fn from(error: crate::config::ConfigError) -> Self {
        Self {
            message: error.message().clone(),
            config: Some(Box::new(error)),
        }
    }
}

/// A storage backend addressed by settings scope.
pub trait SettingsStorage: Send {
    /// Runs `apply` under the scope's lock and writes the returned text when present.
    fn with_lock(
        &mut self,
        scope: SettingsScope,
        apply: &mut dyn FnMut(Option<&str>) -> Option<String>,
    ) -> Result<(), SettingsFailure>;
}

/// Global `<agentDir>/settings.json` and project `<cwd>/.pi/settings.json`.
#[derive(Debug, Clone)]
pub struct FileSettingsStorage {
    global_settings_path: pi_js_compat::js_json::JsString,
    project_settings_path: pi_js_compat::js_json::JsString,
}

impl FileSettingsStorage {
    pub fn new(
        cwd: &pi_js_compat::js_json::JsString,
        agent_dir: &pi_js_compat::js_json::JsString,
    ) -> Result<Self, SettingsFailure> {
        let process_cwd = pi_js_compat::process_cwd::cwd()
            .map_err(crate::config::ConfigError::from)
            .map_err(SettingsFailure::from)?;
        let resolved_cwd = resolve_path(cwd, &process_cwd, &PathInputOptions::default())
            .map_err(crate::config::ConfigError::from)
            .map_err(SettingsFailure::from)?;
        let resolved_agent_dir = resolve_path(agent_dir, &process_cwd, &PathInputOptions::default())
            .map_err(crate::config::ConfigError::from)
            .map_err(SettingsFailure::from)?;
        Ok(Self {
            global_settings_path: crate::utils::paths::node_join(&resolved_agent_dir, "settings.json"),
            project_settings_path: crate::utils::paths::node_join(
                crate::utils::paths::node_join(&resolved_cwd, &package_config().config_dir_name),
                "settings.json",
            ),
        })
    }

    fn acquire_lock_sync_with_retry(path: &Path) -> Result<crate::utils::lockfile::LockGuard, SettingsFailure> {
        let options = LockOptions {
            realpath: false,
            ..LockOptions::default()
        };
        let mut last_error = None;
        for attempt in 1..=SETTINGS_LOCK_ATTEMPTS {
            match acquire_sync(path, &options) {
                Ok(guard) => return Ok(guard),
                Err(error) => {
                    if error.code != LockErrorCode::Locked || attempt == SETTINGS_LOCK_ATTEMPTS {
                        return Err(SettingsFailure::new(error.message));
                    }
                    last_error = Some(error.message);
                    std::thread::sleep(SETTINGS_LOCK_DELAY);
                }
            }
        }
        Err(SettingsFailure::new(
            last_error.unwrap_or_else(|| "Failed to acquire settings lock".to_owned()),
        ))
    }
}

impl SettingsStorage for FileSettingsStorage {
    fn with_lock(
        &mut self,
        scope: SettingsScope,
        apply: &mut dyn FnMut(Option<&str>) -> Option<String>,
    ) -> Result<(), SettingsFailure> {
        let path = match scope {
            SettingsScope::Global => &self.global_settings_path,
            SettingsScope::Project => &self.project_settings_path,
        };
        let source_path = path;
        let encoded = pi_js_compat::js_json::encode_node_utf8(path);
        let path = Path::new(&encoded);
        let dir = path.parent().map_or_else(|| PathBuf::from("."), Path::to_path_buf);

        let file_exists = path.exists();
        let mut release = if file_exists {
            Some(Self::acquire_lock_sync_with_retry(path)?)
        } else {
            None
        };

        let current =
            if file_exists {
                Some(fs::read_to_string(path).map_err(|error| {
                    SettingsFailure::new(pi_js_compat::node_fs::node_read_error(&error, source_path))
                })?)
            } else {
                None
            };
        let next = apply(current.as_deref());
        if let Some(next) = next {
            if !dir.exists() {
                fs::create_dir_all(&dir).map_err(|error| SettingsFailure::new(error.to_string()))?;
            }
            if release.is_none() {
                release = Some(Self::acquire_lock_sync_with_retry(path)?);
            }
            fs::write(path, next).map_err(|error| {
                SettingsFailure::new(pi_js_compat::node_fs::node_fs_error(&error, "open", source_path))
            })?;
        }
        drop(release);
        Ok(())
    }
}

/// A storage backend that keeps both scopes in memory.
#[derive(Debug, Clone, Default)]
pub struct InMemorySettingsStorage {
    global: Option<String>,
    project: Option<String>,
}

impl InMemorySettingsStorage {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, scope: SettingsScope, content: Option<String>) {
        match scope {
            SettingsScope::Global => self.global = content,
            SettingsScope::Project => self.project = content,
        }
    }

    pub fn get(&self, scope: SettingsScope) -> Option<&str> {
        match scope {
            SettingsScope::Global => self.global.as_deref(),
            SettingsScope::Project => self.project.as_deref(),
        }
    }
}

impl SettingsStorage for InMemorySettingsStorage {
    fn with_lock(
        &mut self,
        scope: SettingsScope,
        apply: &mut dyn FnMut(Option<&str>) -> Option<String>,
    ) -> Result<(), SettingsFailure> {
        let current = self.get(scope).map(str::to_owned);
        let next = apply(current.as_deref());
        if let Some(next) = next {
            self.set(scope, Some(next));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SettingsManagerCreateOptions {
    pub project_trusted: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactionSettings {
    pub threshold_percent: Option<f64>,
    pub idle: Option<bool>,
    pub enabled: Option<bool>,
    pub reserve_tokens: Option<f64>,
    pub keep_recent_tokens: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchSummarySettings {
    pub reserve_tokens: Option<f64>,
    pub skip_prompt: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRetrySettings {
    pub timeout_ms: Option<f64>,
    pub max_retries: Option<f64>,
    pub max_retry_delay_ms: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrySettings {
    pub enabled: Option<bool>,
    pub max_retries: Option<f64>,
    pub base_delay_ms: Option<f64>,
    pub max_backoff_ms: Option<f64>,
    pub provider: Option<ProviderRetrySettings>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSettings {
    pub show_images: Option<bool>,
    pub image_width_cells: Option<f64>,
    pub clear_on_shrink: Option<bool>,
    pub show_terminal_progress: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageSettings {
    pub auto_resize: Option<bool>,
    pub block_images: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThinkingBudgetsSettings {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "js_number::serialize_option"
    )]
    pub minimal: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "js_number::serialize_option"
    )]
    pub low: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "js_number::serialize_option"
    )]
    pub medium: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "js_number::serialize_option"
    )]
    pub high: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkdownSettings {
    pub code_block_indent: Option<JsString>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentProfileContext {
    Fresh,
    Inherit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentProfileThinkingLevel {
    Off,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
    Ultra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentProfileSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<AgentProfileContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<JsString>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<AgentProfileThinkingLevel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<JsString>>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PackageFilter {
    pub source: JsString,
    pub extensions: Option<Vec<JsString>>,
    pub skills: Option<Vec<JsString>>,
    pub prompts: Option<Vec<JsString>>,
    pub themes: Option<Vec<JsString>>,
    /// Unknown lossless properties and original insertion order; typed fields own known values.
    pub properties: pi_ai::types::ModelProperties,
}

impl Serialize for PackageFilter {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut fields = JsObject::new();
        fields.insert("source".into(), Value::String(self.source.clone()));
        for (name, values) in [
            ("extensions", &self.extensions),
            ("skills", &self.skills),
            ("prompts", &self.prompts),
            ("themes", &self.themes),
        ] {
            if let Some(values) = values {
                fields.insert(name.into(), strings_to_value(values));
            }
        }
        self.properties.serialize(&Value::Object(fields), serializer)
    }
}

impl<'de> Deserialize<'de> for PackageFilter {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let (fields, properties): (JsObject, _) = pi_ai::types::ModelProperties::decode(
            Value::deserialize(deserializer)?,
            &["source", "extensions", "skills", "prompts", "themes"],
        )
        .map_err(D::Error::custom)?;
        let optional = |name: &str| fields.get(name).cloned().unwrap_or(Value::Null);
        Ok(Self {
            source: js_json::from_value(fields.get("source").ok_or_else(|| D::Error::missing_field("source"))?)
                .map_err(D::Error::custom)?,
            extensions: js_json::from_value(&optional("extensions")).map_err(D::Error::custom)?,
            skills: js_json::from_value(&optional("skills")).map_err(D::Error::custom)?,
            prompts: js_json::from_value(&optional("prompts")).map_err(D::Error::custom)?,
            themes: js_json::from_value(&optional("themes")).map_err(D::Error::custom)?,
            properties,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum PackageSource {
    Name(JsString),
    Filter(PackageFilter),
}

const PACKAGE_SOURCE_MISMATCH: &str = "data did not match any variant of untagged enum PackageSource";

/// Lossless JSON cannot pass through serde's buffered untagged content, so the variant is
/// chosen from the parsed value (same acceptance and message as the derived untagged enum).
impl<'de> Deserialize<'de> for PackageSource {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        package_source_from_value(&Value::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

fn package_source_from_value(value: &Value) -> Result<PackageSource, &'static str> {
    match value {
        Value::String(name) => Ok(PackageSource::Name(name.clone())),
        Value::Object(_) => js_json::from_value(value)
            .map(PackageSource::Filter)
            .map_err(|_| PACKAGE_SOURCE_MISMATCH),
        _ => Err(PACKAGE_SOURCE_MISMATCH),
    }
}

impl PackageSource {
    pub fn source(&self) -> &JsString {
        match self {
            Self::Name(source) => source,
            Self::Filter(filter) => &filter.source,
        }
    }

    pub fn filter(&self) -> Option<&PackageFilter> {
        match self {
            Self::Name(_) => None,
            Self::Filter(filter) => Some(filter),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DefaultProjectTrust {
    Ask,
    Always,
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoubleEscapeAction {
    Fork,
    Tree,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeFilterMode {
    Default,
    NoTools,
    UserOnly,
    LabeledOnly,
    All,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompactionSettingsView {
    pub threshold_percent: Option<f64>,
    pub enabled: bool,
    pub reserve_tokens: f64,
    pub keep_recent_tokens: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BranchSummarySettingsView {
    pub reserve_tokens: f64,
    pub skip_prompt: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RetrySettingsView {
    pub enabled: bool,
    pub max_retries: f64,
    pub base_delay_ms: f64,
    pub max_backoff_ms: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProviderRetrySettingsView {
    pub timeout_ms: Option<f64>,
    pub max_retries: Option<f64>,
    pub max_retry_delay_ms: f64,
}

struct PendingSettingsWrite {
    scope: SettingsScope,
    settings: Settings,
    modified_fields: Vec<String>,
    modified_nested_fields: Vec<(String, Vec<String>)>,
}

/// Loaded global and project settings with typed accessors.
pub struct SettingsManager {
    storage: Box<dyn SettingsStorage>,
    global_settings: Settings,
    project_settings: Settings,
    session_sandbox_profile: Option<SandboxProfileName>,
    settings: Settings,
    project_trusted: bool,
    modified_fields: Vec<String>,
    modified_nested_fields: Vec<(String, Vec<String>)>,
    modified_project_fields: Vec<String>,
    modified_project_nested_fields: Vec<(String, Vec<String>)>,
    global_settings_load_error: Option<JsString>,
    project_settings_load_error: Option<JsString>,
    errors: Vec<SettingsError>,
    pending_writes: Vec<PendingSettingsWrite>,
    drain_scheduled: bool,
}

impl std::fmt::Debug for SettingsManager {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SettingsManager")
            .field("global_settings", &self.global_settings)
            .field("project_settings", &self.project_settings)
            .field("project_trusted", &self.project_trusted)
            .finish()
    }
}

impl SettingsManager {
    /// Creates a manager that loads `<agentDir>/settings.json` and `<cwd>/.pi/settings.json`.
    pub fn create(
        cwd: &pi_js_compat::js_json::JsString,
        agent_dir: Option<&pi_js_compat::js_json::JsString>,
        options: SettingsManagerCreateOptions,
    ) -> Result<Self, SettingsFailure> {
        let agent_dir = match agent_dir {
            Some(directory) => directory.clone(),
            None => get_agent_dir()?,
        };
        let storage = FileSettingsStorage::new(cwd, &agent_dir)?;
        Ok(Self::from_storage(Box::new(storage), options))
    }

    /// Creates a manager from an arbitrary storage backend.
    pub fn from_storage(storage: Box<dyn SettingsStorage>, options: SettingsManagerCreateOptions) -> Self {
        let project_trusted = options.project_trusted.unwrap_or(true);
        let mut storage = storage;
        let global_load = Self::try_load_from_storage(storage.as_mut(), SettingsScope::Global, true);
        let project_load = Self::try_load_from_storage(storage.as_mut(), SettingsScope::Project, project_trusted);
        let mut errors = Vec::new();
        if let Some(error) = &global_load.error {
            errors.push(SettingsError {
                scope: SettingsScope::Global,
                message: error.clone(),
            });
        }
        if let Some(error) = &project_load.error {
            errors.push(SettingsError {
                scope: SettingsScope::Project,
                message: error.clone(),
            });
        }

        let mut manager = Self {
            storage,
            global_settings: Settings::new(),
            project_settings: Settings::new(),
            session_sandbox_profile: None,
            settings: Settings::new(),
            project_trusted,
            modified_fields: Vec::new(),
            modified_nested_fields: Vec::new(),
            modified_project_fields: Vec::new(),
            modified_project_nested_fields: Vec::new(),
            global_settings_load_error: global_load.error,
            project_settings_load_error: project_load.error,
            errors,
            pending_writes: Vec::new(),
            drain_scheduled: false,
        };
        manager.global_settings = global_load.settings;
        manager.project_settings = project_load.settings;
        manager.settings = deep_merge_settings(&manager.global_settings, &manager.project_settings);
        manager
    }

    /// Creates an in-memory manager (no file I/O).
    pub fn in_memory(settings: Settings, options: SettingsManagerCreateOptions) -> Self {
        let mut storage = InMemorySettingsStorage::new();
        let mut initial = settings;
        migrate_settings(&mut initial);
        let _ = storage.with_lock(SettingsScope::Global, &mut |_| {
            Some(js_json::stringify_lossless_pretty(&Value::Object(initial.clone())))
        });
        Self::from_storage(Box::new(storage), options)
    }

    fn load_from_storage(
        storage: &mut dyn SettingsStorage,
        scope: SettingsScope,
        project_trusted: bool,
    ) -> Result<Settings, SettingsFailure> {
        if scope == SettingsScope::Project && !project_trusted {
            return Ok(Settings::new());
        }

        let mut content = None;
        storage.with_lock(scope, &mut |current| {
            content = current.map(str::to_owned);
            None
        })?;

        let Some(content) = content.filter(|content| !content.is_empty()) else {
            return Ok(Settings::new());
        };
        let mut settings = parse_settings(&content)?;
        migrate_settings(&mut settings);
        Ok(settings)
    }

    fn try_load_from_storage(
        storage: &mut dyn SettingsStorage,
        scope: SettingsScope,
        project_trusted: bool,
    ) -> SettingsLoad {
        match Self::load_from_storage(storage, scope, project_trusted) {
            Ok(settings) => SettingsLoad { settings, error: None },
            Err(error) => SettingsLoad {
                settings: Settings::new(),
                error: Some(error.message().clone()),
            },
        }
    }

    pub fn get_global_settings(&self) -> Settings {
        self.global_settings.clone()
    }

    pub fn get_project_settings(&self) -> Settings {
        self.project_settings.clone()
    }

    pub fn get_merged_settings(&self) -> Settings {
        let mut settings = self.settings.clone();
        if let Some(profile) = self.session_sandbox_profile {
            settings.insert("sandboxProfile".into(), Value::String(profile.as_str().into()));
        }
        settings
    }

    pub fn get_agent_profile(&self, agent_type: &JsString) -> Option<Value> {
        let normalized = agent_type.trim();
        if normalized.is_empty() {
            return None;
        }
        let configured = self
            .settings
            .get("agents")
            .and_then(Value::as_object)
            .and_then(|agents| agents.get(&normalized));
        match configured.filter(|value| !value.is_null()) {
            Some(value) => Some(value.clone()),
            None => normalized
                .to_utf8()
                .ok()
                .as_deref()
                .and_then(default_agent_profile)
                .map(|profile| js_json::to_value(&profile).expect("built-in agent profile must serialize")),
        }
    }

    pub fn get_permission_prompt_tool(&self) -> Option<&JsString> {
        self.settings.get("permissionPromptTool").and_then(Value::as_string)
    }

    pub fn get_approval_policy(&self) -> ApprovalPolicy {
        normalize_approval_policy(enum_literal(self.settings.get("approvalPolicy")).as_deref())
    }

    pub fn get_approval_preset(&self) -> ApprovalPresetName {
        if let Some(preset) = self
            .settings
            .get("approvalPreset")
            .and_then(Value::as_string)
            .filter(|preset| is_approval_preset_name(preset))
        {
            return ApprovalPresetName::parse(&preset.to_utf8().expect("validated ASCII preset"))
                .expect("validated approval preset");
        }
        approval_preset_for_policy(self.get_approval_policy())
    }

    pub fn set_approval_policy(&mut self, policy: ApprovalPolicy) {
        set_string(&mut self.global_settings, "approvalPolicy", policy.as_str());
        set_string(
            &mut self.global_settings,
            "approvalPreset",
            approval_preset_for_policy(policy).as_str(),
        );
        self.mark_modified("approvalPolicy", None);
        self.mark_modified("approvalPreset", None);
        self.save();
    }

    pub fn set_approval_preset(
        &mut self,
        preset_name: ApprovalPresetName,
        scope: SettingsScope,
    ) -> Result<(), SettingsFailure> {
        let preset = find_approval_preset(preset_name);
        if scope == SettingsScope::Project {
            self.assert_project_trusted_for_write()?;
            let mut project_settings = self.project_settings.clone();
            set_string(&mut project_settings, "approvalPreset", preset_name.as_str());
            set_string(&mut project_settings, "approvalPolicy", preset.policy.as_str());
            self.mark_project_modified("approvalPreset", None);
            self.mark_project_modified("approvalPolicy", None);
            return self.save_project_settings(project_settings);
        }

        set_string(&mut self.global_settings, "approvalPreset", preset_name.as_str());
        set_string(&mut self.global_settings, "approvalPolicy", preset.policy.as_str());
        self.mark_modified("approvalPreset", None);
        self.mark_modified("approvalPolicy", None);
        self.save();
        Ok(())
    }

    pub fn get_sandbox_profile(&self) -> SandboxProfileName {
        self.get_explicit_sandbox_profile()
            .unwrap_or(SandboxProfileName::WorkspaceWrite)
    }

    pub fn get_configured_sandbox_profile(&self) -> SandboxProfileName {
        self.get_explicit_configured_sandbox_profile()
            .unwrap_or(SandboxProfileName::WorkspaceWrite)
    }

    fn get_explicit_configured_sandbox_profile(&self) -> Option<SandboxProfileName> {
        if let Some(profile) = self
            .project_settings
            .get("sandboxProfile")
            .and_then(Value::as_string)
            .filter(|profile| is_sandbox_profile_name(profile))
        {
            return SandboxProfileName::parse(&profile.to_utf8().expect("validated ASCII profile"));
        }
        self.global_settings
            .get("sandboxProfile")
            .and_then(Value::as_string)
            .filter(|profile| is_sandbox_profile_name(profile))
            .and_then(|profile| SandboxProfileName::parse(&profile.to_utf8().expect("validated ASCII profile")))
    }

    pub fn get_explicit_sandbox_profile(&self) -> Option<SandboxProfileName> {
        self.session_sandbox_profile
            .or_else(|| self.get_explicit_configured_sandbox_profile())
    }

    pub fn get_session_sandbox_profile(&self) -> Option<SandboxProfileName> {
        self.session_sandbox_profile
    }

    /// `setSessionSandboxProfile` on the live settings snapshot.
    pub fn set_session_sandbox_profile(&mut self, profile: Option<SandboxProfileName>) {
        self.session_sandbox_profile = profile;
    }

    pub fn set_sandbox_profile(
        &mut self,
        profile: SandboxProfileName,
        scope: SettingsScope,
    ) -> Result<(), SettingsFailure> {
        if scope == SettingsScope::Project {
            return self.update_project_settings("sandboxProfile", &mut |settings| {
                set_string(settings, "sandboxProfile", profile.as_str());
            });
        }
        set_string(&mut self.global_settings, "sandboxProfile", profile.as_str());
        self.mark_modified("sandboxProfile", None);
        self.save();
        Ok(())
    }

    pub fn is_project_trusted(&self) -> bool {
        self.project_trusted
    }

    pub fn set_project_trusted(&mut self, trusted: bool) {
        if self.project_trusted == trusted {
            return;
        }
        self.project_trusted = trusted;
        self.modified_project_fields.clear();
        self.modified_project_nested_fields.clear();

        if !trusted {
            self.project_settings = Settings::new();
            self.project_settings_load_error = None;
            self.settings = deep_merge_settings(&self.global_settings, &self.project_settings);
            return;
        }

        let project_load = Self::try_load_from_storage(self.storage.as_mut(), SettingsScope::Project, trusted);
        self.project_settings = project_load.settings;
        self.project_settings_load_error = project_load.error.clone();
        if let Some(error) = project_load.error {
            self.record_error(SettingsScope::Project, &error);
        }
        self.settings = deep_merge_settings(&self.global_settings, &self.project_settings);
    }

    pub fn reload(&mut self) {
        self.flush();
        let global_load = Self::try_load_from_storage(self.storage.as_mut(), SettingsScope::Global, true);
        match global_load.error {
            Some(error) => {
                self.global_settings_load_error = Some(error.clone());
                self.record_error(SettingsScope::Global, &error);
            }
            None => {
                self.global_settings = global_load.settings;
                self.global_settings_load_error = None;
            }
        }

        self.modified_fields.clear();
        self.modified_nested_fields.clear();
        self.modified_project_fields.clear();
        self.modified_project_nested_fields.clear();

        let project_load =
            Self::try_load_from_storage(self.storage.as_mut(), SettingsScope::Project, self.project_trusted);
        match project_load.error {
            Some(error) => {
                self.project_settings_load_error = Some(error.clone());
                self.record_error(SettingsScope::Project, &error);
            }
            None => {
                self.project_settings = project_load.settings;
                self.project_settings_load_error = None;
            }
        }

        self.settings = deep_merge_settings(&self.global_settings, &self.project_settings);
    }

    pub fn apply_overrides(&mut self, overrides: &Settings) {
        self.settings = deep_merge_settings(&self.settings, overrides);
    }

    fn mark_modified(&mut self, field: &str, nested_key: Option<&str>) {
        push_unique(&mut self.modified_fields, field);
        if let Some(nested_key) = nested_key {
            push_nested(&mut self.modified_nested_fields, field, nested_key);
        }
    }

    fn mark_project_modified(&mut self, field: &str, nested_key: Option<&str>) {
        push_unique(&mut self.modified_project_fields, field);
        if let Some(nested_key) = nested_key {
            push_nested(&mut self.modified_project_nested_fields, field, nested_key);
        }
    }

    fn assert_project_trusted_for_write(&self) -> Result<(), SettingsFailure> {
        if !self.project_trusted {
            return Err(SettingsFailure::new(
                "Project is not trusted; refusing to write project settings",
            ));
        }
        Ok(())
    }

    fn record_error(&mut self, scope: SettingsScope, error: impl Into<JsString>) {
        self.errors.push(SettingsError {
            scope,
            message: error.into(),
        });
    }

    fn clear_modified_scope(&mut self, scope: SettingsScope) {
        match scope {
            SettingsScope::Global => {
                self.modified_fields.clear();
                self.modified_nested_fields.clear();
            }
            SettingsScope::Project => {
                self.modified_project_fields.clear();
                self.modified_project_nested_fields.clear();
            }
        }
    }

    fn persist_scoped_settings(
        &mut self,
        scope: SettingsScope,
        snapshot_settings: &Settings,
        modified_fields: &[String],
        modified_nested_fields: &[(String, Vec<String>)],
    ) -> Result<(), SettingsFailure> {
        let mut storage_error = None;
        let mut current_parse_error = None;
        let result = self.storage.with_lock(scope, &mut |current| {
            let current_file_settings = match current.filter(|content| !content.is_empty()) {
                Some(content) => match parse_settings(content) {
                    Ok(mut settings) => {
                        migrate_settings(&mut settings);
                        settings
                    }
                    Err(error) => {
                        current_parse_error = Some(error.message().clone());
                        return None;
                    }
                },
                None => Settings::new(),
            };
            let mut merged = current_file_settings.clone();
            for field in modified_fields {
                let Some(value) = snapshot_settings.get(field) else {
                    merged.shift_remove(field);
                    continue;
                };
                let nested_modified = modified_nested_fields
                    .iter()
                    .find(|(name, _)| name == field)
                    .map(|(_, nested)| nested);
                if let Some(nested_modified) = nested_modified.filter(|_| value.is_object()) {
                    let mut merged_nested = current_file_settings
                        .get(field)
                        .and_then(Value::as_object)
                        .cloned()
                        .unwrap_or_default();
                    let in_memory_nested = value.as_object().expect("checked object");
                    for nested_key in nested_modified {
                        match in_memory_nested.get(nested_key) {
                            Some(value) => {
                                merged_nested.insert(nested_key.as_str().into(), value.clone());
                            }
                            None => {
                                merged_nested.shift_remove(nested_key);
                            }
                        }
                    }
                    merged.insert(field.as_str().into(), Value::Object(merged_nested));
                } else {
                    merged.insert(field.as_str().into(), value.clone());
                }
            }
            Some(js_json::stringify_lossless_pretty(&Value::Object(merged)))
        });
        if let Err(error) = result {
            storage_error = Some(error.message().clone());
        }
        if let Some(error) = current_parse_error {
            return Err(SettingsFailure::new(error));
        }
        if let Some(error) = storage_error {
            return Err(SettingsFailure::new(error));
        }
        Ok(())
    }

    fn save(&mut self) {
        self.settings = deep_merge_settings(&self.global_settings, &self.project_settings);
        if self.global_settings_load_error.is_some() {
            return;
        }

        self.pending_writes.push(PendingSettingsWrite {
            scope: SettingsScope::Global,
            settings: self.global_settings.clone(),
            modified_fields: self.modified_fields.clone(),
            modified_nested_fields: self.modified_nested_fields.clone(),
        });
    }

    fn save_project_settings(&mut self, settings: Settings) -> Result<(), SettingsFailure> {
        self.assert_project_trusted_for_write()?;
        self.project_settings = settings;
        self.settings = deep_merge_settings(&self.global_settings, &self.project_settings);
        if self.project_settings_load_error.is_some() {
            return Ok(());
        }

        self.pending_writes.push(PendingSettingsWrite {
            scope: SettingsScope::Project,
            settings: self.project_settings.clone(),
            modified_fields: self.modified_project_fields.clone(),
            modified_nested_fields: self.modified_project_nested_fields.clone(),
        });
        Ok(())
    }

    fn update_project_settings(
        &mut self,
        field: &str,
        update: &mut dyn FnMut(&mut Settings),
    ) -> Result<(), SettingsFailure> {
        self.assert_project_trusted_for_write()?;
        let mut project_settings = self.project_settings.clone();
        update(&mut project_settings);
        self.mark_project_modified(field, None);
        self.save_project_settings(project_settings)
    }

    fn persist_pending_settings(&mut self, pending: &PendingSettingsWrite) -> Result<(), SettingsFailure> {
        if pending.scope == SettingsScope::Project {
            self.assert_project_trusted_for_write()?;
        }
        self.persist_scoped_settings(
            pending.scope,
            &pending.settings,
            &pending.modified_fields,
            &pending.modified_nested_fields,
        )?;
        self.clear_modified_scope(pending.scope);
        Ok(())
    }

    /// Settles all earlier write snapshots in source order, recording task errors without throwing.
    pub fn flush(&mut self) {
        for pending in std::mem::take(&mut self.pending_writes) {
            // Like the source task catch, failure need not roll back storage to continue later snapshots.
            if let Err(error) = self.persist_pending_settings(&pending) {
                self.record_error(pending.scope, error.message());
            }
        }
    }

    pub fn drain_errors(&mut self) -> Vec<SettingsError> {
        std::mem::take(&mut self.errors)
    }

    pub fn get_last_changelog_version(&self) -> Option<&JsString> {
        self.settings.get("lastChangelogVersion").and_then(Value::as_string)
    }

    pub fn set_last_changelog_version(&mut self, version: &JsString) {
        set_string(&mut self.global_settings, "lastChangelogVersion", version);
        self.mark_modified("lastChangelogVersion", None);
        self.save();
    }

    pub fn get_session_dir(&self) -> Option<pi_js_compat::js_json::JsString> {
        let value = self.settings.get("sessionDir").and_then(Value::as_string)?;
        if value.is_empty() {
            return Some(Default::default());
        }
        Some(normalize_path(value.clone(), &PathInputOptions::default()).unwrap_or_else(|_| value.clone()))
    }

    pub fn get_default_provider(&self) -> Option<&JsString> {
        self.settings.get("defaultProvider").and_then(Value::as_string)
    }

    pub fn get_default_model(&self) -> Option<&JsString> {
        self.settings.get("defaultModel").and_then(Value::as_string)
    }

    pub fn set_default_provider(&mut self, provider: &str) {
        set_string(&mut self.global_settings, "defaultProvider", provider);
        self.mark_modified("defaultProvider", None);
        self.save();
    }

    pub fn set_default_model(&mut self, model_id: &str) {
        set_string(&mut self.global_settings, "defaultModel", model_id);
        self.mark_modified("defaultModel", None);
        self.save();
    }

    pub fn set_default_model_and_provider(&mut self, provider: &str, model_id: &str) {
        set_string(&mut self.global_settings, "defaultProvider", provider);
        set_string(&mut self.global_settings, "defaultModel", model_id);
        self.mark_modified("defaultProvider", None);
        self.mark_modified("defaultModel", None);
        self.save();
    }

    pub fn get_steering_mode(&self) -> pi_agent_core::harness::QueueMode {
        parse_queue_mode(enum_literal(self.settings.get("steeringMode")).as_deref())
            .unwrap_or(pi_agent_core::harness::QueueMode::All)
    }

    pub fn set_steering_mode(&mut self, mode: pi_agent_core::harness::QueueMode) {
        set_string(&mut self.global_settings, "steeringMode", mode.as_str());
        self.mark_modified("steeringMode", None);
        self.save();
    }

    pub fn get_follow_up_mode(&self) -> pi_agent_core::harness::QueueMode {
        parse_queue_mode(enum_literal(self.settings.get("followUpMode")).as_deref())
            .unwrap_or(pi_agent_core::harness::QueueMode::OneAtATime)
    }

    pub fn set_follow_up_mode(&mut self, mode: pi_agent_core::harness::QueueMode) {
        set_string(&mut self.global_settings, "followUpMode", mode.as_str());
        self.mark_modified("followUpMode", None);
        self.save();
    }

    pub fn get_theme_setting(&self) -> Option<&JsString> {
        self.settings.get("theme").and_then(Value::as_string)
    }

    pub fn get_theme(&self) -> Option<&JsString> {
        self.get_theme_setting().filter(|theme| !theme.contains("/"))
    }

    pub fn set_theme(&mut self, theme: &JsString) {
        set_string(&mut self.global_settings, "theme", theme);
        self.mark_modified("theme", None);
        self.save();
    }

    pub fn get_default_thinking_level(&self) -> Option<ThinkingLevel> {
        parse_thinking_level(enum_literal(self.settings.get("defaultThinkingLevel")).as_deref())
    }

    pub fn set_default_thinking_level(&mut self, level: ThinkingLevel) {
        set_string(&mut self.global_settings, "defaultThinkingLevel", level.as_str());
        self.mark_modified("defaultThinkingLevel", None);
        self.save();
    }

    pub fn get_transport(&self) -> Transport {
        parse_transport(enum_literal(self.settings.get("transport")).as_deref()).unwrap_or(Transport::Auto)
    }

    pub fn set_transport(&mut self, transport: Transport) {
        let value = match transport {
            Transport::Sse => "sse",
            Transport::Websocket => "websocket",
            Transport::WebsocketCached => "websocket-cached",
            Transport::Auto => "auto",
        };
        set_string(&mut self.global_settings, "transport", value);
        self.mark_modified("transport", None);
        self.save();
    }

    pub fn get_compaction_enabled(&self) -> bool {
        self.settings
            .get("compaction")
            .and_then(Value::as_object)
            .and_then(|compaction| compaction.get("enabled"))
            .and_then(Value::as_bool)
            .unwrap_or(true)
    }

    pub fn set_compaction_enabled(&mut self, enabled: bool) {
        set_nested(&mut self.global_settings, "compaction", "enabled", Value::Bool(enabled));
        self.mark_modified("compaction", Some("enabled"));
        self.save();
    }

    pub fn get_compaction_reserve_tokens(&self) -> f64 {
        nested_number(&self.settings, "compaction", "reserveTokens").unwrap_or(16_384.0)
    }

    pub fn get_compaction_keep_recent_tokens(&self) -> f64 {
        nested_number(&self.settings, "compaction", "keepRecentTokens").unwrap_or(20_000.0)
    }

    pub fn get_idle_compaction_enabled(&self) -> bool {
        nested_bool(&self.settings, "compaction", "idle").unwrap_or(true)
    }

    pub fn get_compaction_settings(&self) -> Result<CompactionSettingsView, SettingsFailure> {
        let threshold_percent = crate::core::compaction_threshold::parse_compaction_threshold_percent(
            self.settings
                .get("compaction")
                .and_then(|settings| settings.get("thresholdPercent")),
        )?;
        Ok(CompactionSettingsView {
            threshold_percent,
            enabled: self.get_compaction_enabled(),
            reserve_tokens: self.get_compaction_reserve_tokens(),
            keep_recent_tokens: self.get_compaction_keep_recent_tokens(),
        })
    }

    pub fn get_branch_summary_settings(&self) -> BranchSummarySettingsView {
        BranchSummarySettingsView {
            reserve_tokens: nested_number(&self.settings, "branchSummary", "reserveTokens").unwrap_or(16_384.0),
            skip_prompt: self.get_branch_summary_skip_prompt(),
        }
    }

    pub fn get_branch_summary_skip_prompt(&self) -> bool {
        self.settings
            .get("branchSummary")
            .and_then(Value::as_object)
            .and_then(|branch_summary| branch_summary.get("skipPrompt"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    pub fn get_retry_enabled(&self) -> bool {
        self.settings
            .get("retry")
            .and_then(Value::as_object)
            .and_then(|retry| retry.get("enabled"))
            .and_then(Value::as_bool)
            .unwrap_or(true)
    }

    pub fn set_retry_enabled(&mut self, enabled: bool) {
        set_nested(&mut self.global_settings, "retry", "enabled", Value::Bool(enabled));
        self.mark_modified("retry", Some("enabled"));
        self.save();
    }

    pub fn get_retry_settings(&self) -> Result<RetrySettingsView, SettingsFailure> {
        let base_delay_ms = self.parse_retry_delay("baseDelayMs", 30_000.0)?;
        let max_backoff_ms = self.parse_retry_delay("maxBackoffMs", 300_000.0)?;
        if max_backoff_ms < base_delay_ms {
            return Err(SettingsFailure::new(
                "retry.maxBackoffMs must be at least retry.baseDelayMs",
            ));
        }
        Ok(RetrySettingsView {
            enabled: self.get_retry_enabled(),
            max_retries: nested_number(&self.settings, "retry", "maxRetries").unwrap_or(30.0),
            base_delay_ms,
            max_backoff_ms,
        })
    }

    fn parse_retry_delay(&self, name: &str, default: f64) -> Result<f64, SettingsFailure> {
        let Some(value) = self.settings.get("retry").and_then(|retry| retry.get(name)) else {
            return Ok(default);
        };
        if let Some(delay) = value.as_f64()
            && delay.is_finite()
            && delay.fract() == 0.0
            && (0.0..=2_147_483_647.0).contains(&delay)
        {
            return Ok(delay);
        }
        Err(SettingsFailure::new(format!(
            "Invalid retry.{name} setting: {}; expected integer milliseconds between 0 and 2147483647",
            js_json::to_js_string(&js_json::JsValue::from(value))
                .map_err(SettingsFailure::new)?
                .to_utf8_lossy()
        )))
    }

    pub fn get_http_idle_timeout_ms(&self) -> Result<u64, SettingsFailure> {
        self.parse_timeout_setting("httpIdleTimeoutMs")?
            .map_or(Ok(DEFAULT_HTTP_IDLE_TIMEOUT_MS), Ok)
    }

    pub fn set_http_idle_timeout_ms(&mut self, timeout_ms: f64) -> Result<(), SettingsFailure> {
        if !timeout_ms.is_finite() || timeout_ms < 0.0 {
            return Err(SettingsFailure::new(format!(
                "Invalid httpIdleTimeoutMs setting: {}",
                js_number::js_number_to_string(timeout_ms)
            )));
        }
        set_number(&mut self.global_settings, "httpIdleTimeoutMs", timeout_ms.floor());
        self.mark_modified("httpIdleTimeoutMs", None);
        self.save();
        Ok(())
    }

    fn parse_timeout_setting(&self, setting_name: &str) -> Result<Option<u64>, SettingsFailure> {
        let value = self.settings.get(setting_name);
        let parsed = match value {
            None => None,
            Some(Value::Number(number)) => parse_http_idle_timeout_ms(HttpIdleTimeout::Number(*number)),
            // The timeout grammar is ASCII: unpaired units are an invalid number, as in JavaScript.
            Some(Value::String(text)) => text
                .to_utf8()
                .ok()
                .and_then(|text| parse_http_idle_timeout_ms(HttpIdleTimeout::String(&text))),
            Some(_) => None,
        };
        if parsed.is_some() {
            return Ok(parsed);
        }
        if let Some(value) = value {
            return Err(SettingsFailure::new(
                JsString::from(format!("Invalid {setting_name} setting: "))
                    + js_json::to_js_string(value).map_err(SettingsFailure::new)?,
            ));
        }
        Ok(None)
    }

    pub fn get_provider_retry_settings(&self) -> ProviderRetrySettingsView {
        let provider = self
            .settings
            .get("retry")
            .and_then(Value::as_object)
            .and_then(|retry| retry.get("provider"))
            .and_then(Value::as_object);
        ProviderRetrySettingsView {
            timeout_ms: provider
                .and_then(|provider| provider.get("timeoutMs"))
                .and_then(Value::as_f64),
            max_retries: provider
                .and_then(|provider| provider.get("maxRetries"))
                .and_then(Value::as_f64),
            max_retry_delay_ms: provider
                .and_then(|provider| provider.get("maxRetryDelayMs"))
                .and_then(Value::as_f64)
                .unwrap_or(60_000.0),
        }
    }

    pub fn get_websocket_connect_timeout_ms(&self) -> Result<Option<u64>, SettingsFailure> {
        self.parse_timeout_setting("websocketConnectTimeoutMs")
    }

    pub fn get_hide_thinking_block(&self) -> bool {
        self.settings
            .get("hideThinkingBlock")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    pub fn get_hide_tool_output(&self) -> bool {
        self.settings
            .get("hideToolOutput")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    pub fn set_hide_tool_output(&mut self, hide: bool) {
        set_bool(&mut self.global_settings, "hideToolOutput", hide);
        self.mark_modified("hideToolOutput", None);
        self.save();
    }

    pub fn get_hide_coordination_messages(&self) -> bool {
        self.settings
            .get("hideCoordinationMessages")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    pub fn set_hide_coordination_messages(&mut self, hide: bool) {
        set_bool(&mut self.global_settings, "hideCoordinationMessages", hide);
        self.mark_modified("hideCoordinationMessages", None);
        self.save();
    }

    pub fn get_external_editor_command(&self) -> JsString {
        if let Some(configured) = self.settings.get("externalEditor").and_then(Value::as_string)
            && !configured.trim().is_empty()
        {
            return configured.clone();
        }
        if let Some(environment_editor) = pi_js_compat::process_env::get("VISUAL")
            .filter(|value| !value.is_empty())
            .or_else(|| pi_js_compat::process_env::get("EDITOR").filter(|value| !value.is_empty()))
        {
            return environment_editor.into();
        }
        if cfg!(target_os = "windows") {
            "notepad".into()
        } else {
            "nano".into()
        }
    }

    pub fn set_hide_thinking_block(&mut self, hide: bool) {
        set_bool(&mut self.global_settings, "hideThinkingBlock", hide);
        self.mark_modified("hideThinkingBlock", None);
        self.save();
    }

    pub fn get_shell_path(&self) -> Option<JsString> {
        self.settings.get("shellPath").and_then(Value::as_string).cloned()
    }

    pub fn set_shell_path(&mut self, path: Option<&JsString>) {
        set_optional_string(&mut self.global_settings, "shellPath", path);
        self.mark_modified("shellPath", None);
        self.save();
    }

    pub fn get_quiet_startup(&self) -> bool {
        self.settings
            .get("quietStartup")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    pub fn set_quiet_startup(&mut self, quiet: bool) {
        set_bool(&mut self.global_settings, "quietStartup", quiet);
        self.mark_modified("quietStartup", None);
        self.save();
    }

    pub fn get_default_project_trust(&self) -> DefaultProjectTrust {
        match enum_literal(self.global_settings.get("defaultProjectTrust")).as_deref() {
            Some("always") => DefaultProjectTrust::Always,
            Some("never") => DefaultProjectTrust::Never,
            _ => DefaultProjectTrust::Ask,
        }
    }

    pub fn set_default_project_trust(&mut self, trust: DefaultProjectTrust) {
        let value = match trust {
            DefaultProjectTrust::Ask => "ask",
            DefaultProjectTrust::Always => "always",
            DefaultProjectTrust::Never => "never",
        };
        set_string(&mut self.global_settings, "defaultProjectTrust", value);
        self.mark_modified("defaultProjectTrust", None);
        self.save();
    }

    pub fn get_shell_command_prefix(&self) -> Option<&JsString> {
        self.settings.get("shellCommandPrefix").and_then(Value::as_string)
    }

    pub fn set_shell_command_prefix(&mut self, prefix: Option<&JsString>) {
        set_optional_string(&mut self.global_settings, "shellCommandPrefix", prefix);
        self.mark_modified("shellCommandPrefix", None);
        self.save();
    }

    pub fn get_npm_command(&self) -> Option<Vec<JsString>> {
        string_array(self.settings.get("npmCommand"))
    }

    pub fn set_npm_command(&mut self, command: Option<&[JsString]>) {
        set_optional_string_array(&mut self.global_settings, "npmCommand", command);
        self.mark_modified("npmCommand", None);
        self.save();
    }

    pub fn get_collapse_changelog(&self) -> bool {
        self.settings
            .get("collapseChangelog")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    pub fn set_collapse_changelog(&mut self, collapse: bool) {
        set_bool(&mut self.global_settings, "collapseChangelog", collapse);
        self.mark_modified("collapseChangelog", None);
        self.save();
    }

    pub fn get_enable_install_telemetry(&self) -> bool {
        self.settings
            .get("enableInstallTelemetry")
            .and_then(Value::as_bool)
            .unwrap_or(true)
    }

    pub fn set_enable_install_telemetry(&mut self, enabled: bool) {
        set_bool(&mut self.global_settings, "enableInstallTelemetry", enabled);
        self.mark_modified("enableInstallTelemetry", None);
        self.save();
    }

    pub fn get_enable_analytics(&self) -> bool {
        self.settings
            .get("enableAnalytics")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    pub fn get_tracking_id(&self) -> Option<&JsString> {
        self.settings.get("trackingId").and_then(Value::as_string)
    }

    pub fn set_enable_analytics(&mut self, enabled: bool) -> Result<(), pi_js_compat::node_error::NativeNodeError> {
        set_bool(&mut self.global_settings, "enableAnalytics", enabled);
        self.mark_modified("enableAnalytics", None);
        if enabled
            && self
                .global_settings
                .get("trackingId")
                .and_then(Value::as_string)
                .is_none_or(JsString::is_empty)
        {
            set_string(&mut self.global_settings, "trackingId", random_uuid()?);
            self.mark_modified("trackingId", None);
        }
        self.save();
        Ok(())
    }

    pub fn get_packages(&self) -> Result<Vec<PackageSource>, SettingsFailure> {
        parse_packages(self.settings.get("packages"))
    }

    pub fn set_packages(&mut self, packages: Vec<PackageSource>) {
        set_value(&mut self.global_settings, "packages", packages_to_value(&packages));
        self.mark_modified("packages", None);
        self.save();
    }

    pub fn set_project_packages(&mut self, packages: Vec<PackageSource>) -> Result<(), SettingsFailure> {
        let value = packages_to_value(&packages);
        self.update_project_settings("packages", &mut |settings| {
            settings.insert("packages".into(), value.clone());
        })
    }

    pub fn get_extension_paths(&self) -> Vec<JsString> {
        string_array(self.settings.get("extensions")).unwrap_or_default()
    }

    pub fn get_disabled_extensions(&self) -> Vec<JsString> {
        string_array(self.settings.get("disabledExtensions")).unwrap_or_default()
    }

    pub fn set_extension_paths(&mut self, paths: &[JsString]) {
        set_string_array(&mut self.global_settings, "extensions", paths);
        self.mark_modified("extensions", None);
        self.save();
    }

    pub fn set_project_extension_paths(&mut self, paths: &[JsString]) -> Result<(), SettingsFailure> {
        self.update_project_settings("extensions", &mut |settings| {
            set_string_array(settings, "extensions", paths);
        })
    }

    pub fn get_skill_paths(&self) -> Vec<JsString> {
        string_array(self.settings.get("skills")).unwrap_or_default()
    }

    pub fn set_skill_paths(&mut self, paths: &[JsString]) {
        set_string_array(&mut self.global_settings, "skills", paths);
        self.mark_modified("skills", None);
        self.save();
    }

    pub fn set_project_skill_paths(&mut self, paths: &[JsString]) -> Result<(), SettingsFailure> {
        self.update_project_settings("skills", &mut |settings| {
            set_string_array(settings, "skills", paths);
        })
    }

    pub fn get_prompt_template_paths(&self) -> Vec<JsString> {
        string_array(self.settings.get("prompts")).unwrap_or_default()
    }

    pub fn set_prompt_template_paths(&mut self, paths: &[JsString]) {
        set_string_array(&mut self.global_settings, "prompts", paths);
        self.mark_modified("prompts", None);
        self.save();
    }

    pub fn set_project_prompt_template_paths(&mut self, paths: &[JsString]) -> Result<(), SettingsFailure> {
        self.update_project_settings("prompts", &mut |settings| {
            set_string_array(settings, "prompts", paths);
        })
    }

    pub fn get_theme_paths(&self) -> Vec<JsString> {
        string_array(self.settings.get("themes")).unwrap_or_default()
    }

    pub fn set_theme_paths(&mut self, paths: &[JsString]) {
        set_string_array(&mut self.global_settings, "themes", paths);
        self.mark_modified("themes", None);
        self.save();
    }

    pub fn set_project_theme_paths(&mut self, paths: &[JsString]) -> Result<(), SettingsFailure> {
        self.update_project_settings("themes", &mut |settings| {
            set_string_array(settings, "themes", paths);
        })
    }

    pub fn get_enable_skill_commands(&self) -> bool {
        self.settings
            .get("enableSkillCommands")
            .and_then(Value::as_bool)
            .unwrap_or(true)
    }

    pub fn set_enable_skill_commands(&mut self, enabled: bool) {
        set_bool(&mut self.global_settings, "enableSkillCommands", enabled);
        self.mark_modified("enableSkillCommands", None);
        self.save();
    }

    pub fn get_thinking_budgets(&self) -> Option<ThinkingBudgetsSettings> {
        self.settings
            .get("thinkingBudgets")
            .and_then(|value| js_json::from_value(value).ok())
    }

    pub fn get_show_images(&self) -> bool {
        nested_bool(&self.settings, "terminal", "showImages").unwrap_or(true)
    }

    pub fn set_show_images(&mut self, show: bool) {
        set_nested(&mut self.global_settings, "terminal", "showImages", Value::Bool(show));
        self.mark_modified("terminal", Some("showImages"));
        self.save();
    }

    pub fn get_image_width_cells(&self) -> f64 {
        nested_number(&self.settings, "terminal", "imageWidthCells")
            .filter(|width| width.is_finite())
            .map_or(60.0, |width| width.floor().max(1.0))
    }

    pub fn set_image_width_cells(&mut self, width: f64) {
        set_number_nested(
            &mut self.global_settings,
            "terminal",
            "imageWidthCells",
            js_floor_max(width, 1.0),
        );
        self.mark_modified("terminal", Some("imageWidthCells"));
        self.save();
    }

    pub fn get_clear_on_shrink(&self) -> bool {
        if let Some(clear) = nested_bool(&self.settings, "terminal", "clearOnShrink") {
            return clear;
        }
        pi_js_compat::process_env::get("PI_CLEAR_ON_SHRINK").is_some_and(|value| value == "1")
    }

    pub fn set_clear_on_shrink(&mut self, enabled: bool) {
        set_nested(
            &mut self.global_settings,
            "terminal",
            "clearOnShrink",
            Value::Bool(enabled),
        );
        self.mark_modified("terminal", Some("clearOnShrink"));
        self.save();
    }

    pub fn get_show_terminal_progress(&self) -> bool {
        nested_bool(&self.settings, "terminal", "showTerminalProgress").unwrap_or(false)
    }

    pub fn set_show_terminal_progress(&mut self, enabled: bool) {
        set_nested(
            &mut self.global_settings,
            "terminal",
            "showTerminalProgress",
            Value::Bool(enabled),
        );
        self.mark_modified("terminal", Some("showTerminalProgress"));
        self.save();
    }

    pub fn get_image_auto_resize(&self) -> bool {
        nested_bool(&self.settings, "images", "autoResize").unwrap_or(true)
    }

    pub fn set_image_auto_resize(&mut self, enabled: bool) {
        set_nested(&mut self.global_settings, "images", "autoResize", Value::Bool(enabled));
        self.mark_modified("images", Some("autoResize"));
        self.save();
    }

    pub fn get_block_images(&self) -> bool {
        nested_bool(&self.settings, "images", "blockImages").unwrap_or(false)
    }

    pub fn set_block_images(&mut self, blocked: bool) {
        set_nested(&mut self.global_settings, "images", "blockImages", Value::Bool(blocked));
        self.mark_modified("images", Some("blockImages"));
        self.save();
    }

    pub fn get_enabled_models(&self) -> Option<Vec<JsString>> {
        string_array(self.settings.get("enabledModels"))
    }

    pub fn set_enabled_models(
        &mut self,
        patterns: Option<&[JsString]>,
        scope: SettingsScope,
    ) -> Result<(), SettingsFailure> {
        if scope == SettingsScope::Project {
            let value = patterns.map(strings_to_value).unwrap_or(Value::Null);
            return self.update_project_settings("enabledModels", &mut |settings| {
                if value.is_null() {
                    settings.shift_remove("enabledModels");
                } else {
                    settings.insert("enabledModels".into(), value.clone());
                }
            });
        }
        match patterns {
            Some(patterns) => set_string_array(&mut self.global_settings, "enabledModels", patterns),
            None => {
                self.global_settings.shift_remove("enabledModels");
            }
        }
        self.mark_modified("enabledModels", None);
        self.save();
        Ok(())
    }

    pub fn get_double_escape_action(&self) -> DoubleEscapeAction {
        match enum_literal(self.settings.get("doubleEscapeAction")).as_deref() {
            Some("fork") => DoubleEscapeAction::Fork,
            Some("none") => DoubleEscapeAction::None,
            _ => DoubleEscapeAction::Tree,
        }
    }

    pub fn set_double_escape_action(&mut self, action: DoubleEscapeAction) {
        let value = match action {
            DoubleEscapeAction::Fork => "fork",
            DoubleEscapeAction::Tree => "tree",
            DoubleEscapeAction::None => "none",
        };
        set_string(&mut self.global_settings, "doubleEscapeAction", value);
        self.mark_modified("doubleEscapeAction", None);
        self.save();
    }

    pub fn get_tree_filter_mode(&self) -> TreeFilterMode {
        match enum_literal(self.settings.get("treeFilterMode")).as_deref() {
            Some("no-tools") => TreeFilterMode::NoTools,
            Some("user-only") => TreeFilterMode::UserOnly,
            Some("labeled-only") => TreeFilterMode::LabeledOnly,
            Some("all") => TreeFilterMode::All,
            _ => TreeFilterMode::Default,
        }
    }

    pub fn set_tree_filter_mode(&mut self, mode: TreeFilterMode) {
        let value = match mode {
            TreeFilterMode::Default => "default",
            TreeFilterMode::NoTools => "no-tools",
            TreeFilterMode::UserOnly => "user-only",
            TreeFilterMode::LabeledOnly => "labeled-only",
            TreeFilterMode::All => "all",
        };
        set_string(&mut self.global_settings, "treeFilterMode", value);
        self.mark_modified("treeFilterMode", None);
        self.save();
    }

    pub fn get_show_hardware_cursor(&self) -> bool {
        if let Some(value) = self.settings.get("showHardwareCursor").and_then(Value::as_bool) {
            return value;
        }
        pi_js_compat::process_env::get("PI_HARDWARE_CURSOR").is_some_and(|value| value == "1")
    }

    pub fn set_show_hardware_cursor(&mut self, enabled: bool) {
        set_bool(&mut self.global_settings, "showHardwareCursor", enabled);
        self.mark_modified("showHardwareCursor", None);
        self.save();
    }

    pub fn get_editor_padding_x(&self) -> f64 {
        self.settings
            .get("editorPaddingX")
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
    }

    pub fn set_editor_padding_x(&mut self, padding: f64) {
        set_number(
            &mut self.global_settings,
            "editorPaddingX",
            js_floor_clamp(padding, 0.0, 3.0),
        );
        self.mark_modified("editorPaddingX", None);
        self.save();
    }

    pub fn get_output_pad(&self) -> u8 {
        if self.settings.get("outputPad").and_then(Value::as_f64) == Some(0.0) {
            0
        } else {
            1
        }
    }

    pub fn set_output_pad(&mut self, padding: u8) {
        set_number(&mut self.global_settings, "outputPad", f64::from(padding.min(1)));
        self.mark_modified("outputPad", None);
        self.save();
    }

    pub fn get_autocomplete_max_visible(&self) -> f64 {
        self.settings
            .get("autocompleteMaxVisible")
            .and_then(Value::as_f64)
            .unwrap_or(5.0)
    }

    pub fn set_autocomplete_max_visible(&mut self, max_visible: f64) {
        set_number(
            &mut self.global_settings,
            "autocompleteMaxVisible",
            js_floor_clamp(max_visible, 3.0, 20.0),
        );
        self.mark_modified("autocompleteMaxVisible", None);
        self.save();
    }

    pub fn get_code_block_indent(&self) -> JsString {
        nested_string(&self.settings, "markdown", "codeBlockIndent")
            .cloned()
            .unwrap_or_else(|| "  ".into())
    }

    pub fn get_warnings(&self) -> Settings {
        self.settings
            .get("warnings")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default()
    }

    pub fn set_warnings(&mut self, warnings: &Settings) {
        set_value(&mut self.global_settings, "warnings", Value::Object(warnings.clone()));
        self.mark_modified("warnings", None);
        self.save();
    }
}

struct SettingsLoad {
    settings: Settings,
    error: Option<JsString>,
}

fn parse_settings(content: &str) -> Result<Settings, SettingsFailure> {
    match js_json::parse_lossless(content).map_err(|error| SettingsFailure::new(error.to_string()))? {
        Value::Object(settings) => Ok(settings),
        other => Err(SettingsFailure::new(format!(
            "Settings must be a JSON object, got {}",
            js_json::stringify_lossless(&other)
        ))),
    }
}

fn migrate_settings(settings: &mut Settings) {
    if settings.contains_key("queueMode") && !settings.contains_key("steeringMode") {
        let queue_mode = settings.get("queueMode").cloned().unwrap_or(Value::Null);
        settings.insert("steeringMode".into(), queue_mode);
        settings.shift_remove("queueMode");
    }

    if !settings.contains_key("transport")
        && let Some(websockets) = settings.get("websockets").and_then(Value::as_bool)
    {
        settings.insert(
            "transport".into(),
            Value::String(if websockets { "websocket" } else { "sse" }.into()),
        );
        settings.shift_remove("websockets");
    }

    if let Some(Value::Object(skills_settings)) = settings.get("skills").cloned() {
        if let Some(enable) = skills_settings.get("enableSkillCommands")
            && !settings.contains_key("enableSkillCommands")
        {
            settings.insert("enableSkillCommands".into(), enable.clone());
        }
        match skills_settings.get("customDirectories").and_then(Value::as_array) {
            Some(directories) if !directories.is_empty() => {
                settings.insert("skills".into(), Value::Array(directories.clone()));
            }
            _ => {
                settings.shift_remove("skills");
            }
        }
    }

    if enum_literal(settings.get("approvalPreset")).as_deref() == Some("llm-approved") {
        settings.insert("approvalPreset".into(), Value::String("llm-approved-deny".into()));
    }

    if let Some(Value::Object(retry)) = settings.get("retry").cloned() {
        let mut retry = retry;
        if let Some(provider) = retry.get("provider") {
            let mut provider = provider.as_object().cloned().unwrap_or_default();
            let max_delay = retry.get("maxDelayMs").and_then(Value::as_f64);
            let provider_delay = provider.get("maxRetryDelayMs");
            if let Some(max_delay) = max_delay
                && provider_delay.is_none_or(Value::is_null)
            {
                provider.insert("maxRetryDelayMs".into(), json_number(max_delay));
                retry.insert("provider".into(), Value::Object(provider));
            }
        } else if let Some(max_delay) = retry.get("maxDelayMs").and_then(Value::as_f64) {
            let mut provider = JsObject::new();
            provider.insert("maxRetryDelayMs".into(), json_number(max_delay));
            retry.insert("provider".into(), Value::Object(provider));
        }
        retry.shift_remove("maxDelayMs");
        settings.insert("retry".into(), Value::Object(retry));
    }
}

/// `JSON.stringify` writes non-finite numbers as `null`.
fn json_number(value: f64) -> Value {
    if value.is_finite() {
        Value::Number(value)
    } else {
        Value::Null
    }
}

fn is_mergeable_object(value: &Value) -> bool {
    value.is_object()
}

fn deep_merge_objects(base: &Settings, overrides: &Settings) -> Settings {
    let mut result = base.clone();
    for (key, override_value) in overrides {
        let merged = match base.get(key) {
            Some(base_value) if is_mergeable_object(base_value) && is_mergeable_object(override_value) => {
                let base_object = base_value.as_object().expect("checked object");
                let override_object = override_value.as_object().expect("checked object");
                let mut overlay = base_object.clone();
                overlay.extend(override_object.clone());
                Value::Object(overlay)
            }
            _ => override_value.clone(),
        };
        result.insert(key.clone(), merged);
    }
    result
}

fn deep_merge_settings(base: &Settings, overrides: &Settings) -> Settings {
    deep_merge_objects(base, overrides)
}

fn default_agent_profile(name: &str) -> Option<AgentProfileSettings> {
    let profile = match name {
        "browser" => AgentProfileSettings {
            context: Some(AgentProfileContext::Fresh),
            model: None,
            thinking_level: None,
            tools: Some(vec!["browser-cli".into()]),
        },
        "explore" => AgentProfileSettings {
            context: None,
            model: Some("openai/gpt-5-mini".into()),
            thinking_level: Some(AgentProfileThinkingLevel::Low),
            tools: None,
        },
        "implement" => AgentProfileSettings {
            context: None,
            model: Some("openai/gpt-5.5".into()),
            thinking_level: Some(AgentProfileThinkingLevel::Medium),
            tools: None,
        },
        "verifier" => AgentProfileSettings {
            context: None,
            model: Some("openai/gpt-5-mini".into()),
            thinking_level: Some(AgentProfileThinkingLevel::Low),
            tools: None,
        },
        "documentation-update" => AgentProfileSettings {
            context: None,
            model: Some("openai/gpt-5-mini".into()),
            thinking_level: Some(AgentProfileThinkingLevel::Low),
            tools: None,
        },
        "reviewer" => AgentProfileSettings {
            context: Some(AgentProfileContext::Fresh),
            model: Some("openai/gpt-5.5".into()),
            thinking_level: Some(AgentProfileThinkingLevel::Medium),
            tools: None,
        },
        _ => return None,
    };
    Some(profile)
}

fn parse_queue_mode(value: Option<&str>) -> Option<pi_agent_core::harness::QueueMode> {
    match value {
        Some("all") => Some(pi_agent_core::harness::QueueMode::All),
        Some("one-at-a-time") => Some(pi_agent_core::harness::QueueMode::OneAtATime),
        _ => None,
    }
}

fn parse_thinking_level(value: Option<&str>) -> Option<ThinkingLevel> {
    match value {
        Some("off") => Some(ThinkingLevel::Off),
        Some("minimal") => Some(ThinkingLevel::Minimal),
        Some("low") => Some(ThinkingLevel::Low),
        Some("medium") => Some(ThinkingLevel::Medium),
        Some("high") => Some(ThinkingLevel::High),
        Some("xhigh") => Some(ThinkingLevel::XHigh),
        Some("max") => Some(ThinkingLevel::Max),
        Some("ultra") => Some(ThinkingLevel::Ultra),
        _ => None,
    }
}

fn parse_transport(value: Option<&str>) -> Option<Transport> {
    match value {
        Some("sse") => Some(Transport::Sse),
        Some("websocket") => Some(Transport::Websocket),
        Some("websocket-cached") => Some(Transport::WebsocketCached),
        Some("auto") => Some(Transport::Auto),
        _ => None,
    }
}

fn parse_packages(value: Option<&Value>) -> Result<Vec<PackageSource>, SettingsFailure> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let entries = value
        .as_array()
        .ok_or_else(|| SettingsFailure::new("packages must be an array"))?;
    entries
        .iter()
        .map(|entry| {
            package_source_from_value(entry)
                .map_err(|error| SettingsFailure::new(format!("Invalid package source: {error}")))
        })
        .collect()
}

fn packages_to_value(packages: &[PackageSource]) -> Value {
    Value::Array(
        packages
            .iter()
            .map(|package| js_json::to_value(package).expect("package source serializes"))
            .collect(),
    )
}

fn strings_to_value(values: &[JsString]) -> Value {
    Value::Array(values.iter().map(|value| Value::String(value.clone())).collect())
}

fn string_array(value: Option<&Value>) -> Option<Vec<JsString>> {
    let values = value?.as_array()?;
    values.iter().map(|value| value.as_string().cloned()).collect()
}

/// Settings enum values are ASCII literals. A string with unpaired units matches none of them,
/// so treating its failed UTF-8 conversion as "no literal" is exact, not lossy.
fn enum_literal(value: Option<&Value>) -> Option<String> {
    value?.as_string()?.to_utf8().ok()
}

fn nested_value<'a>(settings: &'a Settings, field: &str, nested_key: &str) -> Option<&'a Value> {
    settings
        .get(field)
        .and_then(Value::as_object)
        .and_then(|object| object.get(nested_key))
}

fn nested_number(settings: &Settings, field: &str, nested_key: &str) -> Option<f64> {
    nested_value(settings, field, nested_key).and_then(Value::as_f64)
}

fn nested_bool(settings: &Settings, field: &str, nested_key: &str) -> Option<bool> {
    nested_value(settings, field, nested_key).and_then(Value::as_bool)
}

fn nested_string<'a>(settings: &'a Settings, field: &str, nested_key: &str) -> Option<&'a JsString> {
    nested_value(settings, field, nested_key).and_then(Value::as_string)
}

fn set_value(settings: &mut Settings, key: &str, value: Value) {
    settings.insert(key.into(), value);
}

fn set_string(settings: &mut Settings, key: &str, value: impl Into<JsString>) {
    settings.insert(key.into(), Value::String(value.into()));
}

fn set_optional_string(settings: &mut Settings, key: &str, value: Option<&JsString>) {
    match value {
        Some(value) => set_string(settings, key, value),
        None => {
            settings.shift_remove(key);
        }
    }
}

fn set_bool(settings: &mut Settings, key: &str, value: bool) {
    settings.insert(key.into(), Value::Bool(value));
}

fn set_number(settings: &mut Settings, key: &str, value: f64) {
    settings.insert(key.into(), json_number(value));
}

fn set_string_array(settings: &mut Settings, key: &str, values: &[JsString]) {
    settings.insert(key.into(), strings_to_value(values));
}

fn set_optional_string_array(settings: &mut Settings, key: &str, values: Option<&[JsString]>) {
    match values {
        Some(values) => set_string_array(settings, key, values),
        None => {
            settings.shift_remove(key);
        }
    }
}

fn set_nested(settings: &mut Settings, field: &str, nested_key: &str, value: Value) {
    let mut object = settings
        .get(field)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    object.insert(nested_key.into(), value);
    settings.insert(field.into(), Value::Object(object));
}

fn set_number_nested(settings: &mut Settings, field: &str, nested_key: &str, value: f64) {
    set_nested(settings, field, nested_key, json_number(value));
}

/// `Math.max(min, Math.floor(value))`; `NaN` stays `NaN`.
fn js_floor_max(value: f64, min: f64) -> f64 {
    let floored = value.floor();
    if floored.is_nan() { floored } else { floored.max(min) }
}

/// `Math.max(min, Math.min(max, Math.floor(value)))`; `NaN` stays `NaN`.
fn js_floor_clamp(value: f64, min: f64, max: f64) -> f64 {
    let floored = value.floor();
    if floored.is_nan() {
        floored
    } else {
        floored.min(max).max(min)
    }
}

fn push_unique(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|existing| existing == value) {
        values.push(value.to_owned());
    }
}

fn push_nested(values: &mut Vec<(String, Vec<String>)>, field: &str, nested_key: &str) {
    if let Some((_, nested)) = values.iter_mut().find(|(name, _)| name == field) {
        push_unique(nested, nested_key);
    } else {
        values.push((field.to_owned(), vec![nested_key.to_owned()]));
    }
}
