//! npm/git package installation and resource resolution (`core/package-manager.ts`).
//!
//! Settings live behind [`PackageSettingsStore`] so this module does not port the
//! settings manager. `PackageSource` is the settings wire shape and must be shared
//! with that module.
//!
//! Deviations recorded in `docs/porting/traceability/package-manager.md`:
//! - `minimatch` comes from `pi_js_compat`; `glob` is implemented with `globset`
//!   plus a dot-skipping walk (deterministic sorted order).
//! - `semver` uses the `semver` crate with node-style bare-version ranges
//!   normalized to `=`/`>=` ranges; npm hyphen and `||` ranges are unsupported.
//! - Child processes inherit the process environment instead of copying
//!   `process.env`, which is the same set on every supported platform.

use std::collections::{BTreeMap, HashSet};
use std::future::Future;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::future::join_all;
use futures_util::stream::{self, StreamExt};
use pi_ai::request::BoxFuture;
use pi_js_compat::minimatch::{MinimatchOptions, minimatch};
use pi_js_compat::node_ignore::Ignore;
use pi_js_compat::{
    js_format,
    js_json::{JsValue, encode_node_utf8},
    js_regex::JsRegex,
};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;
use tokio::process::Child;

use crate::config;
use crate::core::output_guard::is_stdout_taken_over;
use crate::core::pi_manifest::{PiManifest, read_pi_manifest};
use crate::core::settings_manager::{Settings, SettingsManager, lock_settings_manager};
use crate::utils::child_process::node_spawn_error_message;
use crate::utils::git::{GitSource, parse_git_url};
use crate::utils::paths::{
    PathInputOptions, canonicalize_path, is_local_path, mark_path_ignored_by_cloud_sync, node_basename, node_dirname,
    node_extname, node_is_absolute, node_join, relative_path, resolve_path,
};
use pi_js_compat::js_json;
use pi_js_compat::js_json::JsString;

const NETWORK_TIMEOUT_MS: u64 = 10_000;
const UPDATE_CHECK_CONCURRENCY: usize = 4;
const GIT_UPDATE_CONCURRENCY: usize = 4;
const IGNORE_FILE_NAMES: [&str; 3] = [".gitignore", ".ignore", ".fdignore"];

/// `PackageManagerError` mirrors a thrown TypeScript `Error` message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageManagerError {
    message: JsString,
}

impl PackageManagerError {
    pub fn message(&self) -> &JsString {
        &self.message
    }

    pub fn new(message: impl Into<JsString>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for PackageManagerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&encode_node_utf8(&self.message))
    }
}

impl std::error::Error for PackageManagerError {}

impl From<std::io::Error> for PackageManagerError {
    fn from(error: std::io::Error) -> Self {
        Self::new(error.to_string())
    }
}

/// Resource scope (`SourceScope`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceScope {
    User,
    Project,
    Temporary,
}

impl SourceScope {
    pub fn installed(self) -> Option<InstalledScope> {
        match self {
            Self::User => Some(InstalledScope::User),
            Self::Project => Some(InstalledScope::Project),
            Self::Temporary => None,
        }
    }
}

/// Scope that owns an on-disk install (`Exclude<SourceScope, "temporary">`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstalledScope {
    User,
    Project,
}

impl From<InstalledScope> for SourceScope {
    fn from(scope: InstalledScope) -> Self {
        match scope {
            InstalledScope::User => Self::User,
            InstalledScope::Project => Self::Project,
        }
    }
}

