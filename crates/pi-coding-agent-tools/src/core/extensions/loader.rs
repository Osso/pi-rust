//! Extension registry and discovery (`core/extensions/loader.ts`).
//!
//! TypeScript extension modules are not loaded: Rust extensions are registered
//! as factories keyed by extension id. Discovery, enabled/disabled filtering,
//! load order, and diagnostics text follow the source loader.

use std::path::Path;
use std::sync::Arc;

use indexmap::IndexMap;
use pi_js_compat::{
    js_format,
    js_json::{JsString, JsValue, encode_node_utf8},
};

use crate::config::package_config;
use crate::core::event_bus::{EventBus, EventBusController};
use crate::core::pi_manifest::read_pi_manifest;
use crate::core::source_info::{SyntheticSourceInfoOptions, create_synthetic_source_info};
use crate::core::timings::{TimingLabel, time};
use crate::utils::paths::{
    PathInputOptions, node_basename, node_dirname, node_is_absolute, node_join, node_normalize, process_cwd,
    resolve_path,
};

use super::extension_runtime::{ExtensionRuntime, create_extension_runtime};
use super::types::{
    BoxFuture, CommandHandler, Extension, ExtensionApi, ExtensionEventBus, ExtensionHandlers, ExtensionLoadError,
    ExtensionShortcut, ExtensionState, HandlerError, LoadExtensionsResult, RegisteredCommand, RegisteredTool,
};

/// Extension entry point receiving the registration API.
pub type ExtensionFactory = Arc<dyn Fn(&ExtensionApi) -> BoxFuture<'static, Result<(), HandlerError>> + Send + Sync>;

#[derive(Clone)]
struct RegisteredExtension {
    id: JsString,
    factory: ExtensionFactory,
    hidden: bool,
}

/// Static registry of Rust extensions.
#[derive(Clone, Default)]
pub struct ExtensionRegistry {
    entries: IndexMap<JsString, RegisteredExtension>,
}

