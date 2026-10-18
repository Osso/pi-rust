//! Extension registry and discovery (`core/extensions/loader.ts`).
//!
//! TypeScript extension modules are not loaded: Rust extensions are registered
//! as factories keyed by extension id. Discovery, enabled/disabled filtering,
//! load order, and diagnostics text follow the source loader.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use indexmap::IndexMap;
use serde_json::Value;

use crate::config::package_config;
use crate::core::event_bus::{EventBus, EventBusController};
use crate::core::pi_manifest::read_pi_manifest;
use crate::core::source_info::{SyntheticSourceInfoOptions, create_synthetic_source_info};
use crate::core::timings::{TimingLabel, time};
use crate::utils::paths::{PathInputOptions, resolve_path};

use super::extension_runtime::{ExtensionRuntime, create_extension_runtime};
use super::types::{
    BoxFuture, CommandHandler, Extension, ExtensionApi, ExtensionEventBus, ExtensionHandlers, ExtensionLoadError,
    ExtensionShortcut, HandlerError, LoadExtensionsResult, RegisteredCommand, RegisteredTool,
};

/// Extension entry point receiving the registration API.
pub type ExtensionFactory = Arc<dyn Fn(&ExtensionApi) -> BoxFuture<'static, Result<(), HandlerError>> + Send + Sync>;

#[derive(Clone)]
struct RegisteredExtension {
    id: String,
    factory: ExtensionFactory,
    hidden: bool,
}

/// Static registry of Rust extensions.
#[derive(Clone, Default)]
pub struct ExtensionRegistry {
    entries: IndexMap<String, RegisteredExtension>,
}

impl ExtensionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a Rust extension factory under an id such as `<first-party:goal>`.
    pub fn register(&mut self, id: impl Into<String>, factory: ExtensionFactory) {
        let id = id.into();
        self.entries.insert(
            id.clone(),
            RegisteredExtension {
                id,
                factory,
                hidden: false,
            },
        );
    }

    /// Register a factory omitted from the startup extension list.
    pub fn register_hidden(&mut self, id: impl Into<String>, factory: ExtensionFactory) {
        let id = id.into();
        self.entries.insert(
            id.clone(),
            RegisteredExtension {
                id,
                factory,
                hidden: true,
            },
        );
    }

    pub fn get(&self, id: &str) -> Option<&ExtensionFactory> {
        self.entries.get(id).map(|entry| &entry.factory)
    }

    pub fn ids(&self) -> Vec<String> {
        self.entries.keys().cloned().collect()
    }

    /// Load every registered factory whose id is not disabled, in registration order.
    pub async fn load_registered(
        &self,
        disabled_extensions: &[String],
        cwd: &str,
        event_bus: Arc<dyn EventBus>,
        runtime: ExtensionRuntime,
    ) -> (Vec<Extension>, Vec<ExtensionLoadError>) {
        let mut extensions = Vec::new();
        let mut errors = Vec::new();
        for entry in self.entries.values() {
            if is_extension_factory_disabled(disabled_extensions, &entry.id) {
                continue;
            }
            match load_extension_from_factory(
                Arc::clone(&entry.factory),
                cwd,
                Arc::clone(&event_bus),
                runtime.clone(),
                &entry.id,
            )
            .await
            {
                Ok(mut extension) => {
                    extension.hidden = Some(entry.hidden);
                    extensions.push(extension);
                }
                Err(error) => errors.push(ExtensionLoadError {
                    path: entry.id.clone(),
                    error: error.message,
                }),
            }
        }
        (extensions, errors)
    }
}

fn default_path_options() -> PathInputOptions {
    PathInputOptions::default()
}

fn normalize_unicode_space_options() -> PathInputOptions {
    PathInputOptions {
        normalize_unicode_spaces: true,
        ..PathInputOptions::default()
    }
}

fn resolve_against_process_cwd(input: &str, options: &PathInputOptions) -> String {
    let process_cwd = std::env::current_dir()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    resolve_path(input, &process_cwd, options).unwrap_or_else(|_| input.to_owned())
}

fn extension_source(extension_path: &str) -> String {
    if extension_path.starts_with('<') && extension_path.ends_with('>') {
        let inner = &extension_path[1..extension_path.len() - 1];
        let name = inner.split(':').next().unwrap_or_default();
        if name.is_empty() {
            "temporary".to_owned()
        } else {
            name.to_owned()
        }
    } else {
        "local".to_owned()
    }
}