/// Where a resource path came from (`PathMetadata`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathMetadata {
    pub source: JsString,
    pub scope: SourceScope,
    pub origin: MetadataOrigin,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_dir: Option<JsString>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MetadataOrigin {
    Package,
    TopLevel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedResource {
    pub path: JsString,
    pub enabled: bool,
    pub metadata: PathMetadata,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ResolvedPaths {
    pub extensions: Vec<ResolvedResource>,
    pub skills: Vec<ResolvedResource>,
    pub prompts: Vec<ResolvedResource>,
    pub themes: Vec<ResolvedResource>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissingSourceAction {
    Install,
    Skip,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProgressType {
    Start,
    Progress,
    Complete,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProgressAction {
    Install,
    Remove,
    Update,
    Clone,
    Pull,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProgressEvent {
    #[serde(rename = "type")]
    pub event_type: ProgressType,
    pub action: ProgressAction,
    pub source: JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<JsString>,
}

pub type ProgressCallback = Arc<dyn Fn(&ProgressEvent) + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageUpdate {
    pub source: JsString,
    pub display_name: JsString,
    #[serde(rename = "type")]
    pub update_type: PackageUpdateType,
    pub scope: InstalledScope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PackageUpdateType {
    Npm,
    Git,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfiguredPackage {
    pub source: JsString,
    pub scope: InstalledScope,
    pub filtered: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed_path: Option<JsString>,
}

pub use crate::core::settings_manager::{PackageFilter, PackageSource};

/// Settings snapshot consumed by package resolution.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PackageSettingsSnapshot {
    pub packages: Vec<PackageSource>,
    pub extensions: Vec<JsString>,
    pub skills: Vec<JsString>,
    pub prompts: Vec<JsString>,
    pub themes: Vec<JsString>,
}

impl PackageSettingsSnapshot {
    fn resource_entries(&self, resource_type: ResourceType) -> &[JsString] {
        match resource_type {
            ResourceType::Extensions => &self.extensions,
            ResourceType::Skills => &self.skills,
            ResourceType::Prompts => &self.prompts,
            ResourceType::Themes => &self.themes,
        }
    }
}

/// Settings manager surface used by the package manager.
pub trait PackageSettingsStore: Send + Sync {
    fn global_settings(&self) -> PackageSettingsSnapshot;
    fn project_settings(&self) -> PackageSettingsSnapshot;
    fn set_packages(&self, packages: Vec<PackageSource>);
    fn set_project_packages(&self, packages: Vec<PackageSource>);
    fn is_project_trusted(&self) -> bool;
    fn get_npm_command(&self) -> Option<Vec<JsString>>;
}

pub type MissingSourceHandler<'a> =
    Box<dyn Fn(JsString) -> BoxFuture<'a, Result<MissingSourceAction, PackageManagerError>> + Send + Sync + 'a>;
/// Adapter from the ported settings manager to [`PackageSettingsStore`].
///
/// The settings manager owns mutable state; the package manager only needs
/// `&self` access, so the adapter serializes access through a mutex.
pub struct SettingsManagerStore {
    manager: Arc<Mutex<SettingsManager>>,
}

impl SettingsManagerStore {
    pub fn new(manager: SettingsManager) -> Self {
        Self {
            manager: Arc::new(Mutex::new(manager)),
        }
    }

    /// Shares an existing settings manager, so reloads stay visible to its owner.
    pub fn from_shared(manager: Arc<Mutex<SettingsManager>>) -> Self {
        Self { manager }
    }

    pub fn with<T>(&self, operation: impl FnOnce(&mut SettingsManager) -> T) -> T {
        let mut manager = lock_settings_manager(&self.manager);
        operation(&mut manager)
    }
}

impl PackageSettingsStore for SettingsManagerStore {
    fn global_settings(&self) -> PackageSettingsSnapshot {
        self.with(|manager| snapshot_from_settings(&manager.get_global_settings()))
    }

    fn project_settings(&self) -> PackageSettingsSnapshot {
        self.with(|manager| snapshot_from_settings(&manager.get_project_settings()))
    }

    fn set_packages(&self, packages: Vec<PackageSource>) {
        self.with(|manager| manager.set_packages(packages));
    }

    fn set_project_packages(&self, packages: Vec<PackageSource>) {
        let _ = self.with(|manager| manager.set_project_packages(packages));
    }

    fn is_project_trusted(&self) -> bool {
        self.with(|manager| manager.is_project_trusted())
    }

    fn get_npm_command(&self) -> Option<Vec<JsString>> {
        self.with(|manager| manager.get_npm_command())
    }
}

fn snapshot_from_settings(settings: &Settings) -> PackageSettingsSnapshot {
    // Typed API: entries that do not match the wire shape are ignored instead of
    // reproducing JavaScript runtime type errors.
    let packages = settings
        .get("packages")
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default();
    PackageSettingsSnapshot {
        packages,
        extensions: string_list(settings, "extensions"),
        skills: string_list(settings, "skills"),
        prompts: string_list(settings, "prompts"),
        themes: string_list(settings, "themes"),
    }
}

fn string_list(settings: &Settings, key: &str) -> Vec<JsString> {
    settings
        .get(key)
        .and_then(JsValue::as_array)
        .and_then(|entries| entries.iter().map(|entry| entry.as_string().cloned()).collect())
        .unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NpmSource {
    pub spec: JsString,
    pub name: JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<JsString>,
    pub pinned: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocalSource {
    pub path: JsString,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ParsedSource {
    Npm(NpmSource),
    Git(GitSource),
    Local(LocalSource),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceType {
    Extensions,
    Skills,
    Prompts,
    Themes,
}

impl ResourceType {
    pub const ALL: [ResourceType; 4] = [
        ResourceType::Extensions,
        ResourceType::Skills,
        ResourceType::Prompts,
        ResourceType::Themes,
    ];

    fn manifest_entries(self, manifest: &PiManifest) -> Option<&Vec<JsString>> {
        match self {
            ResourceType::Extensions => manifest.extensions.as_ref(),
            ResourceType::Skills => manifest.skills.as_ref(),
            ResourceType::Prompts => manifest.prompts.as_ref(),
            ResourceType::Themes => manifest.themes.as_ref(),
        }
    }

    fn matches_file_name(self, name: &JsString) -> bool {
        match self {
            ResourceType::Extensions => name.ends_with(".ts") || name.ends_with(".js"),
            ResourceType::Skills | ResourceType::Prompts => name.ends_with(".md"),
            ResourceType::Themes => name.ends_with(".json"),
        }
    }

    fn directory_name(self) -> &'static str {
        match self {
            ResourceType::Extensions => "extensions",
            ResourceType::Skills => "skills",
            ResourceType::Prompts => "prompts",
            ResourceType::Themes => "themes",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillDiscoveryMode {
    Pi,
    Agents,
}

type ResourceMap = Vec<(JsString, ResourceEntry)>;

#[derive(Debug, Clone)]
struct ResourceEntry {
    metadata: PathMetadata,
    enabled: bool,
}

#[derive(Debug, Clone, Default)]
struct ResourceAccumulator {
    extensions: ResourceMap,
    skills: ResourceMap,
    prompts: ResourceMap,
    themes: ResourceMap,
}

impl ResourceAccumulator {
    fn target(&mut self, resource_type: ResourceType) -> &mut ResourceMap {
        match resource_type {
            ResourceType::Extensions => &mut self.extensions,
            ResourceType::Skills => &mut self.skills,
            ResourceType::Prompts => &mut self.prompts,
            ResourceType::Themes => &mut self.themes,
        }
    }

    fn into_paths(self) -> ResolvedPaths {
        ResolvedPaths {
            extensions: to_resolved(self.extensions),
            skills: to_resolved(self.skills),
            prompts: to_resolved(self.prompts),
            themes: to_resolved(self.themes),
        }
    }
}

fn to_resolved(entries: ResourceMap) -> Vec<ResolvedResource> {
    let mut resolved: Vec<ResolvedResource> = entries
        .into_iter()
        .map(|(path, entry)| ResolvedResource {
            path,
            enabled: entry.enabled,
            metadata: entry.metadata,
        })
        .collect();
    resolved.sort_by_key(|resource| resource_precedence_rank(&resource.metadata));
    let mut seen = HashSet::new();
    resolved.retain(|resource| seen.insert(canonicalize_path(&resource.path)));
    resolved
}

fn filter_patterns(filter: &PackageFilter, resource_type: ResourceType) -> Option<&Vec<JsString>> {
    match resource_type {
        ResourceType::Extensions => filter.extensions.as_ref(),
        ResourceType::Skills => filter.skills.as_ref(),
        ResourceType::Prompts => filter.prompts.as_ref(),
        ResourceType::Themes => filter.themes.as_ref(),
    }
}

fn resource_precedence_rank(metadata: &PathMetadata) -> u8 {
    if metadata.origin == MetadataOrigin::Package {
        return 4;
    }
    let scope_base = if metadata.scope == SourceScope::Project { 0 } else { 2 };
    scope_base + u8::from(metadata.source != "local")
}

fn is_offline_mode_enabled() -> bool {
    let Some(value) = pi_js_compat::process_env::get("PI_OFFLINE").filter(|value| !value.is_empty()) else {
        return false;
    };
    value == "1" || value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("yes")
}

fn to_posix_path(path: &JsString) -> JsString {
    path.replace(std::path::MAIN_SEPARATOR, "/")
}

fn home_dir() -> JsString {
    pi_js_compat::process_env::get("HOME").unwrap_or_default().into()
}

fn resolve_input(input: &JsString, base_dir: &JsString) -> Result<JsString, PackageManagerError> {
    let options = PathInputOptions {
        trim: true,
        home_dir: Some(home_dir()),
        ..PathInputOptions::default()
    };
    resolve_path(input, base_dir, &options).map_err(|error| PackageManagerError::new(error.message().clone()))
}

/// `getExtensionTempFolder`.
pub fn get_extension_temp_folder(agent_dir: &JsString) -> Result<JsString, PackageManagerError> {
    let temp_folder = node_join(node_join(agent_dir, "tmp"), "extensions");
    std::fs::create_dir_all(encode_node_utf8(&temp_folder))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(encode_node_utf8(&temp_folder), std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(temp_folder)
}

struct IgnoreState(Ignore);

impl IgnoreState {
    fn new(_root_dir: &JsString) -> Self {
        Self(Ignore::new())
    }

    fn add(&mut self, patterns: &[JsString]) {
        for pattern in patterns {
            self.0.add(pattern);
        }
    }

    fn ignores(&self, path: &JsString, is_dir: bool) -> bool {
        let path = if is_dir {
            js_format!("{}/", path)
        } else {
            path.to_owned()
        };
        self.0
            .ignores(path)
            .expect("discovery supplies a nonempty root-relative path")
    }
}

fn prefix_ignore_pattern(line: &JsString, prefix: &JsString) -> Option<JsString> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with('#') && !trimmed.starts_with("\\#") {
        return None;
    }

    let mut pattern = line.clone();
    let mut negated = false;
    if let Some(rest) = pattern.strip_prefix('!') {
        negated = true;
        pattern = rest;
    } else if let Some(rest) = pattern.strip_prefix("\\!") {
        pattern = rest;
    }
    if let Some(rest) = pattern.strip_prefix('/') {
        pattern = rest;
    }

    let prefixed = js_format!("{}{}", prefix, pattern);
    Some(if negated { js_format!("!{}", prefixed) } else { prefixed })
}

fn add_ignore_rules(state: &mut IgnoreState, dir: &JsString, root_dir: &JsString) {
    let relative_dir = relative_path(root_dir, dir).expect("resolved discovery paths");
    let prefix = if relative_dir.is_empty() {
        JsString::default()
    } else {
        js_format!("{}/", to_posix_path(&relative_dir))
    };

    for filename in IGNORE_FILE_NAMES {
        let ignore_path = node_join(dir, filename);
        let Ok(content) = read_text_file(&ignore_path) else {
            continue;
        };
        let patterns: Vec<JsString> = content
            .split('\n')
            .map(|line| line.strip_suffix('\r').unwrap_or(line))
            .filter_map(|line| prefix_ignore_pattern(&line, &prefix))
            .collect();
        if !patterns.is_empty() {
            state.add(&patterns);
        }
    }
}

fn is_pattern(entry: &JsString) -> bool {
    entry.starts_with('!')
        || entry.starts_with('+')
        || entry.starts_with('-')
        || entry.contains('*')
        || entry.contains('?')
}

fn is_override_pattern(entry: &JsString) -> bool {
    entry.starts_with('!') || entry.starts_with('+') || entry.starts_with('-')
}

fn has_glob_pattern(entry: &JsString) -> bool {
    entry.contains('*') || entry.contains('?')
}

fn split_patterns(entries: &[JsString]) -> (Vec<JsString>, Vec<JsString>) {
    let mut plain = Vec::new();
    let mut patterns = Vec::new();
    for entry in entries {
        if is_pattern(entry) {
            patterns.push(entry.clone());
        } else {
            plain.push(entry.clone());
        }
    }
    (plain, patterns)
}

fn minimatch_path(path: &JsString, pattern: &JsString) -> bool {
    minimatch(path, pattern, &MinimatchOptions::default())
}

fn read_dir_entries(dir: &JsString) -> Vec<std::fs::DirEntry> {
    let Ok(entries) = std::fs::read_dir(encode_node_utf8(dir)) else {
        return Vec::new();
    };
    entries.filter_map(Result::ok).collect()
}

/// Reads a visible directory entry, skipping dotfiles, `node_modules`, and
/// entries whose metadata cannot be read.
fn read_visible_entry(dir: &JsString, entry: &std::fs::DirEntry) -> Option<(JsString, JsString, std::fs::Metadata)> {
    let name = JsString::from(entry.file_name().to_string_lossy().as_ref());
    if name.starts_with('.') || name == "node_modules" {
        return None;
    }
    let path = node_join(dir, &name);
    let metadata = std::fs::metadata(encode_node_utf8(&path)).ok()?;
    Some((name, path, metadata))
}

fn collect_files(
    dir: &JsString,
    resource_type: ResourceType,
    skip_node_modules: bool,
    ignore: &mut IgnoreState,
    root_dir: &JsString,
) -> Vec<JsString> {
    let mut files = Vec::new();
    if !path_exists(dir) {
        return files;
    }
    add_ignore_rules(ignore, dir, root_dir);

    for entry in read_dir_entries(dir) {
        let name = JsString::from(entry.file_name().to_string_lossy().as_ref());
        if name.starts_with('.') {
            continue;
        }
        if skip_node_modules && name == "node_modules" {
            continue;
        }
        let full_path = node_join(dir, &name);
        let Ok(metadata) = std::fs::metadata(encode_node_utf8(&full_path)) else {
            continue;
        };
        let full_path = full_path;
        let rel_path = to_posix_path(&relative_path(root_dir, &full_path).expect("resolved discovery paths"));
        if ignore.ignores(&rel_path, metadata.is_dir()) {
            continue;
        }
        if metadata.is_dir() {
            files.extend(collect_files(
                &full_path,
                resource_type,
                skip_node_modules,
                ignore,
                root_dir,
            ));
        } else if metadata.is_file() && resource_type.matches_file_name(&name) {
            files.push(full_path);
        }
    }
    files
}

fn collect_skill_entries(
    dir: &JsString,
    mode: SkillDiscoveryMode,
    ignore: &mut IgnoreState,
    root_dir: &JsString,
) -> Vec<JsString> {
    let mut entries = Vec::new();
    if !path_exists(dir) {
        return entries;
    }
    add_ignore_rules(ignore, dir, root_dir);
    let dir_entries = read_dir_entries(dir);

    for entry in &dir_entries {
        if entry.file_name().to_string_lossy() != "SKILL.md" {
            continue;
        }
        let full_path = node_join(dir, JsString::from(entry.file_name().to_string_lossy().as_ref()));
        let Ok(metadata) = std::fs::metadata(encode_node_utf8(&full_path)) else {
            continue;
        };
        let full_path = full_path;
        let rel_path = to_posix_path(&relative_path(root_dir, &full_path).expect("resolved discovery paths"));
        if metadata.is_file() && !ignore.ignores(&rel_path, false) {
            entries.push(full_path);
            return entries;
        }
    }

    for entry in &dir_entries {
        let Some((name, full_path, metadata)) = read_visible_entry(dir, entry) else {
            continue;
        };
        let rel_path = to_posix_path(&relative_path(root_dir, &full_path).expect("resolved discovery paths"));
        if mode == SkillDiscoveryMode::Pi
            && dir == root_dir
            && metadata.is_file()
            && name.ends_with(".md")
            && !ignore.ignores(&rel_path, false)
        {
            entries.push(full_path);
            continue;
        }
        if !metadata.is_dir() {
            continue;
        }
        if ignore.ignores(&rel_path, true) {
            continue;
        }
        entries.extend(collect_skill_entries(&full_path, mode, ignore, root_dir));
    }
    entries
}

fn collect_auto_skill_entries(dir: &JsString, mode: SkillDiscoveryMode) -> Vec<JsString> {
    let mut ignore = IgnoreState::new(dir);
    collect_skill_entries(dir, mode, &mut ignore, dir)
}

fn collect_ancestor_skill_dirs(start_dir: &JsString, child: &str) -> Vec<JsString> {
    let mut dirs = Vec::new();
    let mut dir = resolve_input(start_dir, start_dir).unwrap_or_else(|_| start_dir.to_owned());
    loop {
        dirs.push(node_join(&dir, child));
        let parent = node_dirname(&dir);
        if parent == dir {
            break;
        }
        dir = parent;
    }
    dirs
}

fn collect_ancestor_agents_skill_dirs(start_dir: &JsString) -> Vec<JsString> {
    collect_ancestor_skill_dirs(start_dir, ".agents/skills")
}

fn collect_ancestor_tool_skill_dirs(start_dir: &JsString) -> Vec<JsString> {
    let mut dirs = Vec::new();
    let mut dir = resolve_input(start_dir, start_dir).unwrap_or_else(|_| start_dir.to_owned());
    loop {
        dirs.push(node_join(&dir, ".codex/skills"));
        dirs.push(node_join(&dir, ".claude/skills"));
        let parent = node_dirname(&dir);
        if parent == dir {
            break;
        }
        dir = parent;
    }
    dirs
}

fn collect_flat_files(dir: &JsString, suffix: &str) -> Vec<JsString> {
    let mut entries = Vec::new();
    if !path_exists(dir) {
        return entries;
    }
    let mut ignore = IgnoreState::new(dir);
    add_ignore_rules(&mut ignore, dir, dir);

    for entry in read_dir_entries(dir) {
        let Some((name, full_path, metadata)) = read_visible_entry(dir, &entry) else {
            continue;
        };
        let rel_path = to_posix_path(&relative_path(dir, &full_path).expect("resolved discovery paths"));
        if ignore.ignores(&rel_path, false) {
            continue;
        }
        if metadata.is_file() && name.ends_with(suffix) {
            entries.push(full_path);
        }
    }
    entries
}

fn collect_auto_prompt_entries(dir: &JsString) -> Vec<JsString> {
    collect_flat_files(dir, ".md")
}

fn collect_auto_theme_entries(dir: &JsString) -> Vec<JsString> {
    collect_flat_files(dir, ".json")
}

fn resolve_extension_entries(dir: &JsString) -> Option<Vec<JsString>> {
    let package_json_path = node_join(dir, "package.json");
    if path_exists(&package_json_path)
        && let Some(manifest) = read_pi_manifest(&package_json_path)
        && let Some(extensions) = manifest.extensions.as_ref().filter(|entries| !entries.is_empty())
    {
        let mut entries = Vec::new();
        for extension in extensions {
            let resolved = node_join(dir, extension);
            if path_exists(&resolved) {
                entries.push(resolved);
            }
        }
        if !entries.is_empty() {
            return Some(entries);
        }
    }

    let index_ts = node_join(dir, "index.ts");
    if path_exists(&index_ts) {
        return Some(vec![index_ts]);
    }
    let index_js = node_join(dir, "index.js");
    if path_exists(&index_js) {
        return Some(vec![index_js]);
    }
    None
}

fn collect_auto_extension_entries(dir: &JsString) -> Vec<JsString> {
    let mut entries = Vec::new();
    if !path_exists(dir) {
        return entries;
    }
    if let Some(root_entries) = resolve_extension_entries(dir) {
        return root_entries;
    }

    let mut ignore = IgnoreState::new(dir);
    add_ignore_rules(&mut ignore, dir, dir);

    for entry in read_dir_entries(dir) {
        let Some((name, full_path, metadata)) = read_visible_entry(dir, &entry) else {
            continue;
        };
        let rel_path = to_posix_path(&relative_path(dir, &full_path).expect("resolved discovery paths"));
        if ignore.ignores(&rel_path, metadata.is_dir()) {
            continue;
        }
        if metadata.is_file() && (name.ends_with(".ts") || name.ends_with(".js")) {
            entries.push(full_path);
        } else if metadata.is_dir()
            && let Some(resolved_entries) = resolve_extension_entries(&full_path)
        {
            entries.extend(resolved_entries);
        }
    }
    entries
}

fn collect_resource_files(dir: &JsString, resource_type: ResourceType) -> Vec<JsString> {
    match resource_type {
        ResourceType::Skills => collect_auto_skill_entries(dir, SkillDiscoveryMode::Pi),
        ResourceType::Extensions => collect_auto_extension_entries(dir),
        ResourceType::Prompts | ResourceType::Themes => {
            let mut ignore = IgnoreState::new(dir);
            collect_files(dir, resource_type, true, &mut ignore, dir)
        }
    }
}

fn skill_parent_paths(file_path: &JsString) -> Option<(JsString, JsString, JsString)> {
    let parent_dir = node_dirname(file_path);
    let parent_name = node_basename(&parent_dir);
    Some((parent_dir.clone(), to_posix_path(&parent_dir), parent_name))
}

fn matches_any_pattern(file_path: &JsString, patterns: &[JsString], base_dir: &JsString) -> bool {
    let rel = to_posix_path(&relative_path(base_dir, file_path).expect("resolved pattern paths"));
    let name = node_basename(file_path);
    let file_path_posix = to_posix_path(file_path);
    let is_skill_file = name == "SKILL.md";
    let parent = is_skill_file.then(|| skill_parent_paths(file_path)).flatten();

    patterns.iter().any(|pattern| {
        let normalized_pattern = to_posix_path(pattern);
        if minimatch_path(&rel, &normalized_pattern)
            || minimatch_path(&name, &normalized_pattern)
            || minimatch_path(&file_path_posix, &normalized_pattern)
        {
            return true;
        }
        let Some((parent_dir, parent_dir_posix, parent_name)) = parent.as_ref() else {
            return false;
        };
        let parent_rel = to_posix_path(&relative_path(base_dir, parent_dir).expect("resolved pattern paths"));
        minimatch_path(&parent_rel, &normalized_pattern)
            || minimatch_path(parent_name, &normalized_pattern)
            || minimatch_path(parent_dir_posix, &normalized_pattern)
    })
}

fn normalize_exact_pattern(pattern: &JsString) -> JsString {
    let normalized = pattern
        .strip_prefix("./")
        .or_else(|| pattern.strip_prefix(".\\"))
        .unwrap_or_else(|| pattern.clone());
    to_posix_path(&normalized)
}

fn matches_any_exact_pattern(file_path: &JsString, patterns: &[JsString], base_dir: &JsString) -> bool {
    if patterns.is_empty() {
        return false;
    }
    let rel = to_posix_path(&relative_path(base_dir, file_path).expect("resolved pattern paths"));
    let name = node_basename(file_path);
    let file_path_posix = to_posix_path(file_path);
    let is_skill_file = name == "SKILL.md";
    let parent = is_skill_file.then(|| skill_parent_paths(file_path)).flatten();

    patterns.iter().any(|pattern| {
        let normalized = normalize_exact_pattern(pattern);
        if normalized == rel || normalized == file_path_posix {
            return true;
        }
        let Some((parent_dir, parent_dir_posix, _)) = parent.as_ref() else {
            return false;
        };
        let parent_rel = to_posix_path(&relative_path(base_dir, parent_dir).expect("resolved pattern paths"));
        normalized == parent_rel || normalized == *parent_dir_posix
    })
}

fn get_override_patterns(entries: &[JsString]) -> Vec<JsString> {
    entries
        .iter()
        .filter(|entry| is_override_pattern(entry))
        .cloned()
        .collect()
}

fn is_enabled_by_overrides(file_path: &JsString, patterns: &[JsString], base_dir: &JsString) -> bool {
    let excludes: Vec<JsString> = get_override_patterns(patterns)
        .iter()
        .filter_map(|pattern| pattern.strip_prefix('!'))
        .collect();
    !matches_any_pattern(file_path, &excludes, base_dir)
}

/// Apply include/exclude/force patterns and return the enabled path set.
fn apply_patterns(all_paths: &[JsString], patterns: &[JsString], base_dir: &JsString) -> HashSet<JsString> {
    let mut includes = Vec::new();
    let mut excludes = Vec::new();
    let mut force_includes = Vec::new();
    let mut force_excludes = Vec::new();
    for pattern in patterns {
        if let Some(rest) = pattern.strip_prefix('+') {
            force_includes.push(rest.to_owned());
        } else if let Some(rest) = pattern.strip_prefix('-') {
            force_excludes.push(rest.to_owned());
        } else if let Some(rest) = pattern.strip_prefix('!') {
            excludes.push(rest.to_owned());
        } else {
            includes.push(pattern.clone());
        }
    }

    let mut result: Vec<JsString> = if includes.is_empty() {
        all_paths.to_vec()
    } else {
        all_paths
            .iter()
            .filter(|path| matches_any_pattern(path, &includes, base_dir))
            .cloned()
            .collect()
    };

    if !excludes.is_empty() {
        result.retain(|path| !matches_any_pattern(path, &excludes, base_dir));
    }

    if !force_includes.is_empty() {
        for path in all_paths {
            if !result.contains(path) && matches_any_exact_pattern(path, &force_includes, base_dir) {
                result.push(path.clone());
            }
        }
    }

    if !force_excludes.is_empty() {
        result.retain(|path| !matches_any_exact_pattern(path, &force_excludes, base_dir));
    }

    result.into_iter().collect()
}

fn apply_autoload_disabled_patterns(
    all_paths: &[JsString],
    patterns: &[JsString],
    base_dir: &JsString,
) -> Vec<(JsString, bool)> {
    let mut result: Vec<(JsString, bool)> = Vec::new();
    for pattern in patterns {
        let target = if ['+', '-', '!'].iter().any(|prefix| pattern.starts_with(*prefix)) {
            pattern.slice(1, pattern.len())
        } else {
            pattern.clone()
        };
        let enabled = !pattern.starts_with('-') && !pattern.starts_with('!');
        let exact = pattern.starts_with('+') || pattern.starts_with('-');
        for file_path in all_paths {
            let matched = if exact {
                matches_any_exact_pattern(file_path, std::slice::from_ref(&target), base_dir)
            } else {
                matches_any_pattern(file_path, std::slice::from_ref(&target), base_dir)
            };
            if !matched {
                continue;
            }
            match result.iter_mut().find(|(path, _)| path == file_path) {
                Some(entry) => entry.1 = enabled,
                None => result.push((file_path.clone(), enabled)),
            }
        }
    }
    result
}

/// node-semver `validRange` text for the range forms npm packages use.
fn node_version_range(version: Option<&JsString>) -> Option<JsString> {
    let version = version?;
    // Semver grammar is ASCII-only; unmatched units cannot form a valid range.
    let scalar = match version.to_utf8() {
        Ok(value) => value,
        Err(_) => return None,
    };
    normalize_version_range(&scalar).map(JsString::from)
}

fn normalize_version_range(version: &str) -> Option<String> {
    if version.is_empty() {
        return None;
    }
    let version = version.trim();
    if version.is_empty() || version == "*" || version.eq_ignore_ascii_case("x") {
        return Some("*".to_owned());
    }
    let normalized = version.strip_prefix('=').unwrap_or(version);
    let normalized = normalized.strip_prefix('v').unwrap_or(normalized);
    if normalized.contains("||") {
        let parts: Option<Vec<String>> = normalized
            .split("||")
            .map(|part| node_single_range(part.trim()))
            .collect();
        return Some(parts?.join("||"));
    }
    node_single_range(normalized)
}

fn node_single_range(normalized: &str) -> Option<String> {
    if Version::parse(normalized).is_ok() {
        return Some(normalized.to_owned());
    }
    if let Some((lower, upper)) = normalized.split_once(" - ") {
        return expand_hyphen(lower.trim(), upper.trim());
    }
    if let Some(rest) = normalized.strip_prefix('^') {
        return expand_caret(rest);
    }
    if let Some(rest) = normalized.strip_prefix('~') {
        return expand_tilde(rest);
    }
    if let Some(expanded) = expand_wildcard(normalized) {
        return Some(expanded);
    }
    let parts: Vec<&str> = normalized.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }
    let normalized_parts: Option<Vec<String>> = parts.iter().map(|part| normalize_comparator(part)).collect();
    Some(normalized_parts?.join(" "))
}

fn split_prerelease(text: &str) -> (&str, Option<&str>) {
    match text.find('-') {
        Some(index) => (&text[..index], Some(&text[index + 1..])),
        None => (text, None),
    }
}

fn parse_components(core: &str) -> Option<(Option<u64>, Option<u64>, Option<u64>)> {
    let parts: Vec<&str> = core.split('.').collect();
    if parts.is_empty() || parts.len() > 3 {
        return None;
    }
    let parse = |index: usize| -> Option<Option<u64>> {
        match parts.get(index) {
            None => Some(None),
            Some(part) if matches!(*part, "x" | "X" | "*") => Some(None),
            Some(part) => part.parse::<u64>().ok().map(Some),
        }
    };
    Some((parse(0)?, parse(1)?, parse(2)?))
}

fn version_text(major: u64, minor: u64, patch: u64, prerelease: Option<&str>) -> String {
    match prerelease {
        Some(prerelease) => format!("{major}.{minor}.{patch}-{prerelease}"),
        None => format!("{major}.{minor}.{patch}"),
    }
}

fn expand_caret(rest: &str) -> Option<String> {
    let (core, prerelease) = split_prerelease(rest);
    let (major, minor, patch) = parse_components(core)?;
    let Some(major) = major else {
        return Some("*".to_owned());
    };
    let lower = version_text(major, minor.unwrap_or(0), patch.unwrap_or(0), prerelease);
    let upper = if major > 0 {
        format!("{}.0.0-0", major + 1)
    } else {
        match (minor, patch) {
            (Some(minor), _) if minor > 0 => format!("0.{}.0-0", minor + 1),
            (Some(0), Some(patch)) => format!("0.0.{}-0", patch + 1),
            (Some(0), None) => "0.1.0-0".to_owned(),
            _ => "1.0.0-0".to_owned(),
        }
    };
    Some(format!(">={lower} <{upper}"))
}

fn expand_tilde(rest: &str) -> Option<String> {
    let (core, prerelease) = split_prerelease(rest);
    let (major, minor, patch) = parse_components(core)?;
    let Some(major) = major else {
        return Some("*".to_owned());
    };
    let lower = version_text(major, minor.unwrap_or(0), patch.unwrap_or(0), prerelease);
    let upper = match minor {
        Some(minor) => format!("{major}.{}.0-0", minor + 1),
        None => format!("{}.0.0-0", major + 1),
    };
    Some(format!(">={lower} <{upper}"))
}

fn expand_wildcard(text: &str) -> Option<String> {
    let (core, prerelease) = split_prerelease(text);
    let (major, minor, patch) = parse_components(core)?;
    let Some(major) = major else {
        return Some("*".to_owned());
    };
    match (minor, patch) {
        (Some(minor), None) => Some(format!(">={}.{minor}.0 <{major}.{}.0-0", major, minor + 1)),
        (None, _) => Some(format!(">={major}.0.0 <{}.0.0-0", major + 1)),
        (Some(_), Some(_)) => {
            let _ = prerelease;
            None
        }
    }
}

fn expand_hyphen(lower: &str, upper: &str) -> Option<String> {
    let (lower_core, lower_prerelease) = split_prerelease(lower);
    let (lower_major, lower_minor, lower_patch) = parse_components(lower_core)?;
    let lower_major = lower_major?;
    let lower_text = format!(
        ">={}",
        version_text(
            lower_major,
            lower_minor.unwrap_or(0),
            lower_patch.unwrap_or(0),
            lower_prerelease
        )
    );
    let (upper_core, upper_prerelease) = split_prerelease(upper);
    let (upper_major, upper_minor, upper_patch) = parse_components(upper_core)?;
    let upper_major = upper_major?;
    let upper_text = match (upper_minor, upper_patch) {
        (Some(minor), Some(patch)) => {
            format!("<={}", version_text(upper_major, minor, patch, upper_prerelease))
        }
        (Some(minor), None) => format!("<{upper_major}.{}.0-0", minor + 1),
        (None, _) => format!("<{}.0.0-0", upper_major + 1),
    };
    Some(format!("{lower_text} {upper_text}"))
}

fn normalize_comparator(token: &str) -> Option<String> {
    let (operator, rest) = [(">=", ">="), ("<=", "<="), (">", ">"), ("<", "<"), ("=", "=")]
        .iter()
        .find_map(|(prefix, operator)| token.strip_prefix(prefix).map(|rest| (*operator, rest)))
        .unwrap_or(("=", token));
    let (core, prerelease) = split_prerelease(rest);
    let (major, minor, patch) = parse_components(core)?;
    let Some(major) = major else {
        return Some("*".to_owned());
    };
    let minor = minor.unwrap_or(0);
    let patch = patch.unwrap_or(0);
    let version = version_text(major, minor, patch, prerelease);
    Some(match operator {
        "=" => format!("={version}"),
        other => format!("{other}{version}"),
    })
}

fn single_range_requirement(range: &str) -> Option<VersionReq> {
    if range == "*" {
        return VersionReq::parse("*").ok();
    }
    let parts: Vec<String> = range
        .split_whitespace()
        .map(normalize_comparator)
        .collect::<Option<Vec<_>>>()?;
    let joined = parts
        .iter()
        .flat_map(|part| part.split(", ").map(str::to_owned))
        .collect::<Vec<_>>()
        .join(", ");
    VersionReq::parse(&joined).ok()
}

fn node_range_matches(installed: &str, range: &str) -> bool {
    let installed = installed.strip_prefix('v').unwrap_or(installed);
    let Ok(version) = Version::parse(installed) else {
        return false;
    };
    range
        .split("||")
        .any(|part| single_range_requirement(part.trim()).is_some_and(|requirement| requirement.matches(&version)))
}

fn installed_version_satisfies(installed: &JsString, range: &JsString) -> bool {
    let (Ok(installed), Ok(range)) = (installed.to_utf8(), range.to_utf8()) else {
        return false;
    };
    node_range_matches(&installed, &range)
}

fn parse_native_semver(version: &JsString) -> Option<Version> {
    let normalized = version.strip_prefix('v').unwrap_or_else(|| version.clone());
    let scalar = match normalized.to_utf8() {
        Ok(value) => value,
        Err(_) => return None,
    };
    match Version::parse(&scalar) {
        Ok(version) => Some(version),
        Err(_) => None,
    }
}

fn max_satisfying(versions: &[JsString], range: &JsString) -> Option<JsString> {
    versions
        .iter()
        .filter(|version| installed_version_satisfies(version, range))
        .filter_map(|version| parse_native_semver(version).map(|parsed| (parsed, version)))
        .max_by(|left, right| left.0.cmp(&right.0))
        .map(|(_, version)| version.clone())
}

fn is_exact_npm_version(version: Option<&JsString>) -> bool {
    version.is_some_and(|version| parse_native_semver(version).is_some())
}

fn highest_version(versions: &[JsString]) -> Option<JsString> {
    versions
        .iter()
        .filter_map(|version| parse_native_semver(version).map(|parsed| (parsed, version)))
        .max_by(|left, right| left.0.cmp(&right.0))
        .map(|(_, version)| version.clone())
}

/// Recursive `glob` walk used for manifest entries: dot files are skipped.
fn glob_sync(pattern: &JsString, root: &JsString) -> Vec<JsString> {
    let pattern = pattern
        .strip_prefix("./")
        .or_else(|| pattern.strip_prefix(".\\"))
        .unwrap_or_else(|| pattern.clone());
    let mut matches = Vec::new();
    collect_glob_matches(root, root, &pattern, &mut matches);
    matches
}

fn collect_glob_matches(dir: &JsString, root: &JsString, pattern: &JsString, matches: &mut Vec<JsString>) {
    for entry in read_dir_entries(dir) {
        let name = JsString::from(entry.file_name().to_string_lossy().as_ref());
        if name.starts_with('.') {
            continue;
        }
        let full_path = node_join(dir, name);
        let relative = to_posix_path(&relative_path(root, &full_path).expect("absolute glob paths"));
        if minimatch_path(&relative, pattern) {
            matches.push(full_path.clone());
        }
        if std::fs::metadata(encode_node_utf8(&full_path)).is_ok_and(|metadata| metadata.is_dir()) {
            collect_glob_matches(&full_path, root, pattern, matches);
        }
    }
}

struct NpmCommand {
    command: JsString,
    args: Vec<JsString>,
}

type PackageUpdateTask<'a> = std::pin::Pin<Box<dyn Future<Output = Result<(), PackageManagerError>> + 'a>>;

struct ConfiguredUpdateSource {
    source: JsString,
    scope: InstalledScope,
}

struct NpmUpdateTarget {
    source: JsString,
    scope: InstalledScope,
    parsed: NpmSource,
}

struct GitUpdateTarget {
    source: JsString,
    scope: InstalledScope,
    parsed: GitSource,
}

/// Package manager backed by [`PackageSettingsStore`].
pub struct DefaultPackageManager {
    cwd: JsString,
    agent_dir: JsString,
    settings: Arc<dyn PackageSettingsStore>,
    global_npm_root: Mutex<Option<(JsString, JsString)>>,
    progress_callback: Mutex<Option<ProgressCallback>>,
}

impl DefaultPackageManager {
    pub fn new(cwd: &JsString, agent_dir: &JsString, settings: Arc<dyn PackageSettingsStore>) -> Self {
        Self {
            cwd: resolve_input(cwd, cwd).unwrap_or_else(|_| cwd.to_owned()),
            agent_dir: resolve_input(agent_dir, agent_dir).unwrap_or_else(|_| agent_dir.to_owned()),
            settings,
            global_npm_root: Mutex::new(None),
            progress_callback: Mutex::new(None),
        }
    }

    pub fn set_progress_callback(&self, callback: Option<ProgressCallback>) {
        *self.progress_callback.lock().unwrap() = callback;
    }

    fn emit_progress(&self, event: ProgressEvent) {
        let callback = self.progress_callback.lock().unwrap().clone();
        if let Some(callback) = callback {
            callback(&event);
        }
    }

    async fn with_progress<F>(
        &self,
        action: ProgressAction,
        source: &JsString,
        message: JsString,
        operation: F,
    ) -> Result<(), PackageManagerError>
    where
        F: Future<Output = Result<(), PackageManagerError>>,
    {
        self.emit_progress(ProgressEvent {
            event_type: ProgressType::Start,
            action,
            source: source.to_owned(),
            message: Some(message),
        });
        match operation.await {
            Ok(()) => {
                self.emit_progress(ProgressEvent {
                    event_type: ProgressType::Complete,
                    action,
                    source: source.to_owned(),
                    message: None,
                });
                Ok(())
            }
            Err(error) => {
                self.emit_progress(ProgressEvent {
                    event_type: ProgressType::Error,
                    action,
                    source: source.to_owned(),
                    message: Some(error.message.clone()),
                });
                Err(error)
            }
        }
    }

    pub fn add_source_to_settings(&self, source: &JsString, local: bool) -> Result<bool, PackageManagerError> {
        let scope = if local { SourceScope::Project } else { SourceScope::User };
        let current_settings = self.settings_for_scope(scope);
        let current_packages = current_settings.packages;
        let normalized_source = self.normalize_package_source_for_settings(source, scope)?;
        let match_index = current_packages
            .iter()
            .position(|existing| self.package_sources_match(existing, source, scope));

        if let Some(index) = match_index {
            let existing = &current_packages[index];
            if existing.source() == normalized_source {
                return Ok(false);
            }
            let mut next_packages = current_packages.clone();
            next_packages[index] = match existing {
                PackageSource::Name(_) => PackageSource::Name(normalized_source),
                PackageSource::Filter(filter) => PackageSource::Filter(PackageFilter {
                    source: normalized_source,
                    ..filter.clone()
                }),
            };
            self.set_packages_for_scope(scope, next_packages);
            return Ok(true);
        }

        let mut next_packages = current_packages;
        next_packages.push(PackageSource::Name(normalized_source));
        self.set_packages_for_scope(scope, next_packages);
        Ok(true)
    }

    pub fn remove_source_from_settings(&self, source: &JsString, local: bool) -> bool {
        let scope = if local { SourceScope::Project } else { SourceScope::User };
        let current_packages = self.settings_for_scope(scope).packages;
        let next_packages: Vec<PackageSource> = current_packages
            .iter()
            .filter(|existing| !self.package_sources_match(existing, source, scope))
            .cloned()
            .collect();
        if next_packages.len() == current_packages.len() {
            return false;
        }
        self.set_packages_for_scope(scope, next_packages);
        true
    }

    fn settings_for_scope(&self, scope: SourceScope) -> PackageSettingsSnapshot {
        if scope == SourceScope::Project {
            self.settings.project_settings()
        } else {
            self.settings.global_settings()
        }
    }

    fn set_packages_for_scope(&self, scope: SourceScope, packages: Vec<PackageSource>) {
        if scope == SourceScope::Project {
            self.settings.set_project_packages(packages);
        } else {
            self.settings.set_packages(packages);
        }
    }

    pub fn get_installed_path(&self, source: &JsString, scope: InstalledScope) -> Option<JsString> {
        let parsed = self.parse_source(source);
        let path = match &parsed {
            ParsedSource::Npm(npm) => self.get_npm_install_path(npm, scope.into()).ok()?,
            ParsedSource::Git(git) => self.get_git_install_path(git, scope.into()).ok()?,
            ParsedSource::Local(local) => {
                let base_dir = self.get_base_dir_for_scope(scope.into()).ok()?;
                resolve_input(&local.path, &base_dir).ok()?
            }
        };
        path_exists(&path).then_some(path)
    }

    pub fn list_configured_packages(&self) -> Vec<ConfiguredPackage> {
        let global = self.settings.global_settings();
        let project = self.settings.project_settings();
        let mut configured = Vec::new();
        for (settings, scope) in [(global, InstalledScope::User), (project, InstalledScope::Project)] {
            for package in settings.packages {
                let source = package.source().to_owned();
                configured.push(ConfiguredPackage {
                    installed_path: self.get_installed_path(&source, scope),
                    source,
                    scope,
                    filtered: package.filter().is_some(),
                });
            }
        }
        configured
    }

    pub async fn resolve(
        &self,
        on_missing: Option<MissingSourceHandler<'_>>,
    ) -> Result<ResolvedPaths, PackageManagerError> {
        let mut accumulator = ResourceAccumulator::default();
        let global_settings = self.settings.global_settings();
        let project_settings = self.settings.project_settings();

        let mut all_packages: Vec<(PackageSource, SourceScope)> = Vec::new();
        for package in &project_settings.packages {
            all_packages.push((package.clone(), SourceScope::Project));
        }
        for package in &global_settings.packages {
            all_packages.push((package.clone(), SourceScope::User));
        }
        let package_sources = self.dedupe_packages(all_packages);
        self.resolve_package_sources(&package_sources, &mut accumulator, on_missing.as_ref())
            .await?;

        let global_base_dir = self.agent_dir.clone();
        let project_base_dir = node_join(&self.cwd, &config::package_config().config_dir_name).clone();

        for resource_type in ResourceType::ALL {
            self.resolve_local_entries(
                project_settings.resource_entries(resource_type),
                resource_type,
                &mut accumulator,
                PathMetadata {
                    source: JsString::from("local"),
                    scope: SourceScope::Project,
                    origin: MetadataOrigin::TopLevel,
                    base_dir: None,
                },
                &project_base_dir,
            )?;
            self.resolve_local_entries(
                global_settings.resource_entries(resource_type),
                resource_type,
                &mut accumulator,
                PathMetadata {
                    source: JsString::from("local"),
                    scope: SourceScope::User,
                    origin: MetadataOrigin::TopLevel,
                    base_dir: None,
                },
                &global_base_dir,
            )?;
        }

        self.add_auto_discovered_resources(
            &mut accumulator,
            &global_settings,
            &project_settings,
            &global_base_dir,
            &project_base_dir,
        );
        Ok(accumulator.into_paths())
    }

    pub async fn resolve_extension_sources(
        &self,
        sources: &[JsString],
        local: bool,
        temporary: bool,
    ) -> Result<ResolvedPaths, PackageManagerError> {
        let mut accumulator = ResourceAccumulator::default();
        let scope = if temporary {
            SourceScope::Temporary
        } else if local {
            SourceScope::Project
        } else {
            SourceScope::User
        };
        let package_sources: Vec<(PackageSource, SourceScope)> = sources
            .iter()
            .map(|source| (PackageSource::Name(source.clone()), scope))
            .collect();
        self.resolve_package_sources(&package_sources, &mut accumulator, None)
            .await?;
        Ok(accumulator.into_paths())
    }

    pub async fn install(&self, source: &JsString, local: bool) -> Result<(), PackageManagerError> {
        let parsed = self.parse_source(source);
        let scope = if local { SourceScope::Project } else { SourceScope::User };
        self.assert_project_trusted_for_scope(scope)?;
        let message = js_format!("Installing {}...", source);
        self.with_progress(ProgressAction::Install, source, message, async {
            match &parsed {
                ParsedSource::Npm(npm) => self.install_npm(npm, scope, false).await,
                ParsedSource::Git(git) => self.install_git(git, scope).await,
                ParsedSource::Local(local_source) => {
                    let resolved = self.resolve_path(&local_source.path)?;
                    if !path_exists(&resolved) {
                        return Err(PackageManagerError::new(js_format!(
                            "Path does not exist: {}",
                            resolved
                        )));
                    }
                    Ok(())
                }
            }
        })
        .await
    }

    pub async fn install_and_persist(&self, source: &JsString, local: bool) -> Result<(), PackageManagerError> {
        self.install(source, local).await?;
        self.add_source_to_settings(source, local)?;
        Ok(())
    }

    pub async fn remove(&self, source: &JsString, local: bool) -> Result<(), PackageManagerError> {
        let parsed = self.parse_source(source);
        let scope = if local { SourceScope::Project } else { SourceScope::User };
        self.assert_project_trusted_for_scope(scope)?;
        let message = js_format!("Removing {}...", source);
        self.with_progress(ProgressAction::Remove, source, message, async {
            match &parsed {
                ParsedSource::Npm(npm) => self.uninstall_npm(npm, scope).await,
                ParsedSource::Git(git) => {
                    self.remove_git(git, scope);
                    Ok(())
                }
                ParsedSource::Local(_) => Ok(()),
            }
        })
        .await
    }

    pub async fn remove_and_persist(&self, source: &JsString, local: bool) -> Result<bool, PackageManagerError> {
        self.remove(source, local).await?;
        Ok(self.remove_source_from_settings(source, local))
    }

    pub async fn update(&self, source: Option<&JsString>) -> Result<(), PackageManagerError> {
        let global_settings = self.settings.global_settings();
        let project_settings = self.settings.project_settings();
        let identity = source.map(|source| self.get_package_identity(source, None));
        let mut matched = false;
        let mut update_sources = Vec::new();

        for (packages, scope) in [
            (&global_settings.packages, InstalledScope::User),
            (&project_settings.packages, InstalledScope::Project),
        ] {
            for package in packages {
                let source_str = package.source().to_owned();
                if let Some(identity) = identity.as_ref()
                    && self.get_package_identity(&source_str, Some(scope.into())) != *identity
                {
                    continue;
                }
                matched = true;
                update_sources.push(ConfiguredUpdateSource {
                    source: source_str,
                    scope,
                });
            }
        }

        if let Some(source) = source
            && !matched
        {
            let mut configured = global_settings.packages.clone();
            configured.extend(project_settings.packages.clone());
            return Err(PackageManagerError::new(
                self.build_no_matching_package_message(source, &configured),
            ));
        }

        self.update_configured_sources(update_sources).await
    }

    async fn update_configured_sources(&self, sources: Vec<ConfiguredUpdateSource>) -> Result<(), PackageManagerError> {
        if is_offline_mode_enabled() || sources.is_empty() {
            return Ok(());
        }

        let mut npm_candidates = Vec::new();
        let mut git_candidates = Vec::new();
        for entry in sources {
            match self.parse_source(&entry.source) {
                ParsedSource::Npm(parsed) => {
                    if !parsed.pinned {
                        npm_candidates.push(NpmUpdateTarget {
                            source: entry.source,
                            scope: entry.scope,
                            parsed,
                        });
                    }
                }
                ParsedSource::Git(parsed) => git_candidates.push(GitUpdateTarget {
                    source: entry.source,
                    scope: entry.scope,
                    parsed,
                }),
                ParsedSource::Local(_) => {}
            }
        }

        let npm_checks = npm_candidates
            .into_iter()
            .map(|entry| {
                move || async move {
                    let should_update = self.should_update_npm_source(&entry.parsed, entry.scope).await;
                    (entry, should_update)
                }
            })
            .collect::<Vec<_>>();
        let npm_results = run_with_concurrency(npm_checks, UPDATE_CHECK_CONCURRENCY).await;
        let mut user_updates = Vec::new();
        let mut project_updates = Vec::new();
        for (entry, should_update) in npm_results {
            if !should_update {
                continue;
            }
            match entry.scope {
                InstalledScope::User => user_updates.push(entry),
                InstalledScope::Project => project_updates.push(entry),
            }
        }

        let mut tasks: Vec<PackageUpdateTask<'_>> = Vec::new();
        if !user_updates.is_empty() {
            tasks.push(Box::pin(self.update_npm_batch(user_updates, InstalledScope::User)));
        }
        if !project_updates.is_empty() {
            tasks.push(Box::pin(
                self.update_npm_batch(project_updates, InstalledScope::Project),
            ));
        }
        if !git_candidates.is_empty() {
            let git_tasks = git_candidates
                .into_iter()
                .map(|entry| {
                    move || async move {
                        let message = js_format!("Updating {}...", entry.source);
                        self.with_progress(ProgressAction::Update, &entry.source, message, async {
                            self.update_git(&entry.parsed, entry.scope.into()).await
                        })
                        .await
                    }
                })
                .collect::<Vec<_>>();
            tasks.push(Box::pin(async move {
                run_with_concurrency(git_tasks, GIT_UPDATE_CONCURRENCY)
                    .await
                    .into_iter()
                    .collect::<Result<Vec<_>, _>>()
                    .map(|_| ())
            }));
        }

        for result in join_all(tasks).await {
            result?;
        }
        Ok(())
    }

    async fn should_update_npm_source(&self, source: &NpmSource, scope: InstalledScope) -> bool {
        let Ok(installed_path) = self.get_managed_npm_install_path(source, scope.into()) else {
            return true;
        };
        let installed_version = path_exists(&installed_path)
            .then(|| self.get_installed_npm_version(&installed_path))
            .flatten();
        let Some(installed_version) = installed_version else {
            return true;
        };
        let spec = if source.version.is_some() {
            &source.spec
        } else {
            &source.name
        };
        match self.get_latest_npm_version(spec, source.range.as_ref()).await {
            Ok(target) => target != installed_version,
            Err(_) => true,
        }
    }

    async fn update_npm_batch(
        &self,
        sources: Vec<NpmUpdateTarget>,
        scope: InstalledScope,
    ) -> Result<(), PackageManagerError> {
        if sources.is_empty() {
            return Ok(());
        }
        let scope_name = match scope {
            InstalledScope::User => "user",
            InstalledScope::Project => "project",
        };
        let source_label = if sources.len() == 1 {
            sources[0].source.clone()
        } else {
            js_format!("{} npm packages", scope_name)
        };
        let message = if sources.len() == 1 {
            js_format!("Updating {}...", sources[0].source)
        } else {
            js_format!("Updating {} npm packages...", scope_name)
        };
        let specs: Vec<JsString> = sources
            .iter()
            .map(|entry| {
                if entry.parsed.version.is_some() {
                    entry.parsed.spec.clone()
                } else {
                    js_format!("{}@latest", entry.parsed.name)
                }
            })
            .collect();
        self.with_progress(ProgressAction::Update, &source_label, message, async {
            self.install_npm_batch(&specs, scope).await
        })
        .await
    }

    async fn install_npm_batch(&self, specs: &[JsString], scope: InstalledScope) -> Result<(), PackageManagerError> {
        let install_root = self.get_npm_install_root(scope.into(), false)?;
        self.ensure_npm_project(&install_root)?;
        let args = self.get_npm_install_args(specs, &install_root);
        self.run_npm_command(&args, None).await
    }

    pub async fn check_for_available_updates(&self) -> Vec<PackageUpdate> {
        if is_offline_mode_enabled() {
            return Vec::new();
        }
        let global_settings = self.settings.global_settings();
        let project_settings = self.settings.project_settings();
        let mut all_packages = Vec::new();
        for package in project_settings.packages {
            all_packages.push((package, SourceScope::Project));
        }
        for package in global_settings.packages {
            all_packages.push((package, SourceScope::User));
        }
        let package_sources = self.dedupe_packages(all_packages);

        let checks = package_sources
            .into_iter()
            .filter_map(|(package, scope)| scope.installed().map(|scope| (package, scope)))
            .map(|(package, scope)| move || async move { self.check_package_update(&package, scope).await })
            .collect::<Vec<_>>();
        run_with_concurrency(checks, UPDATE_CHECK_CONCURRENCY)
            .await
            .into_iter()
            .flatten()
            .collect()
    }

    async fn check_package_update(&self, package: &PackageSource, scope: InstalledScope) -> Option<PackageUpdate> {
        let source = package.source().to_owned();
        match self.parse_source(&source) {
            ParsedSource::Local(_) => None,
            ParsedSource::Npm(parsed) => {
                if parsed.pinned {
                    return None;
                }
                let installed_path = self.get_npm_install_path(&parsed, scope.into()).ok()?;
                if !path_exists(&installed_path) {
                    return None;
                }
                self.npm_has_available_update(&parsed, &installed_path)
                    .await
                    .then_some(PackageUpdate {
                        source,
                        display_name: parsed.name,
                        update_type: PackageUpdateType::Npm,
                        scope,
                    })
            }
            ParsedSource::Git(parsed) => {
                if parsed.pinned {
                    return None;
                }
                let installed_path = self.get_git_install_path(&parsed, scope.into()).ok()?;
                if !path_exists(&installed_path) {
                    return None;
                }
                self.git_has_available_update(&installed_path)
                    .await
                    .then_some(PackageUpdate {
                        source,
                        display_name: js_format!("{}/{}", parsed.host, parsed.path),
                        update_type: PackageUpdateType::Git,
                        scope,
                    })
            }
        }
    }

    async fn resolve_package_sources(
        &self,
        sources: &[(PackageSource, SourceScope)],
        accumulator: &mut ResourceAccumulator,
        on_missing: Option<&MissingSourceHandler<'_>>,
    ) -> Result<(), PackageManagerError> {
        for (package, scope) in sources {
            let source_str = package.source().to_owned();
            let filter = package.filter();
            let delta_base = self.find_autoload_delta_base(package, *scope, sources);
            let resolved_source = delta_base
                .as_ref()
                .map_or_else(|| source_str.clone(), |base| base.0.clone());
            let resolved_scope = delta_base.as_ref().map_or(*scope, |base| base.1);
            let parsed = self.parse_source(&resolved_source);
            let mut metadata = PathMetadata {
                source: source_str,
                scope: *scope,
                origin: MetadataOrigin::Package,
                base_dir: None,
            };

            if let ParsedSource::Local(local) = &parsed {
                let base_dir = self.get_base_dir_for_scope(resolved_scope)?;
                self.resolve_local_extension_source(local, accumulator, filter, &mut metadata, &base_dir);
                continue;
            }

            let install_missing = |resolved_source: JsString, parsed: ParsedSource, scope: SourceScope| async move {
                if is_offline_mode_enabled() {
                    return Ok(false);
                }
                if let Some(handler) = on_missing {
                    let action = handler(resolved_source.clone()).await?;
                    if action == MissingSourceAction::Skip {
                        return Ok(false);
                    }
                    if action == MissingSourceAction::Error {
                        return Err(PackageManagerError::new(js_format!(
                            "Missing source: {}",
                            resolved_source
                        )));
                    }
                }
                self.install_parsed_source(&parsed, scope).await?;
                Ok(true)
            };

            match parsed.clone() {
                ParsedSource::Npm(npm) => {
                    let mut installed_path = self.get_npm_install_path(&npm, resolved_scope)?;
                    let needs_install = !path_exists(&installed_path)
                        || !self.installed_npm_matches_configured_version(&npm, &installed_path);
                    if needs_install {
                        let installed = install_missing(resolved_source, parsed.clone(), resolved_scope).await?;
                        if !installed {
                            continue;
                        }
                        installed_path = self.get_npm_install_path(&npm, resolved_scope)?;
                    }
                    metadata.base_dir = Some(installed_path.clone());
                    self.collect_package_resources(&installed_path, accumulator, filter, metadata);
                }
                ParsedSource::Git(git) => {
                    let installed_path = self.get_git_install_path(&git, resolved_scope)?;
                    if !path_exists(&installed_path) {
                        let installed = install_missing(resolved_source, parsed.clone(), resolved_scope).await?;
                        if !installed {
                            continue;
                        }
                    } else if resolved_scope == SourceScope::Temporary && !git.pinned && !is_offline_mode_enabled() {
                        self.refresh_temporary_git_source(&git, &resolved_source).await;
                    }
                    metadata.base_dir = Some(installed_path.clone());
                    self.collect_package_resources(&installed_path, accumulator, filter, metadata);
                }
                ParsedSource::Local(_) => {}
            }
        }
        Ok(())
    }

    fn find_autoload_delta_base(
        &self,
        package: &PackageSource,
        scope: SourceScope,
        sources: &[(PackageSource, SourceScope)],
    ) -> Option<(JsString, SourceScope)> {
        if scope != SourceScope::Project {
            return None;
        }
        let filter = package.filter()?;
        if filter.autoload != Some(false) {
            return None;
        }
        let identity = self.get_package_identity(&filter.source, Some(scope));
        sources.iter().find_map(|(entry, entry_scope)| {
            if *entry_scope != SourceScope::User {
                return None;
            }
            let entry_source = entry.source();
            (self.get_package_identity(entry_source, Some(SourceScope::User)) == identity)
                .then(|| (entry_source.to_owned(), SourceScope::User))
        })
    }

    fn resolve_local_extension_source(
        &self,
        source: &LocalSource,
        accumulator: &mut ResourceAccumulator,
        filter: Option<&PackageFilter>,
        metadata: &mut PathMetadata,
        base_dir: &JsString,
    ) {
        let Ok(resolved) = resolve_input(&source.path, base_dir) else {
            return;
        };
        let Ok(stat) = std::fs::metadata(encode_node_utf8(&resolved)) else {
            return;
        };
        if stat.is_file() {
            metadata.base_dir = Some(node_dirname(&resolved));
            add_resource(
                accumulator.target(ResourceType::Extensions),
                &resolved,
                metadata.clone(),
                true,
            );
            return;
        }
        if stat.is_dir() {
            metadata.base_dir = Some(resolved.clone());
            let has_resources = self.collect_package_resources(&resolved, accumulator, filter, metadata.clone());
            if !has_resources {
                add_resource(
                    accumulator.target(ResourceType::Extensions),
                    &resolved,
                    metadata.clone(),
                    true,
                );
            }
        }
    }

    async fn install_parsed_source(
        &self,
        parsed: &ParsedSource,
        scope: SourceScope,
    ) -> Result<(), PackageManagerError> {
        match parsed {
            ParsedSource::Npm(npm) => self.install_npm(npm, scope, scope == SourceScope::Temporary).await,
            ParsedSource::Git(git) => self.install_git(git, scope).await,
            ParsedSource::Local(_) => Ok(()),
        }
    }

    fn get_source_match_key_for_input(&self, source: &JsString) -> Result<JsString, PackageManagerError> {
        Ok(match self.parse_source(source) {
            ParsedSource::Npm(npm) => js_format!("npm:{}", npm.name),
            ParsedSource::Git(git) => js_format!("git:{}/{}", git.host, git.path),
            ParsedSource::Local(local) => js_format!("local:{}", self.resolve_path(&local.path)?),
        })
    }

    fn get_source_match_key_for_settings(
        &self,
        source: &JsString,
        scope: SourceScope,
    ) -> Result<JsString, PackageManagerError> {
        Ok(match self.parse_source(source) {
            ParsedSource::Npm(npm) => js_format!("npm:{}", npm.name),
            ParsedSource::Git(git) => js_format!("git:{}/{}", git.host, git.path),
            ParsedSource::Local(local) => {
                let base_dir = self.get_base_dir_for_scope(scope)?;
                js_format!("local:{}", resolve_input(&local.path, &base_dir)?)
            }
        })
    }

    fn build_no_matching_package_message(&self, source: &JsString, configured_packages: &[PackageSource]) -> JsString {
        match self.find_suggested_configured_source(source, configured_packages) {
            Some(suggestion) => {
                js_format!("No matching package found for {}. Did you mean {}?", source, suggestion)
            }
            None => js_format!("No matching package found for {}", source),
        }
    }

    fn find_suggested_configured_source(
        &self,
        source: &JsString,
        configured_packages: &[PackageSource],
    ) -> Option<JsString> {
        let trimmed_source = source.trim();
        let mut suggestions: Vec<JsString> = Vec::new();
        for package in configured_packages {
            let source_str = package.source().to_owned();
            match self.parse_source(&source_str) {
                ParsedSource::Npm(parsed) => {
                    if trimmed_source == parsed.name || trimmed_source == parsed.spec {
                        suggestions.push(source_str);
                    }
                }
                ParsedSource::Git(parsed) => {
                    let shorthand = js_format!("{}/{}", parsed.host, parsed.path);
                    let shorthand_with_ref = parsed
                        .reference
                        .as_ref()
                        .map(|reference| js_format!("{}@{}", shorthand, reference));
                    if trimmed_source == shorthand || shorthand_with_ref.is_some_and(|value| trimmed_source == value) {
                        suggestions.push(source_str);
                    }
                }
                ParsedSource::Local(_) => {}
            }
        }
        suggestions.into_iter().next()
    }

    fn package_sources_match(&self, existing: &PackageSource, input_source: &JsString, scope: SourceScope) -> bool {
        let left = self.get_source_match_key_for_settings(existing.source(), scope);
        let right = self.get_source_match_key_for_input(input_source);
        matches!((left, right), (Ok(left), Ok(right)) if left == right)
    }

    fn normalize_package_source_for_settings(
        &self,
        source: &JsString,
        scope: SourceScope,
    ) -> Result<JsString, PackageManagerError> {
        let ParsedSource::Local(local) = self.parse_source(source) else {
            return Ok(source.to_owned());
        };
        let base_dir = self.get_base_dir_for_scope(scope)?;
        let resolved = self.resolve_path(&local.path)?;
        let rel =
            relative_path(&base_dir, &resolved).map_err(|error| PackageManagerError::new(error.message().clone()))?;
        Ok(if rel.is_empty() { JsString::from(".") } else { rel })
    }

    pub fn parse_source(&self, source: &JsString) -> ParsedSource {
        if let Some(rest) = source.strip_prefix("npm:") {
            let spec = rest.trim().to_owned();
            let (name, version) = parse_npm_spec(&spec);
            return ParsedSource::Npm(NpmSource {
                spec,
                name,
                range: node_version_range(version.as_ref()),
                pinned: is_exact_npm_version(version.as_ref()),
                version,
            });
        }
        if is_local_path(source) {
            return ParsedSource::Local(LocalSource {
                path: source.to_owned(),
            });
        }
        if let Some(git) = parse_git_url(source) {
            return ParsedSource::Git(git);
        }
        ParsedSource::Local(LocalSource {
            path: source.to_owned(),
        })
    }

    fn installed_npm_matches_configured_version(&self, source: &NpmSource, installed_path: &JsString) -> bool {
        let Some(installed_version) = self.get_installed_npm_version(installed_path) else {
            return false;
        };
        match source.range.as_ref() {
            Some(range) => installed_version_satisfies(&installed_version, range),
            None => true,
        }
    }

    async fn npm_has_available_update(&self, source: &NpmSource, installed_path: &JsString) -> bool {
        if is_offline_mode_enabled() {
            return false;
        }
        let Some(installed_version) = self.get_installed_npm_version(installed_path) else {
            return false;
        };
        let spec = if source.version.is_some() {
            &source.spec
        } else {
            &source.name
        };
        match self.get_latest_npm_version(spec, source.range.as_ref()).await {
            Ok(target) => target != installed_version,
            Err(_) => false,
        }
    }

    fn get_installed_npm_version(&self, installed_path: &JsString) -> Option<JsString> {
        let package_json_path = node_join(installed_path, "package.json");
        if !path_exists(&package_json_path) {
            return None;
        }
        let content = std::fs::read(encode_node_utf8(&package_json_path)).ok()?;
        let parsed: JsValue = js_json::parse_lossless(String::from_utf8_lossy(&content).as_ref()).ok()?;
        parsed.get("version").and_then(JsValue::as_string).cloned()
    }

    async fn get_latest_npm_version(
        &self,
        package_spec: &JsString,
        range: Option<&JsString>,
    ) -> Result<JsString, PackageManagerError> {
        let npm = self.get_npm_command()?;
        let mut args = npm.args.clone();
        args.extend([
            JsString::from("view"),
            package_spec.to_owned(),
            JsString::from("version"),
            JsString::from("--json"),
        ]);
        let stdout = self
            .run_command_capture(&npm.command, &args, Some(&self.cwd), Some(NETWORK_TIMEOUT_MS), &[])
            .await?;
        let raw = stdout.trim();
        if raw.is_empty() {
            return Err(PackageManagerError::new("Empty response from npm view"));
        }
        let parsed: JsValue =
            js_json::parse_lossless(raw).map_err(|error| PackageManagerError::new(error.message().clone()))?;
        if let Some(version) = parsed.as_string() {
            return Ok(version.to_owned());
        }
        if let Some(entries) = parsed.as_array() {
            let versions: Vec<JsString> = entries
                .iter()
                .filter_map(JsValue::as_string)
                .filter(|version| !version.is_empty())
                .cloned()
                .collect();
            let latest = match range {
                Some(range) => max_satisfying(&versions, range),
                None => highest_version(&versions),
            };
            if let Some(latest) = latest {
                return Ok(latest);
            }
        }
        Err(PackageManagerError::new("Unexpected response from npm view"))
    }

    async fn git_has_available_update(&self, installed_path: &JsString) -> bool {
        if is_offline_mode_enabled() {
            return false;
        }
        let local_head = self
            .run_command_capture(
                &"git".into(),
                &[JsString::from("rev-parse"), JsString::from("HEAD")],
                Some(installed_path),
                Some(NETWORK_TIMEOUT_MS),
                &[],
            )
            .await;
        let remote_head = self.get_remote_git_head(installed_path).await;
        match (local_head, remote_head) {
            (Ok(local), Ok(remote)) => local.trim() != remote.trim(),
            _ => false,
        }
    }

    async fn get_remote_git_head(&self, installed_path: &JsString) -> Result<JsString, PackageManagerError> {
        if let Some(upstream_ref) = self.get_git_upstream_ref(installed_path).await {
            let remote_head = self
                .run_git_remote_command(
                    installed_path,
                    &[JsString::from("ls-remote"), JsString::from("origin"), upstream_ref],
                )
                .await?;
            if let Some(commit) = first_remote_commit(&remote_head) {
                return Ok(commit);
            }
        }
        let remote_head = self
            .run_git_remote_command(
                installed_path,
                &[
                    JsString::from("ls-remote"),
                    JsString::from("origin"),
                    JsString::from("HEAD"),
                ],
            )
            .await?;
        first_remote_head(&remote_head).ok_or_else(|| PackageManagerError::new("Failed to determine remote HEAD"))
    }

    async fn get_local_git_update_target(
        &self,
        installed_path: &JsString,
    ) -> Result<GitUpdateTargetSpec, PackageManagerError> {
        let upstream = self
            .run_command_capture(
                &"git".into(),
                &[
                    JsString::from("rev-parse"),
                    JsString::from("--abbrev-ref"),
                    JsString::from("@{upstream}"),
                ],
                Some(installed_path),
                Some(NETWORK_TIMEOUT_MS),
                &[],
            )
            .await;
        if let Ok(upstream) = upstream {
            let trimmed_upstream = upstream.trim();
            if let Some(branch) = trimmed_upstream.strip_prefix("origin/") {
                if branch.is_empty() {
                    return Err(PackageManagerError::new("Missing upstream branch name"));
                }
                let head = self
                    .run_command_capture(
                        &"git".into(),
                        &[JsString::from("rev-parse"), JsString::from("@{upstream}")],
                        Some(installed_path),
                        Some(NETWORK_TIMEOUT_MS),
                        &[],
                    )
                    .await?;
                let _ = head;
                return Ok(GitUpdateTargetSpec {
                    reference: JsString::from("@{upstream}"),
                    fetch_args: vec![
                        JsString::from("fetch"),
                        JsString::from("--prune"),
                        JsString::from("--no-tags"),
                        JsString::from("origin"),
                        js_format!("+refs/heads/{}:refs/remotes/origin/{}", branch, branch),
                    ],
                });
            }
        }

        let _ = self
            .run_command(
                &"git".into(),
                &[
                    JsString::from("remote"),
                    JsString::from("set-head"),
                    JsString::from("origin"),
                    JsString::from("-a"),
                ],
                Some(installed_path),
            )
            .await;
        let head = self
            .run_command_capture(
                &"git".into(),
                &[JsString::from("rev-parse"), JsString::from("origin/HEAD")],
                Some(installed_path),
                Some(NETWORK_TIMEOUT_MS),
                &[],
            )
            .await?;
        let origin_head_ref = self
            .run_command_capture(
                &"git".into(),
                &[
                    JsString::from("symbolic-ref"),
                    JsString::from("refs/remotes/origin/HEAD"),
                ],
                Some(installed_path),
                Some(NETWORK_TIMEOUT_MS),
                &[],
            )
            .await
            .unwrap_or_default();
        let branch = origin_head_ref
            .trim()
            .strip_prefix("refs/remotes/origin/")
            .unwrap_or("")
            .to_owned();
        if branch.is_empty() {
            let _ = head;
            return Ok(GitUpdateTargetSpec {
                reference: JsString::from("origin/HEAD"),
                fetch_args: vec![
                    JsString::from("fetch"),
                    JsString::from("--prune"),
                    JsString::from("--no-tags"),
                    JsString::from("origin"),
                    JsString::from("+HEAD:refs/remotes/origin/HEAD"),
                ],
            });
        }
        let _ = head;
        Ok(GitUpdateTargetSpec {
            reference: JsString::from("origin/HEAD"),
            fetch_args: vec![
                JsString::from("fetch"),
                JsString::from("--prune"),
                JsString::from("--no-tags"),
                JsString::from("origin"),
                js_format!("+refs/heads/{}:refs/remotes/origin/{}", branch, branch),
            ],
        })
    }

    async fn get_git_upstream_ref(&self, installed_path: &JsString) -> Option<JsString> {
        let upstream = self
            .run_command_capture(
                &"git".into(),
                &[
                    JsString::from("rev-parse"),
                    JsString::from("--abbrev-ref"),
                    JsString::from("@{upstream}"),
                ],
                Some(installed_path),
                Some(NETWORK_TIMEOUT_MS),
                &[],
            )
            .await
            .ok()?;
        let branch = upstream.trim().strip_prefix("origin/")?;
        (!branch.is_empty()).then(|| js_format!("refs/heads/{}", branch))
    }

    async fn run_git_remote_command(
        &self,
        installed_path: &JsString,
        args: &[JsString],
    ) -> Result<JsString, PackageManagerError> {
        self.run_command_capture(
            &"git".into(),
            args,
            Some(installed_path),
            Some(NETWORK_TIMEOUT_MS),
            &[(JsString::from("GIT_TERMINAL_PROMPT"), JsString::from("0"))],
        )
        .await
    }

    /// Unique identity ignoring version/ref (`getPackageIdentity`).
    pub fn get_package_identity(&self, source: &JsString, scope: Option<SourceScope>) -> JsString {
        match self.parse_source(source) {
            ParsedSource::Npm(npm) => js_format!("npm:{}", npm.name),
            ParsedSource::Git(git) => js_format!("git:{}/{}", git.host, git.path),
            ParsedSource::Local(local) => match scope {
                Some(scope) => self
                    .get_base_dir_for_scope(scope)
                    .and_then(|base_dir| resolve_input(&local.path, &base_dir))
                    .map_or_else(
                        |_| js_format!("local:{}", local.path),
                        |path| js_format!("local:{}", path),
                    ),
                None => self.resolve_path(&local.path).map_or_else(
                    |_| js_format!("local:{}", local.path),
                    |path| js_format!("local:{}", path),
                ),
            },
        }
    }

    fn dedupe_packages(&self, packages: Vec<(PackageSource, SourceScope)>) -> Vec<(PackageSource, SourceScope)> {
        let mut result: Vec<(PackageSource, SourceScope)> = Vec::new();
        let mut seen: BTreeMap<JsString, usize> = BTreeMap::new();
        for (package, scope) in packages {
            let identity = self.get_package_identity(package.source(), Some(scope));
            let Some(index) = seen.get(&identity).copied() else {
                seen.insert(identity, result.len());
                result.push((package, scope));
                continue;
            };
            let existing = &result[index];
            if existing.1 == SourceScope::Project && scope == SourceScope::User {
                if existing.0.filter().and_then(|filter| filter.autoload) == Some(false) {
                    result.push((package, scope));
                }
            } else if scope == SourceScope::Project {
                result[index] = (package, scope);
            }
        }
        result
    }

    fn assert_project_trusted_for_scope(&self, scope: SourceScope) -> Result<(), PackageManagerError> {
        if scope == SourceScope::Project && !self.settings.is_project_trusted() {
            return Err(PackageManagerError::new(
                "Project is not trusted; refusing to access project package storage",
            ));
        }
        Ok(())
    }

    fn get_npm_command(&self) -> Result<NpmCommand, PackageManagerError> {
        let Some(configured) = self.settings.get_npm_command() else {
            return Ok(NpmCommand {
                command: JsString::from("npm"),
                args: Vec::new(),
            });
        };
        if configured.is_empty() {
            return Ok(NpmCommand {
                command: JsString::from("npm"),
                args: Vec::new(),
            });
        }
        let command = configured[0].clone();
        if command.is_empty() {
            return Err(PackageManagerError::new(
                "Invalid npmCommand: first array entry must be a non-empty command",
            ));
        }
        Ok(NpmCommand {
            command,
            args: configured[1..].to_vec(),
        })
    }

    fn get_package_manager_name(&self) -> Result<JsString, PackageManagerError> {
        let npm = self.get_npm_command()?;
        let mut command_parts = vec![npm.command];
        command_parts.extend(npm.args);
        let package_manager_command = command_parts.iter().rposition(|part| part == "--").map_or_else(
            || command_parts[0].clone(),
            |index| command_parts.get(index + 1).cloned().unwrap_or_default(),
        );
        Ok(package_manager_basename(&package_manager_command))
    }

    async fn run_npm_command(&self, args: &[JsString], cwd: Option<&JsString>) -> Result<(), PackageManagerError> {
        let npm = self.get_npm_command()?;
        let mut full_args = npm.args;
        full_args.extend(args.iter().cloned());
        self.run_command(&npm.command, &full_args, cwd).await
    }

    fn get_git_dependency_install_args(&self) -> Vec<JsString> {
        let configured = self.settings.get_npm_command();
        if configured.is_some_and(|command| !command.is_empty()) {
            return vec![JsString::from("install")];
        }
        vec![JsString::from("install"), JsString::from("--omit=dev")]
    }

    fn run_npm_command_sync(&self, args: &[JsString]) -> Result<JsString, PackageManagerError> {
        let npm = self.get_npm_command()?;
        let mut full_args = npm.args;
        full_args.extend(args.iter().cloned());
        self.run_command_sync(&npm.command, &full_args)
    }

    fn get_npm_install_args(&self, specs: &[JsString], install_root: &JsString) -> Vec<JsString> {
        let Ok(package_manager_name) = self.get_package_manager_name() else {
            return Vec::new();
        };
        if package_manager_name == "bun" {
            let mut args = vec![JsString::from("install")];
            args.extend(specs.iter().cloned());
            args.extend([
                JsString::from("--cwd"),
                install_root.to_owned(),
                JsString::from("--omit=peer"),
            ]);
            return args;
        }
        if package_manager_name == "pnpm" {
            let mut args = vec![JsString::from("install")];
            args.extend(specs.iter().cloned());
            args.extend([
                JsString::from("--prefix"),
                install_root.to_owned(),
                JsString::from("--config.auto-install-peers=false"),
                JsString::from("--config.strict-peer-dependencies=false"),
                JsString::from("--config.strict-dep-builds=false"),
            ]);
            return args;
        }
        let mut args = vec![JsString::from("install")];
        args.extend(specs.iter().cloned());
        args.extend([
            JsString::from("--prefix"),
            install_root.to_owned(),
            JsString::from("--legacy-peer-deps"),
        ]);
        args
    }

    async fn install_npm(
        &self,
        source: &NpmSource,
        scope: SourceScope,
        temporary: bool,
    ) -> Result<(), PackageManagerError> {
        let install_root = self.get_npm_install_root(scope, temporary)?;
        self.ensure_npm_project(&install_root)?;
        let args = self.get_npm_install_args(std::slice::from_ref(&source.spec), &install_root);
        self.run_npm_command(&args, None).await
    }

    async fn uninstall_npm(&self, source: &NpmSource, scope: SourceScope) -> Result<(), PackageManagerError> {
        let install_root = self.get_npm_install_root(scope, false)?;
        if !path_exists(&install_root) {
            return Ok(());
        }
        let package_manager_name = self.get_package_manager_name()?;
        if package_manager_name == "bun" {
            return self
                .run_npm_command(
                    &[
                        JsString::from("uninstall"),
                        source.name.clone(),
                        JsString::from("--cwd"),
                        install_root,
                    ],
                    None,
                )
                .await;
        }
        let mut args = vec![
            JsString::from("uninstall"),
            source.name.clone(),
            JsString::from("--prefix"),
            install_root,
        ];
        if package_manager_name != "pnpm" {
            args.push(JsString::from("--legacy-peer-deps"));
        }
        self.run_npm_command(&args, None).await
    }

    async fn install_git(&self, source: &GitSource, scope: SourceScope) -> Result<(), PackageManagerError> {
        let target_dir = self.get_git_install_path(source, scope)?;
        if path_exists(&target_dir) {
            if let Some(reference) = &source.reference {
                return self
                    .ensure_git_ref(
                        &target_dir,
                        &[JsString::from("fetch"), JsString::from("origin"), reference.clone()],
                        &"FETCH_HEAD".into(),
                    )
                    .await;
            }
            let target = self.get_local_git_update_target(&target_dir).await?;
            return self
                .ensure_git_ref(&target_dir, &target.fetch_args, &target.reference)
                .await;
        }
        let git_root = self.get_git_install_root(scope)?;
        if let Some(git_root) = &git_root {
            self.ensure_git_ignore(git_root)?;
        }
        std::fs::create_dir_all(encode_node_utf8(&node_dirname(&target_dir)))?;
        let _ = std::fs::remove_file(encode_node_utf8(&self.get_git_update_marker_path(&target_dir)));

        let result = async {
            self.run_command(
                &"git".into(),
                &[JsString::from("clone"), source.repo.clone(), target_dir.clone()],
                None,
            )
            .await?;
            if let Some(reference) = &source.reference {
                self.run_command(
                    &"git".into(),
                    &[JsString::from("checkout"), reference.clone()],
                    Some(&target_dir),
                )
                .await?;
            }
            let package_json_path = node_join(&target_dir, "package.json");
            if path_exists(&package_json_path) {
                let args = self.get_git_dependency_install_args();
                self.run_npm_command(&args, Some(&target_dir)).await?;
            }
            Ok(())
        }
        .await;

        if let Err(error) = result {
            remove_dir_all_if_exists(&target_dir);
            self.prune_empty_git_parents(&target_dir, git_root.as_ref());
            return Err(error);
        }
        Ok(())
    }

    async fn update_git(&self, source: &GitSource, scope: SourceScope) -> Result<(), PackageManagerError> {
        let target_dir = self.get_git_install_path(source, scope)?;
        if !path_exists(&target_dir) {
            return self.install_git(source, scope).await;
        }
        if let Some(reference) = &source.reference {
            return self
                .ensure_git_ref(
                    &target_dir,
                    &[JsString::from("fetch"), JsString::from("origin"), reference.clone()],
                    &"FETCH_HEAD".into(),
                )
                .await;
        }
        let target = self.get_local_git_update_target(&target_dir).await?;
        self.ensure_git_ref(&target_dir, &target.fetch_args, &target.reference)
            .await
    }

    fn has_missing_git_dependencies(&self, target_dir: &JsString) -> bool {
        let package_json_path = node_join(target_dir, "package.json");
        if !path_exists(&package_json_path) {
            return false;
        }
        let Ok(content) = std::fs::read(encode_node_utf8(&package_json_path)) else {
            return false;
        };
        let Ok(manifest) = js_json::parse_lossless(String::from_utf8_lossy(&content).as_ref()) else {
            return false;
        };
        let Some(JsValue::Object(dependencies)) = manifest.get("dependencies") else {
            return false;
        };
        let node_modules_dir = node_join(target_dir, "node_modules");
        dependencies.keys().any(|name| {
            let dependency_path = node_join(&node_modules_dir, name);
            let dependency_path = dependency_path;
            let node_modules_prefix = js_format!("{}/", node_modules_dir.clone());
            if !dependency_path.starts_with(&node_modules_prefix) {
                return false;
            }
            !path_exists(&dependency_path)
        })
    }

    async fn repair_missing_git_dependencies(&self, target_dir: &JsString) -> Result<(), PackageManagerError> {
        if !self.has_missing_git_dependencies(target_dir) {
            return Ok(());
        }
        let args = self.get_git_dependency_install_args();
        self.run_npm_command(&args, Some(target_dir)).await
    }

    fn get_git_update_marker_path(&self, target_dir: &JsString) -> JsString {
        let parent = node_dirname(target_dir);
        let name = node_basename(target_dir);
        node_join(parent, js_format!(".{}.pi-update-incomplete", name))
    }

    async fn clean_and_install_git_dependencies(
        &self,
        target_dir: &JsString,
        marker_path: &JsString,
    ) -> Result<(), PackageManagerError> {
        if let Err(error) = self
            .run_command(
                &"git".into(),
                &[JsString::from("clean"), JsString::from("-fdx")],
                Some(target_dir),
            )
            .await
        {
            let _ = self.repair_missing_git_dependencies(target_dir).await;
            return Err(error);
        }

        let package_json_path = node_join(target_dir, "package.json");
        if path_exists(&package_json_path) {
            let args = self.get_git_dependency_install_args();
            self.run_npm_command(&args, Some(target_dir)).await?;
        }
        let _ = std::fs::remove_file(encode_node_utf8(marker_path));
        Ok(())
    }

    async fn ensure_git_ref(
        &self,
        target_dir: &JsString,
        fetch_args: &[JsString],
        reference: &JsString,
    ) -> Result<(), PackageManagerError> {
        self.run_command(&"git".into(), fetch_args, Some(target_dir)).await?;

        let local_head = self
            .run_command_capture(
                &"git".into(),
                &[JsString::from("rev-parse"), JsString::from("HEAD")],
                Some(target_dir),
                Some(NETWORK_TIMEOUT_MS),
                &[],
            )
            .await?;
        let commit_ref = reference.clone() + "^{commit}";
        let target_head = self
            .run_command_capture(
                &"git".into(),
                &[JsString::from("rev-parse"), commit_ref.clone()],
                Some(target_dir),
                Some(NETWORK_TIMEOUT_MS),
                &[],
            )
            .await?;
        let marker_path = self.get_git_update_marker_path(target_dir);
        if local_head.trim() == target_head.trim() {
            if path_exists(&marker_path) {
                return self.clean_and_install_git_dependencies(target_dir, &marker_path).await;
            }
            return self.repair_missing_git_dependencies(target_dir).await;
        }

        std::fs::write(encode_node_utf8(&marker_path), "")?;
        self.run_command(
            &"git".into(),
            &[JsString::from("reset"), JsString::from("--hard"), commit_ref],
            Some(target_dir),
        )
        .await?;
        self.clean_and_install_git_dependencies(target_dir, &marker_path).await
    }

    async fn refresh_temporary_git_source(&self, source: &GitSource, source_str: &JsString) {
        if is_offline_mode_enabled() {
            return;
        }
        let message = js_format!("Refreshing {}...", source_str);
        let _ = self
            .with_progress(ProgressAction::Pull, source_str, message, async {
                self.update_git(source, SourceScope::Temporary).await
            })
            .await;
    }

    fn remove_git(&self, source: &GitSource, scope: SourceScope) {
        if let Ok(target_dir) = self.get_git_install_path(source, scope) {
            remove_dir_all_if_exists(&target_dir);
            let _ = std::fs::remove_file(encode_node_utf8(&self.get_git_update_marker_path(&target_dir)));
            let git_root = self.get_git_install_root(scope).ok().flatten();
            self.prune_empty_git_parents(&target_dir, git_root.as_ref());
        }
    }

    fn prune_empty_git_parents(&self, target_dir: &JsString, install_root: Option<&JsString>) {
        let Some(install_root) = install_root else {
            return;
        };
        let resolved_root = resolve_input(install_root, install_root).unwrap_or_else(|_| install_root.to_owned());
        let mut current = node_dirname(target_dir);
        while current.starts_with(&resolved_root) && current != resolved_root {
            if !path_exists(&current) {
                current = node_dirname(&current);
                continue;
            }
            let empty = std::fs::read_dir(encode_node_utf8(&current))
                .map(|mut entries| entries.next().is_none())
                .unwrap_or(false);
            if !empty {
                break;
            }
            if std::fs::remove_dir_all(encode_node_utf8(&current)).is_err() {
                break;
            }
            current = node_dirname(&current);
        }
    }

    fn ensure_npm_project(&self, install_root: &JsString) -> Result<(), PackageManagerError> {
        if !path_exists(install_root) {
            std::fs::create_dir_all(encode_node_utf8(install_root))?;
        }
        mark_path_ignored_by_cloud_sync(install_root);
        self.ensure_git_ignore(install_root)?;
        let package_json_path = node_join(install_root, "package.json");
        if !path_exists(&package_json_path) {
            let package_json = pi_js_compat::js_json!({"name": "pi-extensions", "private": true});
            std::fs::write(
                encode_node_utf8(&package_json_path),
                js_json::stringify_lossless_pretty(&package_json),
            )?;
        }
        Ok(())
    }

    fn ensure_git_ignore(&self, dir: &JsString) -> Result<(), PackageManagerError> {
        if !path_exists(dir) {
            std::fs::create_dir_all(encode_node_utf8(dir))?;
        }
        let ignore_path = node_join(dir, ".gitignore");
        if !path_exists(&ignore_path) {
            std::fs::write(encode_node_utf8(&ignore_path), "*\n!.gitignore\n")?;
        }
        Ok(())
    }

    fn get_npm_install_root(&self, scope: SourceScope, temporary: bool) -> Result<JsString, PackageManagerError> {
        if temporary {
            return self.get_temporary_dir(&"npm".into(), None);
        }
        if scope == SourceScope::Project {
            self.assert_project_trusted_for_scope(scope)?;
            return Ok(node_join(node_join(&self.cwd, &config::package_config().config_dir_name), "npm").clone());
        }
        Ok(node_join(&self.agent_dir, "npm"))
    }

    fn get_global_npm_root(&self) -> Result<JsString, PackageManagerError> {
        let npm = self.get_npm_command()?;
        let mut command_parts = vec![npm.command.clone()];
        command_parts.extend(npm.args.clone());
        let command_key = JsString::join(command_parts, "\u{0}");
        {
            let cached = self.global_npm_root.lock().unwrap();
            if let Some((cached_key, root)) = cached.as_ref()
                && *cached_key == command_key
            {
                return Ok(root.clone());
            }
        }
        let root = if self.get_package_manager_name()? == "bun" {
            let bin_dir =
                self.run_npm_command_sync(&[JsString::from("pm"), JsString::from("bin"), JsString::from("-g")])?;
            node_join(node_dirname(bin_dir.trim()), "install/global/node_modules")
        } else {
            self.run_npm_command_sync(&[JsString::from("root"), JsString::from("-g")])?
                .trim()
                .to_owned()
        };
        *self.global_npm_root.lock().unwrap() = Some((command_key, root.clone()));
        Ok(root)
    }

    fn get_pnpm_global_package_path(&self, package_name: &JsString) -> Option<JsString> {
        if self.get_package_manager_name().ok()? != "pnpm" {
            return None;
        }
        let output = self
            .run_npm_command_sync(&[
                JsString::from("list"),
                JsString::from("-g"),
                JsString::from("--depth"),
                JsString::from("0"),
                JsString::from("--json"),
            ])
            .ok()?;
        let entries: JsValue = js_json::parse_lossless(&output).ok()?;
        for entry in entries.as_array()? {
            let path = entry
                .get("dependencies")
                .and_then(JsValue::as_object)
                .and_then(|dependencies| dependencies.get(package_name))
                .and_then(|dependency| dependency.get("path"))
                .and_then(JsValue::as_string);
            if let Some(path) = path {
                return Some(path.to_owned());
            }
        }
        None
    }

    fn get_managed_npm_install_path(
        &self,
        source: &NpmSource,
        scope: SourceScope,
    ) -> Result<JsString, PackageManagerError> {
        if scope == SourceScope::Temporary {
            let root = self.get_temporary_dir(&"npm".into(), None)?;
            return Ok(node_join(node_join(&root, "node_modules"), &source.name).clone());
        }
        if scope == SourceScope::Project {
            self.assert_project_trusted_for_scope(scope)?;
            return Ok(node_join(
                node_join(
                    node_join(&self.cwd, &config::package_config().config_dir_name),
                    "npm/node_modules",
                ),
                &source.name,
            )
            .clone());
        }
        Ok(node_join(node_join(&self.agent_dir, "npm/node_modules"), &source.name).clone())
    }

    fn get_legacy_global_npm_install_path(&self, source: &NpmSource) -> Option<JsString> {
        self.get_pnpm_global_package_path(&source.name).or_else(|| {
            self.get_global_npm_root()
                .ok()
                .map(|root| node_join(&root, &source.name))
        })
    }

    fn get_npm_install_path(&self, source: &NpmSource, scope: SourceScope) -> Result<JsString, PackageManagerError> {
        let managed_path = self.get_managed_npm_install_path(source, scope)?;
        if scope != SourceScope::User || path_exists(&managed_path) {
            return Ok(managed_path);
        }
        Ok(match self.get_legacy_global_npm_install_path(source) {
            Some(legacy) if path_exists(&legacy) => legacy,
            _ => managed_path,
        })
    }

    fn get_git_install_path(&self, source: &GitSource, scope: SourceScope) -> Result<JsString, PackageManagerError> {
        if scope == SourceScope::Temporary {
            return self.get_temporary_dir(&js_format!("git-{}", source.host), Some(&source.path));
        }
        let install_root = self
            .get_git_install_root(scope)?
            .ok_or_else(|| PackageManagerError::new("Missing git install root"))?;
        self.resolve_managed_path(&install_root, &[&source.host, &source.path])
    }

    fn get_git_install_root(&self, scope: SourceScope) -> Result<Option<JsString>, PackageManagerError> {
        if scope == SourceScope::Temporary {
            return Ok(None);
        }
        if scope == SourceScope::Project {
            self.assert_project_trusted_for_scope(scope)?;
            return Ok(Some(
                node_join(node_join(&self.cwd, &config::package_config().config_dir_name), "git").clone(),
            ));
        }
        Ok(Some(node_join(&self.agent_dir, "git")))
    }

    fn get_temporary_dir(&self, prefix: &JsString, suffix: Option<&JsString>) -> Result<JsString, PackageManagerError> {
        let temp_folder = get_extension_temp_folder(&self.agent_dir)?;
        let root = self.resolve_managed_path(&temp_folder, &[prefix])?;
        let mut hasher = Sha256::new();
        hasher.update(encode_node_utf8(&js_format!("{}-{}", prefix, suffix.cloned().unwrap_or_default())).as_bytes());
        let hash = JsString::from(format!("{:x}", hasher.finalize()));
        let hash = hash.slice(0, 8);
        self.resolve_managed_path(&root, &[&hash, &suffix.cloned().unwrap_or_default()])
    }

    fn resolve_managed_path(&self, root: &JsString, parts: &[&JsString]) -> Result<JsString, PackageManagerError> {
        let resolved_root = resolve_input(root, root)?;
        let resolved_path = parts
            .iter()
            .fold(resolved_root.clone(), |path, part| node_join(path, *part));
        let resolved_path = resolve_input(&resolved_path, &resolved_root)?;
        if resolved_path != resolved_root
            && !resolved_path.starts_with(&js_format!("{}{}", resolved_root, std::path::MAIN_SEPARATOR))
        {
            return Err(PackageManagerError::new(js_format!(
                "Refusing to use path outside package install root: {}",
                resolved_path
            )));
        }
        Ok(resolved_path)
    }

    fn get_base_dir_for_scope(&self, scope: SourceScope) -> Result<JsString, PackageManagerError> {
        if scope == SourceScope::Project {
            self.assert_project_trusted_for_scope(scope)?;
            return Ok(node_join(&self.cwd, &config::package_config().config_dir_name).clone());
        }
        if scope == SourceScope::User {
            return Ok(self.agent_dir.clone());
        }
        Ok(self.cwd.clone())
    }

    fn resolve_path(&self, input: &JsString) -> Result<JsString, PackageManagerError> {
        resolve_input(input, &self.cwd)
    }

    fn collect_package_resources(
        &self,
        package_root: &JsString,
        accumulator: &mut ResourceAccumulator,
        filter: Option<&PackageFilter>,
        metadata: PathMetadata,
    ) -> bool {
        if let Some(filter) = filter {
            for resource_type in ResourceType::ALL {
                let patterns = filter_patterns(filter, resource_type);
                if filter.autoload == Some(false) {
                    self.apply_package_delta_filter(
                        package_root,
                        patterns.map_or_else(|| &[][..], |patterns| patterns.as_slice()),
                        resource_type,
                        accumulator,
                        &metadata,
                    );
                } else if let Some(patterns) = patterns {
                    self.apply_package_filter(package_root, patterns, resource_type, accumulator, &metadata);
                } else {
                    self.collect_default_resources(package_root, resource_type, accumulator, &metadata);
                }
            }
            return true;
        }

        if let Some(manifest) = read_pi_manifest(&node_join(package_root, "package.json")) {
            for resource_type in ResourceType::ALL {
                self.add_manifest_entries(
                    resource_type.manifest_entries(&manifest),
                    package_root,
                    resource_type,
                    accumulator,
                    &metadata,
                );
            }
            return true;
        }

        let mut has_any_dir = false;
        for resource_type in ResourceType::ALL {
            let dir = node_join(package_root, resource_type.directory_name());
            if path_exists(&dir) {
                let dir = dir;
                for file in collect_resource_files(&dir, resource_type) {
                    add_resource(accumulator.target(resource_type), &file, metadata.clone(), true);
                }
                has_any_dir = true;
            }
        }
        has_any_dir
    }

    fn collect_default_resources(
        &self,
        package_root: &JsString,
        resource_type: ResourceType,
        accumulator: &mut ResourceAccumulator,
        metadata: &PathMetadata,
    ) {
        let manifest = read_pi_manifest(&node_join(package_root, "package.json"));
        if let Some(entries) = manifest
            .as_ref()
            .and_then(|manifest| resource_type.manifest_entries(manifest))
        {
            self.add_manifest_entries(Some(entries), package_root, resource_type, accumulator, metadata);
            return;
        }
        let dir = node_join(package_root, resource_type.directory_name());
        if path_exists(&dir) {
            let dir = dir;
            for file in collect_resource_files(&dir, resource_type) {
                add_resource(accumulator.target(resource_type), &file, metadata.clone(), true);
            }
        }
    }

    fn apply_package_filter(
        &self,
        package_root: &JsString,
        user_patterns: &[JsString],
        resource_type: ResourceType,
        accumulator: &mut ResourceAccumulator,
        metadata: &PathMetadata,
    ) {
        let all_files = self.collect_manifest_files(package_root, resource_type);
        if user_patterns.is_empty() {
            for file in &all_files {
                add_resource(accumulator.target(resource_type), file, metadata.clone(), false);
            }
            return;
        }
        let enabled_by_user = apply_patterns(&all_files, user_patterns, package_root);
        for file in &all_files {
            add_resource(
                accumulator.target(resource_type),
                file,
                metadata.clone(),
                enabled_by_user.contains(file),
            );
        }
    }

    fn apply_package_delta_filter(
        &self,
        package_root: &JsString,
        user_patterns: &[JsString],
        resource_type: ResourceType,
        accumulator: &mut ResourceAccumulator,
        metadata: &PathMetadata,
    ) {
        if user_patterns.is_empty() {
            return;
        }
        let all_files = self.collect_manifest_files(package_root, resource_type);
        for (file_path, enabled) in apply_autoload_disabled_patterns(&all_files, user_patterns, package_root) {
            add_resource(accumulator.target(resource_type), &file_path, metadata.clone(), enabled);
        }
    }

    fn collect_manifest_files(&self, package_root: &JsString, resource_type: ResourceType) -> Vec<JsString> {
        let manifest = read_pi_manifest(&node_join(package_root, "package.json"));
        if let Some(entries) = manifest
            .as_ref()
            .and_then(|manifest| resource_type.manifest_entries(manifest))
            .filter(|entries| !entries.is_empty())
        {
            let all_files = self.collect_files_from_manifest_entries(entries, package_root, resource_type);
            let manifest_patterns = get_override_patterns(entries);
            if manifest_patterns.is_empty() {
                return all_files;
            }
            let enabled = apply_patterns(&all_files, &manifest_patterns, package_root);
            return all_files.into_iter().filter(|file| enabled.contains(file)).collect();
        }

        let convention_dir = node_join(package_root, resource_type.directory_name());
        if !path_exists(&convention_dir) {
            return Vec::new();
        }
        collect_resource_files(&convention_dir, resource_type)
    }

    fn add_manifest_entries(
        &self,
        entries: Option<&Vec<JsString>>,
        root: &JsString,
        resource_type: ResourceType,
        accumulator: &mut ResourceAccumulator,
        metadata: &PathMetadata,
    ) {
        let Some(entries) = entries else {
            return;
        };
        let all_files = self.collect_files_from_manifest_entries(entries, root, resource_type);
        let patterns = get_override_patterns(entries);
        let enabled_paths = apply_patterns(&all_files, &patterns, root);
        for file in &all_files {
            if enabled_paths.contains(file) {
                add_resource(accumulator.target(resource_type), file, metadata.clone(), true);
            }
        }
    }

    fn collect_files_from_manifest_entries(
        &self,
        entries: &[JsString],
        root: &JsString,
        resource_type: ResourceType,
    ) -> Vec<JsString> {
        let resolved: Vec<JsString> = entries
            .iter()
            .filter(|entry| !is_override_pattern(entry))
            .flat_map(|entry| {
                if !has_glob_pattern(entry) {
                    return vec![resolve_input(entry, root).unwrap_or_else(|_| entry.clone())];
                }
                glob_sync(entry, root)
                    .into_iter()
                    .map(|matched| resolve_input(&matched, root).unwrap_or(matched))
                    .collect()
            })
            .collect();
        self.collect_files_from_paths(&resolved, resource_type)
    }

    fn resolve_local_entries(
        &self,
        entries: &[JsString],
        resource_type: ResourceType,
        accumulator: &mut ResourceAccumulator,
        metadata: PathMetadata,
        base_dir: &JsString,
    ) -> Result<(), PackageManagerError> {
        if entries.is_empty() {
            return Ok(());
        }
        let (plain, patterns) = split_patterns(entries);
        let resolved_plain: Vec<JsString> = plain
            .iter()
            .map(|entry| resolve_input(entry, base_dir))
            .collect::<Result<_, _>>()?;
        let all_files = self.collect_files_from_paths(&resolved_plain, resource_type);
        let enabled_paths = apply_patterns(&all_files, &patterns, base_dir);
        for file in &all_files {
            add_resource(
                accumulator.target(resource_type),
                file,
                metadata.clone(),
                enabled_paths.contains(file),
            );
        }
        Ok(())
    }

    fn add_auto_discovered_resources(
        &self,
        accumulator: &mut ResourceAccumulator,
        global_settings: &PackageSettingsSnapshot,
        project_settings: &PackageSettingsSnapshot,
        global_base_dir: &JsString,
        project_base_dir: &JsString,
    ) {
        let user_metadata = PathMetadata {
            source: JsString::from("auto"),
            scope: SourceScope::User,
            origin: MetadataOrigin::TopLevel,
            base_dir: Some(global_base_dir.to_owned()),
        };
        let project_metadata = PathMetadata {
            source: JsString::from("auto"),
            scope: SourceScope::Project,
            origin: MetadataOrigin::TopLevel,
            base_dir: Some(project_base_dir.to_owned()),
        };

        let user_dirs = resource_dirs(global_base_dir);
        let project_dirs = resource_dirs(project_base_dir);
        let home = home_dir();
        let user_agents_skills_dir = node_join(&home, ".agents/skills");
        let user_tool_skill_dirs: HashSet<JsString> = [".codex/skills", ".claude/skills"]
            .iter()
            .map(|child| node_join(&home, *child))
            .collect();
        let project_trusted = self.settings.is_project_trusted();
        let user_agents_resolved = resolve_input(&user_agents_skills_dir, &user_agents_skills_dir).unwrap_or_default();
        let project_agents_skill_dirs: Vec<JsString> = if project_trusted {
            collect_ancestor_agents_skill_dirs(&self.cwd)
                .into_iter()
                .filter(|dir| resolve_input(dir, dir).unwrap_or_default() != user_agents_resolved)
                .collect()
        } else {
            Vec::new()
        };
        let project_tool_skill_dirs: Vec<JsString> = if project_trusted {
            collect_ancestor_tool_skill_dirs(&self.cwd)
                .into_iter()
                .filter(|dir| !user_tool_skill_dirs.contains(&resolve_input(dir, dir).unwrap_or_default()))
                .collect()
        } else {
            Vec::new()
        };

        if project_trusted {
            self.add_resources(
                accumulator,
                ResourceType::Extensions,
                collect_auto_extension_entries(&project_dirs.extensions),
                &project_metadata,
                &project_settings.extensions,
                project_base_dir,
            );
            self.add_resources(
                accumulator,
                ResourceType::Skills,
                collect_auto_skill_entries(&project_dirs.skills, SkillDiscoveryMode::Pi),
                &project_metadata,
                &project_settings.skills,
                project_base_dir,
            );
        }

        for agents_skills_dir in &project_agents_skill_dirs {
            let agents_base_dir = Some(node_dirname(agents_skills_dir)).unwrap_or_default();
            let metadata = PathMetadata {
                base_dir: Some(agents_base_dir.clone()),
                ..project_metadata.clone()
            };
            self.add_resources(
                accumulator,
                ResourceType::Skills,
                collect_auto_skill_entries(agents_skills_dir, SkillDiscoveryMode::Agents),
                &metadata,
                &project_settings.skills,
                &agents_base_dir,
            );
        }

        for tool_skills_dir in &project_tool_skill_dirs {
            let tool_base_dir = Some(node_dirname(tool_skills_dir)).unwrap_or_default();
            let metadata = PathMetadata {
                base_dir: Some(tool_base_dir.clone()),
                ..project_metadata.clone()
            };
            self.add_resources(
                accumulator,
                ResourceType::Skills,
                collect_auto_skill_entries(tool_skills_dir, SkillDiscoveryMode::Pi),
                &metadata,
                &project_settings.skills,
                &tool_base_dir,
            );
        }

        if project_trusted {
            self.add_resources(
                accumulator,
                ResourceType::Prompts,
                collect_auto_prompt_entries(&project_dirs.prompts),
                &project_metadata,
                &project_settings.prompts,
                project_base_dir,
            );
            self.add_resources(
                accumulator,
                ResourceType::Themes,
                collect_auto_theme_entries(&project_dirs.themes),
                &project_metadata,
                &project_settings.themes,
                project_base_dir,
            );
        }

        self.add_resources(
            accumulator,
            ResourceType::Extensions,
            collect_auto_extension_entries(&user_dirs.extensions),
            &user_metadata,
            &global_settings.extensions,
            global_base_dir,
        );
        self.add_resources(
            accumulator,
            ResourceType::Skills,
            collect_auto_skill_entries(&user_dirs.skills, SkillDiscoveryMode::Pi),
            &user_metadata,
            &global_settings.skills,
            global_base_dir,
        );

        let user_agents_base_dir = Some(node_dirname(&user_agents_skills_dir)).unwrap_or_default();
        let agents_metadata = PathMetadata {
            base_dir: Some(user_agents_base_dir.clone()),
            ..user_metadata.clone()
        };
        self.add_resources(
            accumulator,
            ResourceType::Skills,
            collect_auto_skill_entries(&user_agents_skills_dir, SkillDiscoveryMode::Agents),
            &agents_metadata,
            &global_settings.skills,
            &user_agents_base_dir,
        );

        self.add_resources(
            accumulator,
            ResourceType::Prompts,
            collect_auto_prompt_entries(&user_dirs.prompts),
            &user_metadata,
            &global_settings.prompts,
            global_base_dir,
        );
        self.add_resources(
            accumulator,
            ResourceType::Themes,
            collect_auto_theme_entries(&user_dirs.themes),
            &user_metadata,
            &global_settings.themes,
            global_base_dir,
        );
    }

    fn add_resources(
        &self,
        accumulator: &mut ResourceAccumulator,
        resource_type: ResourceType,
        paths: Vec<JsString>,
        metadata: &PathMetadata,
        overrides: &[JsString],
        base_dir: &JsString,
    ) {
        for path in paths {
            let enabled = is_enabled_by_overrides(&path, overrides, base_dir);
            add_resource(accumulator.target(resource_type), &path, metadata.clone(), enabled);
        }
    }

    fn collect_files_from_paths(&self, paths: &[JsString], resource_type: ResourceType) -> Vec<JsString> {
        let mut files = Vec::new();
        for path in paths {
            let Ok(stat) = std::fs::metadata(encode_node_utf8(path)) else {
                continue;
            };
            if stat.is_file() {
                files.push(path.clone());
            } else if stat.is_dir() {
                files.extend(collect_resource_files(path, resource_type));
            }
        }
        files
    }

    async fn run_command(
        &self,
        command: &JsString,
        args: &[JsString],
        cwd: Option<&JsString>,
    ) -> Result<(), PackageManagerError> {
        let mut child = spawn_inherit(command, args, cwd, is_stdout_taken_over())?;
        let status = child.wait().await?;
        if status.success() {
            return Ok(());
        }
        Err(PackageManagerError::new(js_format!(
            "{} {} failed with code {}",
            command,
            JsString::join(args.iter().cloned(), " "),
            status
                .code()
                .map_or_else(|| JsString::from("null"), |code| code.to_string().into())
        )))
    }

    async fn run_command_capture(
        &self,
        command: &JsString,
        args: &[JsString],
        cwd: Option<&JsString>,
        timeout_ms: Option<u64>,
        env: &[(JsString, JsString)],
    ) -> Result<JsString, PackageManagerError> {
        let mut child = spawn_capture(command, args, cwd, env)?;
        let mut stdout = child.stdout.take();
        let mut stderr = child.stderr.take();
        let stdout_task = tokio::spawn(async move {
            let mut buffer = Vec::new();
            if let Some(stdout) = stdout.as_mut() {
                let _ = stdout.read_to_end(&mut buffer).await;
            }
            buffer
        });
        let stderr_task = tokio::spawn(async move {
            let mut buffer = Vec::new();
            if let Some(stderr) = stderr.as_mut() {
                let _ = stderr.read_to_end(&mut buffer).await;
            }
            buffer
        });

        let mut timed_out = false;
        let status = match timeout_ms {
            Some(timeout_ms) => match tokio::time::timeout(Duration::from_millis(timeout_ms), child.wait()).await {
                Ok(status) => status?,
                Err(_) => {
                    timed_out = true;
                    terminate_child(&mut child);
                    child.wait().await?
                }
            },
            None => child.wait().await?,
        };

        let stdout = String::from_utf8_lossy(&stdout_task.await.unwrap_or_default()).into_owned();
        let stderr = String::from_utf8_lossy(&stderr_task.await.unwrap_or_default()).into_owned();
        if timed_out {
            return Err(PackageManagerError::new(js_format!(
                "{} {} timed out after {}ms",
                command,
                JsString::join(args.iter().cloned(), " "),
                timeout_ms.unwrap_or_default()
            )));
        }
        if status.success() {
            return Ok(stdout.trim().into());
        }
        let exit_status = match status.code() {
            Some(code) => js_format!("code {}", code),
            None => js_format!("signal {}", crate::utils::child_process::exit_signal_name(&status)),
        };
        let detail = if stderr.is_empty() { stdout } else { stderr };
        Err(PackageManagerError::new(js_format!(
            "{} {} failed with {}: {}",
            command,
            JsString::join(args.iter().cloned(), " "),
            exit_status,
            detail
        )))
    }

    fn run_command_sync(&self, command: &JsString, args: &[JsString]) -> Result<JsString, PackageManagerError> {
        let output = std::process::Command::new(encode_node_utf8(command))
            .args(args.iter().map(encode_node_utf8))
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output();
        match output {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
                let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
                let value = if stdout.is_empty() { stderr } else { stdout };
                Ok(value.trim().into())
            }
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
                let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
                let detail = if stderr.is_empty() { stdout } else { stderr };
                Err(PackageManagerError::new(js_format!(
                    "Failed to run {} {}: {}",
                    command,
                    JsString::join(args.iter().cloned(), " "),
                    detail
                )))
            }
            Err(error) => Err(PackageManagerError::new(js_format!(
                "Failed to run {} {}: {}",
                command,
                JsString::join(args.iter().cloned(), " "),
                node_spawn_error_message("spawnSync", command, &error)
            ))),
        }
    }
}

struct GitUpdateTargetSpec {
    reference: JsString,
    fetch_args: Vec<JsString>,
}

struct ResourceDirs {
    extensions: JsString,
    skills: JsString,
    prompts: JsString,
    themes: JsString,
}

fn resource_dirs(base_dir: &JsString) -> ResourceDirs {
    ResourceDirs {
        extensions: node_join(base_dir, "extensions"),
        skills: node_join(base_dir, "skills"),
        prompts: node_join(base_dir, "prompts"),
        themes: node_join(base_dir, "themes"),
    }
}

fn add_resource(map: &mut ResourceMap, path: &JsString, metadata: PathMetadata, enabled: bool) {
    if path.is_empty() || map.iter().any(|(existing, _)| existing == path) {
        return;
    }
    map.push((path.to_owned(), ResourceEntry { metadata, enabled }));
}

fn remove_dir_all_if_exists(path: &JsString) {
    let _ = std::fs::remove_dir_all(encode_node_utf8(path));
}

fn package_manager_basename(command: &JsString) -> JsString {
    let basename = node_basename(command);
    let lower = basename.to_lowercase();
    if lower.ends_with(".cmd") {
        return basename.slice(0, basename.len() - 4);
    }
    if lower.ends_with(".exe") {
        return basename.slice(0, basename.len() - 4);
    }
    basename.to_owned()
}

fn parse_npm_spec(spec: &JsString) -> (JsString, Option<JsString>) {
    static PATTERN: std::sync::OnceLock<JsRegex> = std::sync::OnceLock::new();
    let pattern = PATTERN
        .get_or_init(|| JsRegex::new(r"^(@?[^@]+(?:/[^@]+)?)(?:@(.+))?$").expect("npm spec pattern must compile"));
    let Some(captures) = pattern.captures(spec) else {
        return (spec.to_owned(), None);
    };
    let name = captures.get(1).map_or_else(|| spec.to_owned(), |value| value.text());
    let version = captures.get(2).map(|value| value.text());
    (name, version)
}

fn first_remote_commit(output: &JsString) -> Option<JsString> {
    static PATTERN: std::sync::OnceLock<JsRegex> = std::sync::OnceLock::new();
    let pattern = PATTERN
        .get_or_init(|| JsRegex::with_flags(r"^([0-9a-f]{40})\s+", "m").expect("remote commit pattern must compile"));
    pattern
        .captures(output)
        .and_then(|captures| captures.get(1))
        .map(|value| value.text())
}

fn first_remote_head(output: &JsString) -> Option<JsString> {
    static PATTERN: std::sync::OnceLock<JsRegex> = std::sync::OnceLock::new();
    let pattern = PATTERN.get_or_init(|| {
        JsRegex::with_flags(r"^([0-9a-f]{40})\s+HEAD$", "m").expect("remote HEAD pattern must compile")
    });
    pattern
        .captures(output)
        .and_then(|captures| captures.get(1))
        .map(|value| value.text())
}

async fn run_with_concurrency<F, Fut, T>(tasks: Vec<F>, limit: usize) -> Vec<T>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = T>,
{
    if tasks.is_empty() {
        return Vec::new();
    }
    let worker_count = limit.max(1).min(tasks.len());
    let mut indexed: Vec<(usize, T)> = stream::iter(tasks.into_iter().enumerate())
        .map(|(index, task)| async move { (index, task().await) })
        .buffer_unordered(worker_count)
        .collect()
        .await;
    indexed.sort_by_key(|(index, _)| *index);
    indexed.into_iter().map(|(_, value)| value).collect()
}

fn spawn_inherit(
    command: &JsString,
    args: &[JsString],
    cwd: Option<&JsString>,
    redirect_stdout: bool,
) -> Result<Child, PackageManagerError> {
    let mut process = tokio::process::Command::new(encode_node_utf8(command));
    process
        .args(args.iter().map(encode_node_utf8))
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit());
    if redirect_stdout {
        process.stdout(stderr_redirect_stdout()?);
    } else {
        process.stdout(std::process::Stdio::inherit());
    }
    if let Some(cwd) = cwd {
        process.current_dir(encode_node_utf8(cwd));
    }
    process
        .spawn()
        .map_err(|error| PackageManagerError::new(node_spawn_error_message("spawn", command, &error)))
}

fn spawn_capture(
    command: &JsString,
    args: &[JsString],
    cwd: Option<&JsString>,
    env: &[(JsString, JsString)],
) -> Result<Child, PackageManagerError> {
    let mut process = tokio::process::Command::new(encode_node_utf8(command));
    process
        .args(args.iter().map(encode_node_utf8))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    for (name, value) in env {
        process.env(encode_node_utf8(name), encode_node_utf8(value));
    }
    if let Some(cwd) = cwd {
        process.current_dir(encode_node_utf8(cwd));
    }
    process
        .spawn()
        .map_err(|error| PackageManagerError::new(node_spawn_error_message("spawn", command, &error)))
}

#[cfg(unix)]
fn stderr_redirect_stdout() -> Result<std::process::Stdio, PackageManagerError> {
    use std::os::unix::fs::OpenOptionsExt;

    let file = std::fs::OpenOptions::new()
        .write(true)
        .mode(0o600)
        .open("/dev/stderr")?;
    Ok(std::process::Stdio::from(file))
}

#[cfg(not(unix))]
fn stderr_redirect_stdout() -> Result<std::process::Stdio, PackageManagerError> {
    Ok(std::process::Stdio::inherit())
}

fn terminate_child(child: &mut Child) {
    if let Some(pid) = child.id()
        && let Some(pid) = rustix::process::Pid::from_raw(pid as i32)
    {
        let _ = rustix::process::kill_process(pid, rustix::process::Signal::TERM);
        return;
    }
    let _ = child.start_kill();
}

fn path_exists(path: &JsString) -> bool {
    Path::new(&encode_node_utf8(path)).exists()
}
fn read_text_file(path: &JsString) -> std::io::Result<JsString> {
    std::fs::read(encode_node_utf8(path)).map(|bytes| JsString::from(String::from_utf8_lossy(&bytes).as_ref()))
}