impl ExtensionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a Rust extension factory under an id such as `<first-party:goal>`.
    pub fn register(&mut self, id: impl Into<JsString>, factory: ExtensionFactory) {
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
    pub fn register_hidden(&mut self, id: impl Into<JsString>, factory: ExtensionFactory) {
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

    pub fn get(&self, id: &JsString) -> Option<&ExtensionFactory> {
        self.entries.get(id).map(|entry| &entry.factory)
    }

    pub fn ids(&self) -> Vec<JsString> {
        self.entries.keys().cloned().collect()
    }

    /// Load every registered factory whose id is not disabled, in registration order.
    pub async fn load_registered(
        &self,
        disabled_extensions: &[JsString],
        cwd: &JsString,
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
                Ok(extension) => {
                    extension.read().hidden = Some(entry.hidden);
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

fn resolve_against_process_cwd(input: &JsString, options: &PathInputOptions) -> Result<JsString, HandlerError> {
    Ok(resolve_path(input, process_cwd()?, options)?)
}

fn extension_source(extension_path: &JsString) -> JsString {
    if extension_path.starts_with('<') && extension_path.ends_with('>') {
        let inner = extension_path.slice(1, extension_path.len() - 1);
        let name = inner.split(':').next().unwrap_or_default();
        if name.is_empty() {
            "temporary".into()
        } else {
            name.to_owned()
        }
    } else {
        "local".into()
    }
}

/// Create an extension with empty collections.
fn create_extension(extension_path: &JsString, resolved_path: &JsString) -> Extension {
    let source = extension_source(extension_path);
    let base_dir = if extension_path.starts_with('<') {
        None
    } else {
        Some(node_dirname(resolved_path))
    };
    Extension::new(ExtensionState {
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
    })
}

/// Create an extension from an inline Rust factory.
pub async fn load_extension_from_factory(
    factory: ExtensionFactory,
    cwd: &JsString,
    event_bus: Arc<dyn EventBus>,
    runtime: ExtensionRuntime,
    extension_path: &JsString,
) -> Result<Extension, HandlerError> {
    let extension = create_extension(extension_path, extension_path);
    let resolved_cwd = resolve_against_process_cwd(cwd, &default_path_options())?;
    let api = ExtensionApi::new(extension.clone(), runtime, resolved_cwd, event_bus);
    factory(&api).await?;
    time(&js_format!("{} factory", extension_path), TimingLabel::Extensions);
    Ok(extension)
}

/// Resolve extension entry points from a directory.
fn resolve_extension_entries(dir: &JsString) -> Option<Vec<JsString>> {
    let package_json_path = node_join(dir, "package.json");
    if Path::new(&encode_node_utf8(&package_json_path)).exists()
        && let Some(manifest) = read_pi_manifest(&package_json_path)
        && let Some(declared) = manifest.extensions.filter(|entries| !entries.is_empty())
    {
        let mut entries = Vec::new();
        for extension_path in declared {
            let resolved = if node_is_absolute(&extension_path) {
                node_normalize(extension_path)
            } else {
                node_join(dir, extension_path)
            };
            if Path::new(&encode_node_utf8(&resolved)).exists() {
                entries.push(resolved);
            }
        }
        if !entries.is_empty() {
            return Some(entries);
        }
    }

    let index_ts = node_join(dir, "index.ts");
    if Path::new(&encode_node_utf8(&index_ts)).exists() {
        return Some(vec![index_ts]);
    }
    let index_js = node_join(dir, "index.js");
    if Path::new(&encode_node_utf8(&index_js)).exists() {
        return Some(vec![index_js]);
    }
    None
}

fn is_extension_file(name: &JsString) -> bool {
    name.ends_with(".ts") || name.ends_with(".js")
}

/// Discover extension entry points one level below `dir`.
fn discover_extensions_in_dir(dir: &JsString) -> Vec<JsString> {
    if !Path::new(&encode_node_utf8(dir)).exists() {
        return Vec::new();
    }
    let Ok(entries) = std::fs::read_dir(encode_node_utf8(dir)) else {
        return Vec::new();
    };
    let mut discovered = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                eprintln!("Could not read extension directory entry: {error}");
                continue;
            }
        };
        // readdir returns UTF-8-decoded names; retain the original directory's units.
        let entry_name = JsString::from(entry.file_name().to_string_lossy().as_ref());
        let entry_path = node_join(dir, &entry_name);
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                eprintln!("Could not stat extension directory entry: {error}");
                continue;
            }
        };
        let is_symlink = file_type.is_symlink();
        if (file_type.is_file() || is_symlink) && is_extension_file(&entry_name) {
            discovered.push(entry_path.clone());
            continue;
        }
        if (file_type.is_dir() || is_symlink)
            && let Some(entries) = resolve_extension_entries(&entry_path)
        {
            discovered.extend(entries);
        }
    }
    discovered
}