/// Create an extension with empty collections.
fn create_extension(extension_path: &str, resolved_path: &str) -> Extension {
    let source = extension_source(extension_path);
    let base_dir = if extension_path.starts_with('<') {
        None
    } else {
        Path::new(resolved_path)
            .parent()
            .map(|parent| parent.to_string_lossy().into_owned())
    };
    Extension {
        path: extension_path.to_owned(),
        resolved_path: resolved_path.to_owned(),
        hidden: None,
        source_info: create_synthetic_source_info(
            extension_path,
            &SyntheticSourceInfoOptions {
                source,
                scope: None,
                origin: None,
                base_dir,
            },
        ),
        handlers: ExtensionHandlers::default(),
        approval_reviewers: Vec::new(),
        tool_gates: Vec::new(),
        tools: IndexMap::new(),
        message_renderers: IndexMap::new(),
        markdown_transformer: None,
        entry_renderers: Some(IndexMap::new()),
        commands: IndexMap::new(),
        flags: IndexMap::new(),
        shortcuts: IndexMap::new(),
    }
}

/// Create an extension from an inline Rust factory.
pub async fn load_extension_from_factory(
    factory: ExtensionFactory,
    cwd: &str,
    event_bus: Arc<dyn EventBus>,
    runtime: ExtensionRuntime,
    extension_path: &str,
) -> Result<Extension, HandlerError> {
    let extension = create_extension(extension_path, extension_path);
    let resolved_cwd = resolve_against_process_cwd(cwd, &default_path_options());
    let extension = Arc::new(Mutex::new(extension));
    let api = ExtensionApi::new(Arc::clone(&extension), runtime, resolved_cwd, event_bus);
    factory(&api).await?;
    time(&format!("{extension_path} factory"), TimingLabel::Extensions);
    let extension = extension
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    Ok(extension)
}

/// Resolve extension entry points from a directory.
fn resolve_extension_entries(dir: &str) -> Option<Vec<String>> {
    let package_json_path = Path::new(dir).join("package.json");
    if package_json_path.exists() {
        if let Some(manifest) = read_pi_manifest(&package_json_path) {
            if let Some(declared) = manifest.extensions.filter(|entries| !entries.is_empty()) {
                let mut entries = Vec::new();
                for extension_path in declared {
                    let resolved = Path::new(dir).join(&extension_path);
                    if resolved.exists() {
                        entries.push(resolved.to_string_lossy().into_owned());
                    }
                }
                if !entries.is_empty() {
                    return Some(entries);
                }
            }
        }
    }

    let index_ts = Path::new(dir).join("index.ts");
    if index_ts.exists() {
        return Some(vec![index_ts.to_string_lossy().into_owned()]);
    }
    let index_js = Path::new(dir).join("index.js");
    if index_js.exists() {
        return Some(vec![index_js.to_string_lossy().into_owned()]);
    }
    None
}

fn is_extension_file(name: &str) -> bool {
    name.ends_with(".ts") || name.ends_with(".js")
}

/// Discover extension entry points one level below `dir`.
fn discover_extensions_in_dir(dir: &str) -> Vec<String> {
    if !Path::new(dir).exists() {
        return Vec::new();
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut discovered = Vec::new();
    for entry in entries.flatten() {
        let entry_path = entry.path();
        let entry_name = entry.file_name().to_string_lossy().into_owned();
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(_) => continue,
        };
        let is_symlink = file_type.is_symlink();
        if (file_type.is_file() || is_symlink) && is_extension_file(&entry_name) {
            discovered.push(entry_path.to_string_lossy().into_owned());
            continue;
        }
        if file_type.is_dir() || is_symlink {
            if let Some(entries) = resolve_extension_entries(&entry_path.to_string_lossy()) {
                discovered.extend(entries);
            }
        }
    }
    discovered
}

/// Discover extension paths from project/global dirs and configured paths.
pub fn discover_extension_paths(configured_paths: &[String], cwd: &str, agent_dir: &str) -> Vec<String> {
    let resolved_cwd = resolve_against_process_cwd(cwd, &default_path_options());
    let resolved_agent_dir = resolve_against_process_cwd(agent_dir, &default_path_options());
    let mut all_paths: Vec<String> = Vec::new();
    let mut seen: Vec<String> = Vec::new();

    let add_paths = |paths: Vec<String>, all_paths: &mut Vec<String>, seen: &mut Vec<String>| {
        for path in paths {
            let resolved = resolve_against_process_cwd(&path, &default_path_options());
            if !seen.contains(&resolved) {
                seen.push(resolved);
                all_paths.push(path);
            }
        }
    };

    let local_ext_dir = Path::new(&resolved_cwd)
        .join(&package_config().config_dir_name)
        .join("extensions");
    add_paths(
        discover_extensions_in_dir(&local_ext_dir.to_string_lossy()),
        &mut all_paths,
        &mut seen,
    );

    let global_ext_dir = Path::new(&resolved_agent_dir).join("extensions");
    add_paths(
        discover_extensions_in_dir(&global_ext_dir.to_string_lossy()),
        &mut all_paths,
        &mut seen,
    );

    for configured in configured_paths {
        let resolved = resolve_against_process_cwd(configured, &normalize_unicode_space_options());
        let resolved_path = PathBuf::from(&resolved);
        if resolved_path.exists() && resolved_path.is_dir() {
            if let Some(entries) = resolve_extension_entries(&resolved) {
                add_paths(entries, &mut all_paths, &mut seen);
                continue;
            }
            add_paths(discover_extensions_in_dir(&resolved), &mut all_paths, &mut seen);
            continue;
        }
        add_paths(vec![resolved], &mut all_paths, &mut seen);
    }

    all_paths
}

fn first_party_extension_name(extension_path: &str) -> Option<String> {
    let prefix = "<first-party:";
    if extension_path.starts_with(prefix) && extension_path.ends_with('>') {
        return Some(extension_path[prefix.len()..extension_path.len() - 1].to_owned());
    }
    None
}

/// Inline extension name used by disabled-extension filtering.
pub fn inline_extension_name(extension_path: &str) -> Option<String> {
    if let Some(name) = first_party_extension_name(extension_path) {
        return Some(name);
    }
    if extension_path.starts_with('<') && extension_path.ends_with('>') {
        return Some(extension_path[1..extension_path.len() - 1].to_owned());
    }
    None
}

fn disabled_names(disabled_extensions: &[String]) -> Vec<String> {
    disabled_extensions
        .iter()
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
        .collect()
}

fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Whether a filesystem extension path matches a disabled extension name.
pub fn is_extension_path_disabled(disabled_extensions: &[String], extension_path: &str, resolved_path: &str) -> bool {
    let disabled = disabled_names(disabled_extensions);
    if disabled.is_empty() {
        return false;
    }
    [
        extension_path.to_owned(),
        resolved_path.to_owned(),
        file_name(extension_path),
        file_name(resolved_path),
    ]
    .into_iter()
    .filter(|candidate| !candidate.is_empty())
    .any(|candidate| disabled.contains(&candidate))
}

/// Whether an inline/first-party extension id matches a disabled extension name.
pub fn is_extension_factory_disabled(disabled_extensions: &[String], extension_path: &str) -> bool {
    let disabled = disabled_names(disabled_extensions);
    if disabled.is_empty() {
        return false;
    }
    let mut candidates = vec![extension_path.to_owned()];
    if let Some(name) = inline_extension_name(extension_path) {
        candidates.push(name);
    }
    candidates
        .into_iter()
        .filter(|candidate| !candidate.is_empty())
        .any(|candidate| disabled.contains(&candidate))
}

impl ExtensionRegistry {
    async fn load_extension(
        &self,
        extension_path: &str,
        cwd: &str,
        event_bus: Arc<dyn EventBus>,
        runtime: ExtensionRuntime,
    ) -> Result<Extension, ExtensionLoadError> {
        let resolved_path = resolve_against_process_cwd(extension_path, &normalize_unicode_space_options());
        let Some(factory) = self.get(&resolved_path).or_else(|| self.get(extension_path)).cloned() else {
            return Err(ExtensionLoadError {
                path: extension_path.to_owned(),
                error: format!("Extension does not export a valid factory function: {extension_path}"),
            });
        };
        time(&format!("{extension_path} module import"), TimingLabel::Extensions);
        load_extension_from_factory(factory, cwd, event_bus, runtime, extension_path)
            .await
            .map_err(|error| ExtensionLoadError {
                path: extension_path.to_owned(),
                error: format!("Failed to load extension: {}", error.message),
            })
    }

    /// Load extensions from paths; load failures are collected, not thrown.
    pub async fn load_extensions(
        &self,
        paths: &[String],
        cwd: &str,
        event_bus: Option<Arc<dyn EventBus>>,
        runtime: Option<ExtensionRuntime>,
    ) -> LoadExtensionsResult {
        let event_bus = event_bus.unwrap_or_else(|| Arc::new(EventBusController::new()));
        let runtime = runtime.unwrap_or_else(create_extension_runtime);
        let resolved_cwd = resolve_against_process_cwd(cwd, &default_path_options());
        let mut extensions = Vec::new();
        let mut errors = Vec::new();
        for extension_path in paths {
            match self
                .load_extension(extension_path, &resolved_cwd, Arc::clone(&event_bus), runtime.clone())
                .await
            {
                Ok(extension) => extensions.push(extension),
                Err(error) => errors.push(error),
            }
        }
        LoadExtensionsResult {
            extensions,
            errors,
            runtime,
        }
    }