/// Discover extension paths from project/global dirs and configured paths.
pub fn discover_extension_paths(
    configured_paths: &[JsString],
    cwd: &JsString,
    agent_dir: &JsString,
) -> Result<Vec<JsString>, HandlerError> {
    let resolved_cwd = resolve_against_process_cwd(cwd, &default_path_options())?;
    let resolved_agent_dir = resolve_against_process_cwd(agent_dir, &default_path_options())?;
    let mut all_paths: Vec<JsString> = Vec::new();
    let mut seen: Vec<JsString> = Vec::new();

    let add_paths = |paths: Vec<JsString>, all_paths: &mut Vec<JsString>, seen: &mut Vec<JsString>| {
        for path in paths {
            if !seen.contains(&path) {
                seen.push(path.clone());
                all_paths.push(path);
            }
        }
    };

    let local_ext_dir = node_join(
        node_join(&resolved_cwd, &package_config().config_dir_name),
        "extensions",
    );
    add_paths(discover_extensions_in_dir(&local_ext_dir), &mut all_paths, &mut seen);

    let global_ext_dir = node_join(&resolved_agent_dir, "extensions");
    add_paths(discover_extensions_in_dir(&global_ext_dir), &mut all_paths, &mut seen);

    for configured in configured_paths {
        let resolved = resolve_path(configured, &resolved_cwd, &normalize_unicode_space_options())?;
        let encoded = encode_node_utf8(&resolved);
        let resolved_path = Path::new(&encoded);
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

    Ok(all_paths)
}

fn first_party_extension_name(extension_path: &JsString) -> Option<JsString> {
    let prefix = "<first-party:";
    if extension_path.starts_with(prefix) && extension_path.ends_with('>') {
        return Some(extension_path.slice(prefix.len(), extension_path.len() - 1).to_owned());
    }
    None
}

/// Inline extension name used by disabled-extension filtering.
pub fn inline_extension_name(extension_path: &JsString) -> Option<JsString> {
    if let Some(name) = first_party_extension_name(extension_path) {
        return Some(name);
    }
    if extension_path.starts_with('<') && extension_path.ends_with('>') {
        return Some(extension_path.slice(1, extension_path.len() - 1).to_owned());
    }
    None
}

fn disabled_names(disabled_extensions: &[JsString]) -> Vec<JsString> {
    disabled_extensions
        .iter()
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
        .collect()
}

fn file_name(path: &JsString) -> JsString {
    node_basename(path)
}

/// Whether a filesystem extension path matches a disabled extension name.
pub fn is_extension_path_disabled(
    disabled_extensions: &[JsString],
    extension_path: &JsString,
    resolved_path: &JsString,
) -> bool {
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
pub fn is_extension_factory_disabled(disabled_extensions: &[JsString], extension_path: &JsString) -> bool {
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
        extension_path: &JsString,
        resolved_path: &JsString,
        cwd: &JsString,
        event_bus: Arc<dyn EventBus>,
        runtime: ExtensionRuntime,
    ) -> Result<Extension, ExtensionLoadError> {
        let Some(factory) = self.get(resolved_path).or_else(|| self.get(extension_path)).cloned() else {
            return Err(ExtensionLoadError {
                path: extension_path.to_owned(),
                error: js_format!("Extension does not export a valid factory function: {}", extension_path),
            });
        };
        time(&js_format!("{} module import", extension_path), TimingLabel::Extensions);
        let extension = create_extension(extension_path, resolved_path);
        let api = ExtensionApi::new(extension.clone(), runtime, cwd.clone(), event_bus);
        factory(&api).await.map_err(|error| ExtensionLoadError {
            path: extension_path.to_owned(),
            error: pi_js_compat::js_json::JsString::from("Failed to load extension: ") + error.message,
        })?;
        time(&js_format!("{} factory", extension_path), TimingLabel::Extensions);
        Ok(extension)
    }

    /// Load extensions from paths; load failures are collected, not thrown.
    pub async fn load_extensions(
        &self,
        paths: &[JsString],
        cwd: &JsString,
        event_bus: Option<Arc<dyn EventBus>>,
        runtime: Option<ExtensionRuntime>,
    ) -> Result<LoadExtensionsResult, HandlerError> {
        let event_bus = event_bus.unwrap_or_else(|| Arc::new(EventBusController::new()));
        let runtime = runtime.unwrap_or_else(create_extension_runtime);
        let resolved_cwd = resolve_against_process_cwd(cwd, &default_path_options())?;
        let mut extensions = Vec::new();
        let mut errors = Vec::new();
        for extension_path in paths {
            let resolved_path = resolve_path(extension_path, &resolved_cwd, &normalize_unicode_space_options())?;
            match self
                .load_extension(
                    extension_path,
                    &resolved_path,
                    &resolved_cwd,
                    Arc::clone(&event_bus),
                    runtime.clone(),
                )
                .await
            {
                Ok(extension) => extensions.push(extension),
                Err(error) => errors.push(error),
            }
        }
        Ok(LoadExtensionsResult {
            extensions,
            errors,
            runtime,
        })
    }

    /// Discover project/global/configured extensions and load the enabled ones.
    pub async fn discover_and_load_extensions(
        &self,
        configured_paths: &[JsString],
        cwd: &JsString,
        agent_dir: &JsString,
        event_bus: Option<Arc<dyn EventBus>>,
        disabled_extensions: &[JsString],
    ) -> Result<LoadExtensionsResult, HandlerError> {
        let resolved_cwd = resolve_against_process_cwd(cwd, &default_path_options())?;
        let paths = discover_extension_paths(configured_paths, cwd, agent_dir)?;
        let enabled: Vec<JsString> = paths
            .into_iter()
            .filter(|path| !is_extension_path_disabled(disabled_extensions, path, path))
            .collect();
        self.load_extensions(&enabled, &resolved_cwd, event_bus, None).await
    }
}

/// `loadExtensions` over an explicit path list.
pub async fn load_extensions(
    paths: &[JsString],
    cwd: &JsString,
    event_bus: Option<Arc<dyn EventBus>>,
    runtime: Option<ExtensionRuntime>,
) -> Result<LoadExtensionsResult, HandlerError> {
    ExtensionRegistry::new()
        .load_extensions(paths, cwd, event_bus, runtime)
        .await
}

/// Cached loading is identical for static Rust factories.
pub async fn load_extensions_cached(
    paths: &[JsString],
    cwd: &JsString,
    event_bus: Option<Arc<dyn EventBus>>,
    runtime: Option<ExtensionRuntime>,
) -> Result<LoadExtensionsResult, HandlerError> {
    load_extensions(paths, cwd, event_bus, runtime).await
}

/// TypeScript module caching has no Rust counterpart.
pub fn clear_extension_cache() {}

/// `discoverAndLoadExtensions` for the default registry.
pub async fn discover_and_load_extensions(
    configured_paths: &[JsString],
    cwd: &JsString,
    agent_dir: &JsString,
    event_bus: Option<Arc<dyn EventBus>>,
) -> Result<LoadExtensionsResult, HandlerError> {
    ExtensionRegistry::new()
        .discover_and_load_extensions(configured_paths, cwd, agent_dir, event_bus, &[])
        .await
}

/// Convenience: create the API used by inline test extensions.
pub fn create_extension_api(
    extension: Extension,
    runtime: ExtensionRuntime,
    cwd: &JsString,
    event_bus: Arc<dyn EventBus>,
) -> ExtensionApi {
    ExtensionApi::new(extension, runtime, cwd.to_owned(), event_bus)
}

/// Re-exported helper used by extension registries building commands.
pub fn command_handler<F>(handler: F) -> CommandHandler
where
    F: Fn(
            pi_js_compat::js_json::JsString,
            super::types::ExtensionCommandContext,
        ) -> BoxFuture<'static, Result<(), HandlerError>>
        + Send
        + Sync
        + 'static,
{
    Arc::new(handler)
}

/// Re-exported helper used by extension registries building shortcuts.
pub fn shortcut_handler<F>(handler: F) -> crate::core::extensions::types::ContextShortcutHandler
where
    F: Fn(super::types::ExtensionContext) -> BoxFuture<'static, Result<(), HandlerError>> + Send + Sync + 'static,
{
    Arc::new(handler)
}

/// Re-exported event-bus constructor for extension factories.
pub fn extension_event_bus(runtime: ExtensionRuntime, bus: Arc<dyn EventBus>) -> ExtensionEventBus {
    ExtensionEventBus::new(bus, runtime)
}

/// Re-exported placeholder for extension command metadata.
pub fn registered_command(
    name: impl Into<JsString>,
    source_info: crate::core::source_info::SourceInfo,
    description: Option<JsString>,
    handler: CommandHandler,
) -> RegisteredCommand {
    RegisteredCommand {
        name: name.into(),
        source_info,
        description,
        get_argument_completions: None,
        handler,
    }
}

/// Re-exported placeholder for shortcut metadata.
pub fn extension_shortcut(
    shortcut: &JsString,
    description: Option<JsString>,
    handler: crate::core::extensions::types::ContextShortcutHandler,
    extension_path: &JsString,
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
pub fn manifest_extension_value(value: &JsValue) -> Option<JsString> {
    value.as_string().cloned()
}