    /// Discover project/global/configured extensions and load the enabled ones.
    pub async fn discover_and_load_extensions(
        &self,
        configured_paths: &[String],
        cwd: &str,
        agent_dir: &str,
        event_bus: Option<Arc<dyn EventBus>>,
        disabled_extensions: &[String],
    ) -> LoadExtensionsResult {
        let resolved_cwd = resolve_against_process_cwd(cwd, &default_path_options());
        let paths = discover_extension_paths(configured_paths, cwd, agent_dir);
        let enabled: Vec<String> = paths
            .into_iter()
            .filter(|path| {
                let resolved = resolve_against_process_cwd(path, &default_path_options());
                !is_extension_path_disabled(disabled_extensions, path, &resolved)
            })
            .collect();
        self.load_extensions(&enabled, &resolved_cwd, event_bus, None).await
    }
}

/// `loadExtensions` over an explicit path list.
pub async fn load_extensions(
    paths: &[String],
    cwd: &str,
    event_bus: Option<Arc<dyn EventBus>>,
    runtime: Option<ExtensionRuntime>,
) -> LoadExtensionsResult {
    ExtensionRegistry::new()
        .load_extensions(paths, cwd, event_bus, runtime)
        .await
}

/// Cached loading is identical for static Rust factories.
pub async fn load_extensions_cached(
    paths: &[String],
    cwd: &str,
    event_bus: Option<Arc<dyn EventBus>>,
    runtime: Option<ExtensionRuntime>,
) -> LoadExtensionsResult {
    load_extensions(paths, cwd, event_bus, runtime).await
}

/// TypeScript module caching has no Rust counterpart.
pub fn clear_extension_cache() {}

/// `discoverAndLoadExtensions` for the default registry.
pub async fn discover_and_load_extensions(
    configured_paths: &[String],
    cwd: &str,
    agent_dir: &str,
    event_bus: Option<Arc<dyn EventBus>>,
) -> LoadExtensionsResult {
    ExtensionRegistry::new()
        .discover_and_load_extensions(configured_paths, cwd, agent_dir, event_bus, &[])
        .await
}

/// Convenience: create the API used by inline test extensions.
pub fn create_extension_api(
    extension: Arc<Mutex<Extension>>,
    runtime: ExtensionRuntime,
    cwd: &str,
    event_bus: Arc<dyn EventBus>,
) -> ExtensionApi {
    ExtensionApi::new(extension, runtime, cwd.to_owned(), event_bus)
}

/// Re-exported helper used by extension registries building commands.
pub fn command_handler<F>(handler: F) -> CommandHandler
where
    F: for<'a> Fn(&'a str, &'a super::types::ExtensionCommandContext) -> BoxFuture<'a, Result<(), HandlerError>>
        + Send
        + Sync
        + 'static,
{
    Arc::new(handler)
}

/// Re-exported helper used by extension registries building shortcuts.
pub fn shortcut_handler<F>(
    handler: F,
) -> Arc<dyn for<'a> Fn(&'a super::types::ExtensionContext) -> BoxFuture<'a, Result<(), HandlerError>> + Send + Sync>
where
    F: for<'a> Fn(&'a super::types::ExtensionContext) -> BoxFuture<'a, Result<(), HandlerError>>
        + Send
        + Sync
        + 'static,
{
    Arc::new(handler)
}

/// Re-exported event-bus constructor for extension factories.
pub fn extension_event_bus(runtime: ExtensionRuntime, bus: Arc<dyn EventBus>) -> ExtensionEventBus {
    ExtensionEventBus::new(bus, runtime)
}

/// Re-exported placeholder for extension command metadata.
pub fn registered_command(
    name: &str,
    source_info: crate::core::source_info::SourceInfo,
    description: Option<String>,
    handler: CommandHandler,
) -> RegisteredCommand {
    RegisteredCommand {
        name: name.to_owned(),
        source_info,
        description,
        get_argument_completions: None,
        handler,
    }
}

/// Re-exported placeholder for shortcut metadata.
pub fn extension_shortcut(
    shortcut: &str,
    description: Option<String>,
    handler: Arc<
        dyn for<'a> Fn(&'a super::types::ExtensionContext) -> BoxFuture<'a, Result<(), HandlerError>> + Send + Sync,
    >,
    extension_path: &str,
) -> ExtensionShortcut {
    ExtensionShortcut {
        shortcut: shortcut.to_owned(),
        description,
        handler,
        extension_path: extension_path.to_owned(),
    }
}

/// Re-exported registered tool constructor.
pub fn registered_tool(
    definition: super::types::ToolDefinition,
    source_info: crate::core::source_info::SourceInfo,
) -> RegisteredTool {
    RegisteredTool {
        definition,
        source_info,
    }
}

/// Helper matching the source's manifest value type.
pub fn manifest_extension_value(value: &Value) -> Option<String> {
    value.as_str().map(str::to_owned)
}
