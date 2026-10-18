//! Resource discovery (`core/resource-loader.ts`).

use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use indexmap::IndexMap;
use serde::Serialize;

use pi_ai::request::BoxFuture;
use pi_js_compat::locale_compare;

use crate::config::package_config;
use crate::core::diagnostics::{ResourceCollision, ResourceDiagnostic, ResourceDiagnosticType, ResourceType};
use crate::core::event_bus::{EventBus, create_event_bus};
use crate::core::extensions::extension_runtime::create_extension_runtime;
use crate::core::extensions::loader::{
    ExtensionFactory, ExtensionRegistry, clear_extension_cache, is_extension_factory_disabled,
    is_extension_path_disabled, load_extension_from_factory,
};
use crate::core::extensions::types::LoadExtensionsResult;
use crate::core::footer_data_provider::find_git_paths;
use crate::core::package_manager::{
    DefaultPackageManager, MetadataOrigin, PathMetadata, ResolvedResource, SettingsManagerStore,
};
use crate::core::prompt_templates::{LoadPromptTemplatesOptions, PromptTemplate, load_prompt_templates};
use crate::core::settings_manager::SettingsManager;
use crate::core::skills::{LoadSkillsOptions, Skill, load_skills};
use crate::core::source_info::{
    SourceInfo, SourceOrigin, SourceScope, create_source_info, create_synthetic_source_info,
};
use crate::core::system_prompt::ContextFile;
use crate::core::timings::{TimingLabel, reset_timings};
use crate::modes::interactive::theme::{Theme, load_theme_from_path};
use crate::utils::paths::{PathInputOptions, canonicalize_path, is_local_path, resolve_path};

/// `<first-party:name>` paths name a bundled extension.
pub fn first_party_extension_name(extension_path: &str) -> Option<&str> {
    let name = extension_path.strip_prefix("<first-party:")?;
    name.strip_suffix('>')
}

/// `<name>` and `<first-party:name>` paths name inline or bundled extensions.
pub fn inline_extension_name(extension_path: &str) -> Option<&str> {
    if let Some(name) = first_party_extension_name(extension_path) {
        return Some(name);
    }
    if extension_path.starts_with('<') && extension_path.ends_with('>') {
        return extension_path.get(1..extension_path.len() - 1);
    }
    None
}

/// Reads a prompt option that may be a file path or literal text.
pub fn resolve_prompt_input(input: Option<&str>, description: &str) -> Option<String> {
    let input = input.filter(|value| !value.is_empty())?;
    if !Path::new(input).exists() {
        return Some(input.to_owned());
    }

    match fs::read_to_string(input) {
        Ok(content) => Some(content),
        Err(error) => {
            eprintln!("Warning: Could not read {description} file {input}: {error}");
            Some(input.to_owned())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleFile {
    pub path: String,
    pub content: String,
}

/// Loads non-empty `.md` rule files in locale order.
pub fn load_rules_files_from_dir(dir_path: &str) -> std::io::Result<Vec<RuleFile>> {
    let path = Path::new(dir_path);
    if !path.exists() || !path.is_dir() {
        return Ok(Vec::new());
    }

    let mut names: Vec<String> = fs::read_dir(path)?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.ends_with(".md"))
        .collect();
    names.sort_by(|left, right| locale_compare(left, right));

    let mut files = Vec::new();
    for name in names {
        let file_path = path.join(&name);
        let content = fs::read_to_string(&file_path)?.trim().to_owned();
        if content.is_empty() {
            continue;
        }
        files.push(RuleFile {
            path: file_path.to_string_lossy().into_owned(),
            content,
        });
    }
    Ok(files)
}

/// Joins the non-empty rule files with a blank line.
pub fn load_rules_from_dir(dir_path: &str) -> std::io::Result<Option<String>> {
    let contents: Vec<String> = load_rules_files_from_dir(dir_path)?
        .into_iter()
        .map(|file| file.content)
        .collect();
    Ok((!contents.is_empty()).then(|| contents.join("\n\n")))
}

/// Rule directories loaded in addition to the shared top-level rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RulesScope {
    Shared,
    Main,
    Child,
    Architect,
}

impl RulesScope {
    /// Returns the exact TypeScript source literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Shared => "shared",
            Self::Main => "main",
            Self::Child => "child",
            Self::Architect => "architect",
        }
    }
}

const AGENTS_CONTEXT_FILE_CANDIDATES: [&str; 5] = [
    "AGENTS.override.md",
    "AGENTS.md",
    "AGENTS.local.md",
    "AGENTS.MD",
    "AGENTS.local.MD",
];
const CLAUDE_CONTEXT_FILE_CANDIDATES: [&str; 4] = ["CLAUDE.md", "CLAUDE.local.md", "CLAUDE.MD", "CLAUDE.local.MD"];
const PROJECT_MEMORY_RELATIVE_PATH: [&str; 3] = ["docs", "local", "memory.md"];

#[derive(Debug, Clone)]
struct LoadedContextFile {
    path: String,
    content: String,
    real_path: String,
}

fn load_context_files_from_dir(dir: &str, candidates: &[&str]) -> Vec<LoadedContextFile> {
    let mut context_files = Vec::new();
    let ordered_candidates: Vec<&str> = if candidates.contains(&"AGENTS.override.md") {
        let mut ordered = vec!["AGENTS.override.md"];
        ordered.extend(candidates.iter().copied().filter(|name| *name != "AGENTS.override.md"));
        ordered
    } else {
        candidates.to_vec()
    };

    for filename in ordered_candidates {
        let file_path = Path::new(dir).join(filename);
        if !file_path.exists() {
            continue;
        }
        let metadata = match fs::metadata(&file_path) {
            Ok(metadata) => metadata,
            Err(error) => {
                eprintln!("Warning: Could not read {}: {error}", file_path.display());
                continue;
            }
        };
        if !metadata.is_file() {
            continue;
        }
        match fs::read_to_string(&file_path) {
            Ok(content) => {
                let real_path = fs::canonicalize(&file_path).map_or_else(
                    |_| file_path.to_string_lossy().into_owned(),
                    |path| path.to_string_lossy().into_owned(),
                );
                context_files.push(LoadedContextFile {
                    path: file_path.to_string_lossy().into_owned(),
                    content,
                    real_path,
                });
                if filename == "AGENTS.override.md" {
                    return context_files;
                }
            }
            Err(error) => eprintln!("Warning: Could not read {}: {error}", file_path.display()),
        }
    }
    context_files
}

fn load_instruction_context_files_from_dirs(dirs: &[String]) -> (Vec<&'static str>, Vec<Vec<LoadedContextFile>>) {
    let agents_files_by_dir: Vec<Vec<LoadedContextFile>> = dirs
        .iter()
        .map(|dir| load_context_files_from_dir(dir, &AGENTS_CONTEXT_FILE_CANDIDATES))
        .collect();
    if agents_files_by_dir.iter().any(|files| !files.is_empty()) {
        return (AGENTS_CONTEXT_FILE_CANDIDATES.to_vec(), agents_files_by_dir);
    }
    (
        CLAUDE_CONTEXT_FILE_CANDIDATES.to_vec(),
        dirs.iter()
            .map(|dir| load_context_files_from_dir(dir, &CLAUDE_CONTEXT_FILE_CANDIDATES))
            .collect(),
    )
}

fn append_unique_context_files(
    context_files: &mut Vec<ContextFile>,
    loaded_files: Vec<LoadedContextFile>,
    seen_paths: &mut HashSet<String>,
) {
    for context_file in loaded_files {
        if seen_paths.contains(&context_file.real_path) {
            continue;
        }
        context_files.push(ContextFile {
            path: context_file.path,
            content: context_file.content,
        });
        seen_paths.insert(context_file.real_path);
    }
}

fn list_ancestor_dirs(cwd: &str) -> Vec<String> {
    let mut ancestor_dirs: Vec<String> = Path::new(cwd)
        .ancestors()
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    ancestor_dirs.reverse();
    ancestor_dirs
}

fn find_shadowed_context_file(cwd: &str, candidates: &[&str]) -> Option<String> {
    let git_paths = find_git_paths(Path::new(cwd))?;
    let common_git_dir = canonicalize_path(&git_paths.common_git_dir.to_string_lossy());
    let worktree_root = canonicalize_path(&git_paths.repo_dir.to_string_lossy());
    let main_repo_root = Path::new(&common_git_dir)
        .parent()
        .map(|path| path.to_string_lossy().into_owned())?;
    let prefix = format!("{main_repo_root}{}", std::path::MAIN_SEPARATOR);
    if !worktree_root.starts_with(&prefix) {
        return None;
    }
    if canonicalize_path(&Path::new(&main_repo_root).join(".git").to_string_lossy()) != common_git_dir {
        return None;
    }
    let worktree_context_file = load_context_files_from_dir(&worktree_root, candidates)
        .into_iter()
        .next()?;
    let name = Path::new(&worktree_context_file.path).file_name()?;
    Some(Path::new(&main_repo_root).join(name).to_string_lossy().into_owned())
}

fn process_cwd() -> String {
    std::env::current_dir().map_or_else(|_| "/".to_owned(), |path| path.to_string_lossy().into_owned())
}

fn resolve_from_process_cwd(input: &str) -> String {
    resolve_path(input, &process_cwd(), &PathInputOptions::default()).unwrap_or_else(|_| input.to_owned())
}

/// Loads `AGENTS.md`/`CLAUDE.md` context files from the agent dir and cwd ancestors.
pub fn load_project_context_files(cwd: &str, agent_dir: &str) -> Vec<ContextFile> {
    let resolved_cwd = resolve_from_process_cwd(cwd);
    let resolved_agent_dir = resolve_from_process_cwd(agent_dir);
    let global_project_memory_real_path = canonicalize_path(
        &Path::new(&resolved_agent_dir)
            .join(PROJECT_MEMORY_RELATIVE_PATH.join("/"))
            .to_string_lossy(),
    );
    let ancestor_dirs = list_ancestor_dirs(&resolved_cwd);
    let mut instruction_dirs = vec![resolved_agent_dir.clone()];
    instruction_dirs.extend(ancestor_dirs.iter().cloned());
    let (candidates, files_by_dir) = load_instruction_context_files_from_dirs(&instruction_dirs);
    let mut context_files = Vec::new();
    let mut seen_paths = HashSet::new();
    append_unique_context_files(
        &mut context_files,
        files_by_dir.first().cloned().unwrap_or_default(),
        &mut seen_paths,
    );

    let shadowed_context_file = find_shadowed_context_file(&resolved_cwd, &candidates);
    for (index, ancestor_dir) in ancestor_dirs.iter().enumerate() {
        let project_memory_path = Path::new(ancestor_dir)
            .join(PROJECT_MEMORY_RELATIVE_PATH.join("/"))
            .to_string_lossy()
            .into_owned();
        let mut loaded_context_files = files_by_dir.get(index + 1).cloned().unwrap_or_default();
        loaded_context_files.extend(load_context_files_from_dir(
            ancestor_dir,
            &[&PROJECT_MEMORY_RELATIVE_PATH.join("/")],
        ));
        let project_context_files: Vec<LoadedContextFile> = loaded_context_files
            .into_iter()
            .filter(|context_file| {
                (context_file.path != project_memory_path || context_file.real_path != global_project_memory_real_path)
                    && shadowed_context_file
                        .as_ref()
                        .is_none_or(|shadowed| canonicalize_path(&context_file.path) != *shadowed)
            })
            .collect();
        append_unique_context_files(&mut context_files, project_context_files, &mut seen_paths);
    }

    context_files
}

// =============================================================================
// ResourceLoader contract used by AgentSession / sdk
// =============================================================================

/// One extension-provided resource path (`{ path, metadata }`).
#[derive(Debug, Clone, PartialEq)]
pub struct ExtendedResourcePath {
    pub path: String,
    pub metadata: PathMetadata,
}

/// `ResourceExtensionPaths` supplied by extension `resources_discover` handlers.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResourceExtensionPaths {
    pub skill_paths: Vec<ExtendedResourcePath>,
    pub prompt_paths: Vec<ExtendedResourcePath>,
    pub theme_paths: Vec<ExtendedResourcePath>,
}

/// Loaded skills plus diagnostics.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LoadedSkills {
    pub skills: Vec<Skill>,
    pub diagnostics: Vec<ResourceDiagnostic>,
}

/// Loaded prompt templates plus diagnostics.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LoadedPrompts {
    pub prompts: Vec<PromptTemplate>,
    pub diagnostics: Vec<ResourceDiagnostic>,
}

/// Loaded themes plus diagnostics.
#[derive(Clone, Default)]
pub struct LoadedThemes {
    pub themes: Vec<Theme>,
    pub diagnostics: Vec<ResourceDiagnostic>,
}

/// Loaded context files (`AGENTS.md`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LoadedAgentsFiles {
    pub agents_files: Vec<ContextFile>,
}

/// Loaded rule files.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LoadedRulesFiles {
    pub rules_files: Vec<RuleFile>,
}

/// A file-backed prompt source (`{ path }`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedPathSource {
    pub path: String,
}

/// `ResourceLoader` interface consumed by AgentSession.
pub trait ResourceLoader: Send + Sync {
    /// Extension load result (`{ extensions, errors, runtime }`).
    fn get_extensions(&self) -> LoadExtensionsResult;
    fn get_skills(&self) -> LoadedSkills;
    fn get_prompts(&self) -> LoadedPrompts;
    fn get_themes(&self) -> LoadedThemes;
    fn get_agents_files(&self) -> LoadedAgentsFiles;
    fn get_rules_files(&self) -> LoadedRulesFiles;
    fn get_rules_content(&self) -> Option<String>;
    fn get_system_prompt(&self) -> Option<String>;
    fn get_system_prompt_source(&self) -> Option<LoadedPathSource>;
    fn get_append_system_prompt(&self) -> Vec<String>;
    fn get_append_system_prompt_sources(&self) -> Vec<LoadedPathSource>;
    fn extend_resources(&self, paths: ResourceExtensionPaths);
    fn reload(&self) -> BoxFuture<'_, Result<(), String>>;
}

/// Empty loader used by tests and callers without resource discovery.
#[derive(Default)]
pub struct StaticResourceLoader {
    pub extensions: Option<LoadExtensionsResult>,
    pub skills: LoadedSkills,
    pub prompts: LoadedPrompts,
    pub themes: LoadedThemes,
    pub agents_files: LoadedAgentsFiles,
    pub rules_files: LoadedRulesFiles,
    pub rules_content: Option<String>,
    pub system_prompt: Option<String>,
    pub system_prompt_source: Option<LoadedPathSource>,
    pub append_system_prompt: Vec<String>,
    pub append_system_prompt_sources: Vec<LoadedPathSource>,
}

impl StaticResourceLoader {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
}

impl ResourceLoader for StaticResourceLoader {
    fn get_extensions(&self) -> LoadExtensionsResult {
        self.extensions.clone().unwrap_or_else(|| LoadExtensionsResult {
            extensions: Vec::new(),
            errors: Vec::new(),
            runtime: create_extension_runtime(),
        })
    }

    fn get_skills(&self) -> LoadedSkills {
        self.skills.clone()
    }

    fn get_prompts(&self) -> LoadedPrompts {
        self.prompts.clone()
    }

    fn get_themes(&self) -> LoadedThemes {
        self.themes.clone()
    }

    fn get_agents_files(&self) -> LoadedAgentsFiles {
        self.agents_files.clone()
    }

    fn get_rules_files(&self) -> LoadedRulesFiles {
        self.rules_files.clone()
    }

    fn get_rules_content(&self) -> Option<String> {
        self.rules_content.clone()
    }

    fn get_system_prompt(&self) -> Option<String> {
        self.system_prompt.clone()
    }

    fn get_system_prompt_source(&self) -> Option<LoadedPathSource> {
        self.system_prompt_source.clone()
    }

    fn get_append_system_prompt(&self) -> Vec<String> {
        self.append_system_prompt.clone()
    }

    fn get_append_system_prompt_sources(&self) -> Vec<LoadedPathSource> {
        self.append_system_prompt_sources.clone()
    }

    fn extend_resources(&self, _paths: ResourceExtensionPaths) {}

    fn reload(&self) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async { Ok(()) })
    }
}

// =============================================================================
// DefaultResourceLoader
// =============================================================================

/// An inline extension factory (`InlineExtension` in the source).
#[derive(Clone)]
pub enum InlineExtension {
    /// A factory function carrying an optional `extensionPath` property.
    Factory {
        extension_path: Option<String>,
        factory: ExtensionFactory,
    },
    /// `{ name, factory, hidden? }`.
    Named {
        name: String,
        factory: ExtensionFactory,
        hidden: Option<bool>,
    },
}

impl InlineExtension {
    fn factory(&self) -> ExtensionFactory {
        match self {
            Self::Factory { factory, .. } | Self::Named { factory, .. } => Arc::clone(factory),
        }
    }

    fn extension_path(&self, index: usize) -> String {
        match self {
            Self::Named { name, .. } => format!("<inline:{name}>"),
            Self::Factory { extension_path, .. } => extension_path
                .clone()
                .unwrap_or_else(|| format!("<inline:{}>", index + 1)),
        }
    }

    fn is_named(&self) -> bool {
        matches!(self, Self::Named { .. })
    }

    fn hidden(&self) -> bool {
        matches!(self, Self::Named { hidden: Some(true), .. })
    }
}

pub type ExtensionsOverride = Arc<dyn Fn(LoadExtensionsResult) -> LoadExtensionsResult + Send + Sync>;
pub type SkillsOverride = Arc<dyn Fn(LoadedSkills) -> LoadedSkills + Send + Sync>;
pub type PromptsOverride = Arc<dyn Fn(LoadedPrompts) -> LoadedPrompts + Send + Sync>;
pub type ThemesOverride = Arc<dyn Fn(LoadedThemes) -> LoadedThemes + Send + Sync>;
pub type AgentsFilesOverride = Arc<dyn Fn(LoadedAgentsFiles) -> LoadedAgentsFiles + Send + Sync>;
pub type SystemPromptOverride = Arc<dyn Fn(Option<&str>) -> Option<String> + Send + Sync>;
pub type AppendSystemPromptOverride = Arc<dyn Fn(&[String]) -> Vec<String> + Send + Sync>;
/// `ResourceLoaderReloadOptions.resolveProjectTrust`.
pub type ResolveProjectTrust =
    Arc<dyn Fn(LoadExtensionsResult) -> BoxFuture<'static, Result<bool, String>> + Send + Sync>;

/// `DefaultResourceLoaderOptions`.
#[derive(Default)]
pub struct DefaultResourceLoaderOptions {
    pub cwd: String,
    pub agent_dir: String,
    pub settings_manager: Option<Arc<Mutex<SettingsManager>>>,
    pub event_bus: Option<Arc<dyn EventBus>>,
    pub extension_registry: Option<Arc<ExtensionRegistry>>,
    pub additional_extension_paths: Vec<String>,
    pub additional_skill_paths: Vec<String>,
    pub additional_prompt_template_paths: Vec<String>,
    pub additional_theme_paths: Vec<String>,
    pub extension_factories: Vec<InlineExtension>,
    pub no_extensions: bool,
    pub no_skills: bool,
    pub no_prompt_templates: bool,
    pub no_themes: bool,
    pub no_context_files: bool,
    pub rules_scope: Option<RulesScope>,
    pub system_prompt: Option<String>,
    pub append_system_prompt: Option<Vec<String>>,
    pub extensions_override: Option<ExtensionsOverride>,
    pub skills_override: Option<SkillsOverride>,
    pub prompts_override: Option<PromptsOverride>,
    pub themes_override: Option<ThemesOverride>,
    pub agents_files_override: Option<AgentsFilesOverride>,
    pub system_prompt_override: Option<SystemPromptOverride>,
    pub append_system_prompt_override: Option<AppendSystemPromptOverride>,
}

struct ResourceLoaderState {
    extensions_result: LoadExtensionsResult,
    skills: Vec<Skill>,
    skill_diagnostics: Vec<ResourceDiagnostic>,
    prompts: Vec<PromptTemplate>,
    prompt_diagnostics: Vec<ResourceDiagnostic>,
    themes: Vec<Theme>,
    theme_diagnostics: Vec<ResourceDiagnostic>,
    agents_files: Vec<ContextFile>,
    rules_files: Vec<RuleFile>,
    rules_content: Option<String>,
    system_prompt: Option<String>,
    system_prompt_source_path: Option<String>,
    append_system_prompt: Vec<String>,
    append_system_prompt_source_paths: Vec<String>,
    last_skill_paths: Vec<String>,
    last_prompt_paths: Vec<String>,
    last_theme_paths: Vec<String>,
    extension_skill_source_infos: IndexMap<String, SourceInfo>,
    extension_prompt_source_infos: IndexMap<String, SourceInfo>,
    extension_theme_source_infos: IndexMap<String, SourceInfo>,
    resource_metadata_by_path: IndexMap<String, PathMetadata>,
    loaded: bool,
}

impl Default for ResourceLoaderState {
    fn default() -> Self {
        Self {
            extensions_result: LoadExtensionsResult {
                extensions: Vec::new(),
                errors: Vec::new(),
                runtime: create_extension_runtime(),
            },
            skills: Vec::new(),
            skill_diagnostics: Vec::new(),
            prompts: Vec::new(),
            prompt_diagnostics: Vec::new(),
            themes: Vec::new(),
            theme_diagnostics: Vec::new(),
            agents_files: Vec::new(),
            rules_files: Vec::new(),
            rules_content: None,
            system_prompt: None,
            system_prompt_source_path: None,
            append_system_prompt: Vec::new(),
            append_system_prompt_source_paths: Vec::new(),
            last_skill_paths: Vec::new(),
            last_prompt_paths: Vec::new(),
            last_theme_paths: Vec::new(),
            extension_skill_source_infos: IndexMap::new(),
            extension_prompt_source_infos: IndexMap::new(),
            extension_theme_source_infos: IndexMap::new(),
            resource_metadata_by_path: IndexMap::new(),
            loaded: false,
        }
    }
}

/// Resource loader used by the SDK and session services.
pub struct DefaultResourceLoader {
    cwd: String,
    agent_dir: String,
    settings_manager: Arc<Mutex<SettingsManager>>,
    event_bus: Arc<dyn EventBus>,
    package_manager: DefaultPackageManager,
    extension_registry: Arc<ExtensionRegistry>,
    additional_extension_paths: Vec<String>,
    additional_skill_paths: Vec<String>,
    additional_prompt_template_paths: Vec<String>,
    additional_theme_paths: Vec<String>,
    extension_factories: Vec<InlineExtension>,
    no_extensions: bool,
    no_skills: bool,
    no_prompt_templates: bool,
    no_themes: bool,
    no_context_files: bool,
    rules_scope: RulesScope,
    system_prompt_source: Option<String>,
    append_system_prompt_source: Option<Vec<String>>,
    extensions_override: Option<ExtensionsOverride>,
    skills_override: Option<SkillsOverride>,
    prompts_override: Option<PromptsOverride>,
    themes_override: Option<ThemesOverride>,
    agents_files_override: Option<AgentsFilesOverride>,
    system_prompt_override: Option<SystemPromptOverride>,
    append_system_prompt_override: Option<AppendSystemPromptOverride>,
    state: Mutex<ResourceLoaderState>,
}

impl DefaultResourceLoader {
    pub fn new(options: DefaultResourceLoaderOptions) -> Result<Self, String> {
        let cwd = resolve_from_process_cwd(&options.cwd);
        let agent_dir = resolve_from_process_cwd(&options.agent_dir);
        let settings_manager = match options.settings_manager {
            Some(manager) => manager,
            None => Arc::new(Mutex::new(
                SettingsManager::create(&cwd, Some(Path::new(&agent_dir)), Default::default())
                    .map_err(|error| error.to_string())?,
            )),
        };
        let package_manager = DefaultPackageManager::new(
            &cwd,
            &agent_dir,
            Arc::new(SettingsManagerStore::from_shared(Arc::clone(&settings_manager))),
        );
        Ok(Self {
            cwd,
            agent_dir,
            settings_manager,
            event_bus: options.event_bus.unwrap_or_else(|| Arc::new(create_event_bus())),
            package_manager,
            extension_registry: options
                .extension_registry
                .unwrap_or_else(|| Arc::new(ExtensionRegistry::new())),
            additional_extension_paths: options.additional_extension_paths,
            additional_skill_paths: options.additional_skill_paths,
            additional_prompt_template_paths: options.additional_prompt_template_paths,
            additional_theme_paths: options.additional_theme_paths,
            extension_factories: options.extension_factories,
            no_extensions: options.no_extensions,
            no_skills: options.no_skills,
            no_prompt_templates: options.no_prompt_templates,
            no_themes: options.no_themes,
            no_context_files: options.no_context_files,
            rules_scope: options.rules_scope.unwrap_or(RulesScope::Main),
            system_prompt_source: options.system_prompt,
            append_system_prompt_source: options.append_system_prompt,
            extensions_override: options.extensions_override,
            skills_override: options.skills_override,
            prompts_override: options.prompts_override,
            themes_override: options.themes_override,
            agents_files_override: options.agents_files_override,
            system_prompt_override: options.system_prompt_override,
            append_system_prompt_override: options.append_system_prompt_override,
            state: Mutex::new(ResourceLoaderState::default()),
        })
    }

    fn settings(&self) -> std::sync::MutexGuard<'_, SettingsManager> {
        self.settings_manager
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn state(&self) -> std::sync::MutexGuard<'_, ResourceLoaderState> {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn resolve_resource_path(&self, path: &str) -> String {
        resolve_path(
            path,
            &self.cwd,
            &PathInputOptions {
                trim: true,
                ..PathInputOptions::default()
            },
        )
        .unwrap_or_else(|_| path.to_owned())
    }

    fn resolve_extension_load_path(&self, path: &str) -> String {
        resolve_path(
            path,
            &self.cwd,
            &PathInputOptions {
                normalize_unicode_spaces: true,
                ..PathInputOptions::default()
            },
        )
        .unwrap_or_else(|_| path.to_owned())
    }

    fn merge_paths(&self, primary: &[String], additional: &[String]) -> Vec<String> {
        let mut merged = Vec::new();
        let mut seen = HashSet::new();
        for path in primary.iter().chain(additional) {
            let resolved = self.resolve_resource_path(path);
            let canonical_path = canonicalize_path(&resolved);
            if seen.contains(&canonical_path) {
                continue;
            }
            seen.insert(canonical_path);
            merged.push(resolved);
        }
        merged
    }

    fn disabled_extension_names(&self) -> HashSet<String> {
        self.settings()
            .get_disabled_extensions()
            .into_iter()
            .map(|name| name.trim().to_owned())
            .filter(|name| !name.is_empty())
            .collect()
    }

    fn is_extension_path_disabled_with(&self, disabled: &HashSet<String>, extension_path: &str) -> bool {
        if disabled.is_empty() {
            return false;
        }
        let resolved_path = self.resolve_resource_path(extension_path);
        is_extension_path_disabled(
            &disabled.iter().cloned().collect::<Vec<_>>(),
            extension_path,
            &resolved_path,
        )
    }

    fn is_extension_factory_disabled_with(&self, disabled: &HashSet<String>, extension_path: &str) -> bool {
        if disabled.is_empty() {
            return false;
        }
        is_extension_factory_disabled(&disabled.iter().cloned().collect::<Vec<_>>(), extension_path)
    }

    fn filter_disabled_extension_paths(&self, extension_paths: Vec<String>) -> Vec<String> {
        let disabled = self.disabled_extension_names();
        extension_paths
            .into_iter()
            .filter(|path| !self.is_extension_path_disabled_with(&disabled, path))
            .collect()
    }

    /// Forces untrusted project settings and loads user/global plus temporary extensions.
    pub async fn load_project_trust_extensions(&self) -> Result<LoadExtensionsResult, String> {
        {
            let mut settings = self.settings();
            settings.set_project_trusted(false);
            settings.reload();
        }
        self.load_current_extension_set(true).await
    }

    /// Reloads every resource for the current settings and trust state.
    pub async fn reload_with(&self, resolve_project_trust: Option<ResolveProjectTrust>) -> Result<(), String> {
        reset_timings(TimingLabel::Extensions);
        if self.state().loaded {
            clear_extension_cache();
        }

        let pre_trust_extensions = self.load_pre_trust_extensions(resolve_project_trust).await?;
        self.settings().reload();
        let mut resource_sets = self.resolve_resource_sets().await?;
        let mut state = ResourceLoaderState::default();

        self.load_extensions_into(&mut state, &resource_sets, pre_trust_extensions)
            .await?;
        state.resource_metadata_by_path = std::mem::take(&mut resource_sets.metadata_by_path);
        self.load_skills_into(&mut state, &resource_sets)?;
        self.load_prompts_into(&mut state, &resource_sets)?;
        self.load_themes_into(&mut state, &resource_sets)?;
        self.load_agents_files_into(&mut state);
        self.load_rules_into(&mut state)?;
        self.load_system_prompts_into(&mut state);

        state.loaded = true;
        *self.state() = state;
        Ok(())
    }

    async fn load_pre_trust_extensions(
        &self,
        resolve_project_trust: Option<ResolveProjectTrust>,
    ) -> Result<Option<LoadExtensionsResult>, String> {
        let Some(resolver) = resolve_project_trust else {
            return Ok(None);
        };
        let loaded = self.load_project_trust_extensions().await?;
        let project_trusted = resolver(loaded.clone()).await?;
        self.settings().set_project_trusted(project_trusted);
        Ok(Some(loaded))
    }

    async fn resolve_resource_sets(&self) -> Result<ResolvedResourceSets, String> {
        let resolved_paths = self
            .package_manager
            .resolve(None)
            .await
            .map_err(|error| error.to_string())?;
        let cli_paths = self
            .package_manager
            .resolve_extension_sources(&self.additional_extension_paths, false, true)
            .await
            .map_err(|error| error.to_string())?;

        let mut metadata_by_path: IndexMap<String, PathMetadata> = IndexMap::new();
        let enabled_extensions = enabled_paths(&resolved_paths.extensions, &mut metadata_by_path);
        let enabled_skill_resources = enabled_resources(&resolved_paths.skills, &mut metadata_by_path);
        let enabled_prompts = enabled_paths(&resolved_paths.prompts, &mut metadata_by_path);
        let enabled_themes = enabled_paths(&resolved_paths.themes, &mut metadata_by_path);
        let enabled_skills: Vec<String> = enabled_skill_resources
            .iter()
            .map(|resource| map_skill_path(resource, &mut metadata_by_path))
            .collect();

        for resource in cli_paths.extensions.iter().chain(cli_paths.skills.iter()) {
            metadata_by_path.entry(resource.path.clone()).or_insert(PathMetadata {
                source: "cli".to_owned(),
                scope: crate::core::package_manager::SourceScope::Temporary,
                origin: MetadataOrigin::TopLevel,
                base_dir: None,
            });
        }

        Ok(ResolvedResourceSets {
            cli_extensions: enabled_paths(&cli_paths.extensions, &mut metadata_by_path),
            cli_skills: enabled_paths(&cli_paths.skills, &mut metadata_by_path),
            cli_prompts: enabled_paths(&cli_paths.prompts, &mut metadata_by_path),
            cli_themes: enabled_paths(&cli_paths.themes, &mut metadata_by_path),
            enabled_extensions,
            enabled_skills,
            enabled_prompts,
            enabled_themes,
            metadata_by_path,
        })
    }

    async fn load_extensions_into(
        &self,
        state: &mut ResourceLoaderState,
        resource_sets: &ResolvedResourceSets,
        pre_trust_extensions: Option<LoadExtensionsResult>,
    ) -> Result<(), String> {
        let primary_paths = if self.no_extensions {
            resource_sets.cli_extensions.clone()
        } else {
            self.merge_paths(&resource_sets.cli_extensions, &resource_sets.enabled_extensions)
        };
        let extension_paths = self.filter_disabled_extension_paths(primary_paths);
        let mut extensions_result = self
            .load_final_extension_set(&extension_paths, pre_trust_extensions)
            .await;

        for path in &self.additional_extension_paths {
            if !is_local_path(path) {
                continue;
            }
            let resolved = self.resolve_resource_path(path);
            if !Path::new(&resolved).exists() {
                extensions_result
                    .errors
                    .push(crate::core::extensions::types::ExtensionLoadError {
                        path: resolved.clone(),
                        error: format!("Extension path does not exist: {resolved}"),
                    });
            }
        }

        if let Some(override_fn) = &self.extensions_override {
            extensions_result = override_fn(extensions_result);
        }
        self.apply_extension_source_info(&mut extensions_result.extensions, &resource_sets.metadata_by_path)?;
        state.extensions_result = extensions_result;
        Ok(())
    }

    fn load_skills_into(
        &self,
        state: &mut ResourceLoaderState,
        resource_sets: &ResolvedResourceSets,
    ) -> Result<(), String> {
        let mut primary_paths = resource_sets.cli_skills.clone();
        if !self.no_skills {
            primary_paths.extend(resource_sets.enabled_skills.iter().cloned());
        }
        let skill_paths = self.merge_paths(&primary_paths, &self.additional_skill_paths);
        state.last_skill_paths = skill_paths.clone();
        self.update_skills_from_paths(state, &skill_paths)?;
        self.push_missing_path_diagnostics(
            &mut state.skill_diagnostics,
            &self.additional_skill_paths,
            "Skill path does not exist",
            PathCheck::Local,
        );
        Ok(())
    }

    fn load_prompts_into(
        &self,
        state: &mut ResourceLoaderState,
        resource_sets: &ResolvedResourceSets,
    ) -> Result<(), String> {
        let mut primary_paths = resource_sets.cli_prompts.clone();
        if !self.no_prompt_templates {
            primary_paths.extend(resource_sets.enabled_prompts.iter().cloned());
        }
        let prompt_paths = self.merge_paths(&primary_paths, &self.additional_prompt_template_paths);
        state.last_prompt_paths = prompt_paths.clone();
        self.update_prompts_from_paths(state, &prompt_paths)?;
        self.push_missing_path_diagnostics(
            &mut state.prompt_diagnostics,
            &self.additional_prompt_template_paths,
            "Prompt template path does not exist",
            PathCheck::Local,
        );
        Ok(())
    }

    fn load_themes_into(
        &self,
        state: &mut ResourceLoaderState,
        resource_sets: &ResolvedResourceSets,
    ) -> Result<(), String> {
        let mut primary_paths = resource_sets.cli_themes.clone();
        if !self.no_themes {
            primary_paths.extend(resource_sets.enabled_themes.iter().cloned());
        }
        let theme_paths = self.merge_paths(&primary_paths, &self.additional_theme_paths);
        state.last_theme_paths = theme_paths.clone();
        self.update_themes_from_paths(state, &theme_paths)?;
        self.push_missing_path_diagnostics(
            &mut state.theme_diagnostics,
            &self.additional_theme_paths,
            "Theme path does not exist",
            PathCheck::Any,
        );
        Ok(())
    }

    fn push_missing_path_diagnostics(
        &self,
        diagnostics: &mut Vec<ResourceDiagnostic>,
        paths: &[String],
        message: &str,
        check: PathCheck,
    ) {
        for path in paths {
            if check == PathCheck::Local && !is_local_path(path) {
                continue;
            }
            let resolved = self.resolve_resource_path(path);
            if Path::new(&resolved).exists() {
                continue;
            }
            let already_reported = diagnostics
                .iter()
                .any(|diagnostic| diagnostic.path.as_deref() == Some(resolved.as_str()));
            if already_reported {
                continue;
            }
            diagnostics.push(ResourceDiagnostic {
                diagnostic_type: ResourceDiagnosticType::Error,
                message: message.to_owned(),
                path: Some(resolved),
                collision: None,
            });
        }
    }

    fn load_agents_files_into(&self, state: &mut ResourceLoaderState) {
        let agents_files = LoadedAgentsFiles {
            agents_files: if self.no_context_files {
                Vec::new()
            } else {
                load_project_context_files(&self.cwd, &self.agent_dir)
            },
        };
        let resolved_agents_files = match &self.agents_files_override {
            Some(override_fn) => override_fn(agents_files),
            None => agents_files,
        };
        state.agents_files = resolved_agents_files.agents_files;
    }

    fn load_rules_into(&self, state: &mut ResourceLoaderState) -> Result<(), String> {
        let global_rules_dir = Path::new(&self.agent_dir).join("rules");
        let mut rules_files = load_rule_scope_files(&global_rules_dir, self.rules_scope)?;
        if self.settings().is_project_trusted() {
            let project_rules_dir = Path::new(&self.cwd)
                .join(&package_config().config_dir_name)
                .join("rules");
            rules_files.extend(load_rule_scope_files(&project_rules_dir, self.rules_scope)?);
        }
        state.rules_content = (!rules_files.is_empty()).then(|| {
            rules_files
                .iter()
                .map(|file| file.content.clone())
                .collect::<Vec<_>>()
                .join("\n\n")
        });
        state.rules_files = rules_files;
        Ok(())
    }

    fn load_system_prompts_into(&self, state: &mut ResourceLoaderState) {
        let system_prompt_source = self
            .system_prompt_source
            .clone()
            .or_else(|| self.discover_system_prompt_file());
        let base_system_prompt = resolve_prompt_input(system_prompt_source.as_deref(), "system prompt");
        state.system_prompt = match &self.system_prompt_override {
            Some(override_fn) => override_fn(base_system_prompt.as_deref()),
            None => base_system_prompt,
        };
        state.system_prompt_source_path = system_prompt_source
            .as_deref()
            .filter(|source| Path::new(source).exists())
            .map(resolve_from_process_cwd);

        let append_sources = match &self.append_system_prompt_source {
            Some(sources) => sources.clone(),
            None => self.discover_append_system_prompt_file().into_iter().collect(),
        };
        let base_append: Vec<String> = append_sources
            .iter()
            .filter_map(|source| resolve_prompt_input(Some(source), "append system prompt"))
            .collect();
        state.append_system_prompt = match &self.append_system_prompt_override {
            Some(override_fn) => override_fn(&base_append),
            None => base_append,
        };
        state.append_system_prompt_source_paths = append_sources
            .iter()
            .filter(|source| Path::new(source).exists())
            .map(|source| resolve_from_process_cwd(source.as_str()))
            .collect();
    }

    async fn load_current_extension_set(&self, include_inline_factories: bool) -> Result<LoadExtensionsResult, String> {
        let resolved_paths = self
            .package_manager
            .resolve(None)
            .await
            .map_err(|error| error.to_string())?;
        let cli_extension_paths = self
            .package_manager
            .resolve_extension_sources(&self.additional_extension_paths, false, true)
            .await
            .map_err(|error| error.to_string())?;
        let enabled_extensions = resolved_paths
            .extensions
            .iter()
            .filter(|resource| resource.enabled)
            .map(|resource| resource.path.clone())
            .collect::<Vec<_>>();
        let cli_enabled_extensions = cli_extension_paths
            .extensions
            .iter()
            .filter(|resource| resource.enabled)
            .map(|resource| resource.path.clone())
            .collect::<Vec<_>>();
        let extension_paths = self.filter_disabled_extension_paths(if self.no_extensions {
            cli_enabled_extensions
        } else {
            self.merge_paths(&cli_enabled_extensions, &enabled_extensions)
        });
        let mut extensions_result = self.load_extensions_cached(&extension_paths, None).await;
        if !include_inline_factories {
            return Ok(extensions_result);
        }

        let (inline_extensions, inline_errors) = self.load_extension_factories(extensions_result.runtime.clone()).await;
        extensions_result.extensions.extend(inline_extensions);
        extensions_result.errors.extend(inline_errors);
        Ok(extensions_result)
    }

    async fn load_extensions_cached(
        &self,
        paths: &[String],
        runtime: Option<crate::core::extensions::extension_runtime::ExtensionRuntime>,
    ) -> LoadExtensionsResult {
        self.extension_registry
            .load_extensions(paths, &self.cwd, Some(Arc::clone(&self.event_bus)), runtime)
            .await
    }

    async fn load_final_extension_set(
        &self,
        extension_paths: &[String],
        pre_trust_extensions: Option<LoadExtensionsResult>,
    ) -> LoadExtensionsResult {
        let Some(pre_trust) = pre_trust_extensions else {
            let mut extensions_result = self.load_extensions_cached(extension_paths, None).await;
            let (inline_extensions, inline_errors) =
                self.load_extension_factories(extensions_result.runtime.clone()).await;
            extensions_result.extensions.extend(inline_extensions);
            extensions_result.errors.extend(inline_errors);
            self.add_extension_conflict_diagnostics(&mut extensions_result);
            return extensions_result;
        };

        let mut preloaded_by_path: IndexMap<String, crate::core::extensions::types::Extension> = IndexMap::new();
        for extension in pre_trust
            .extensions
            .iter()
            .filter(|extension| !extension.path.starts_with('<'))
        {
            preloaded_by_path.insert(extension.resolved_path.clone(), extension.clone());
        }
        let failed_preload_paths: HashSet<String> = pre_trust
            .errors
            .iter()
            .map(|error| self.resolve_extension_load_path(&error.path))
            .collect();
        let remaining_paths: Vec<String> = extension_paths
            .iter()
            .filter(|path| {
                let resolved_path = self.resolve_extension_load_path(path);
                !preloaded_by_path.contains_key(&resolved_path) && !failed_preload_paths.contains(&resolved_path)
            })
            .cloned()
            .collect();
        let remaining_extensions = self
            .load_extensions_cached(&remaining_paths, Some(pre_trust.runtime.clone()))
            .await;
        let mut loaded_by_path = preloaded_by_path;
        for extension in &remaining_extensions.extensions {
            loaded_by_path.insert(extension.resolved_path.clone(), extension.clone());
        }

        let disabled = self.disabled_extension_names();
        let inline_extensions: Vec<crate::core::extensions::types::Extension> = pre_trust
            .extensions
            .iter()
            .filter(|extension| {
                extension.path.starts_with('<') && !self.is_extension_factory_disabled_with(&disabled, &extension.path)
            })
            .cloned()
            .collect();
        let mut ordered_extensions: Vec<crate::core::extensions::types::Extension> = extension_paths
            .iter()
            .filter_map(|path| loaded_by_path.get(&self.resolve_extension_load_path(path)).cloned())
            .collect();
        ordered_extensions.extend(inline_extensions);

        let mut errors = pre_trust.errors.clone();
        errors.extend(remaining_extensions.errors.clone());
        let mut extensions_result = LoadExtensionsResult {
            extensions: ordered_extensions,
            errors,
            runtime: pre_trust.runtime.clone(),
        };
        self.add_extension_conflict_diagnostics(&mut extensions_result);
        extensions_result
    }

    fn add_extension_conflict_diagnostics(&self, extensions_result: &mut LoadExtensionsResult) {
        let conflicts = detect_extension_conflicts(&extensions_result.extensions);
        for conflict in conflicts {
            extensions_result
                .errors
                .push(crate::core::extensions::types::ExtensionLoadError {
                    path: conflict.path,
                    error: conflict.message,
                });
        }
    }

    fn normalize_extension_paths(&self, entries: &[ExtendedResourcePath]) -> Vec<ExtendedResourcePath> {
        entries
            .iter()
            .map(|entry| {
                let metadata = match &entry.metadata.base_dir {
                    Some(base_dir) => PathMetadata {
                        base_dir: Some(self.resolve_resource_path(base_dir)),
                        ..entry.metadata.clone()
                    },
                    None => entry.metadata.clone(),
                };
                ExtendedResourcePath {
                    path: self.resolve_resource_path(&entry.path),
                    metadata,
                }
            })
            .collect()
    }

    fn update_skills_from_paths(&self, state: &mut ResourceLoaderState, skill_paths: &[String]) -> Result<(), String> {
        let mut skills_result = if self.no_skills && skill_paths.is_empty() {
            LoadedSkills::default()
        } else {
            let result = load_skills(&LoadSkillsOptions {
                cwd: self.cwd.clone(),
                agent_dir: self.agent_dir.clone(),
                skill_paths: skill_paths.to_vec(),
                include_defaults: false,
            })
            .map_err(|error| error.to_string())?;
            LoadedSkills {
                skills: result.skills,
                diagnostics: result.diagnostics,
            }
        };
        if let Some(override_fn) = &self.skills_override {
            skills_result = override_fn(skills_result);
        }
        let metadata_by_path = Some(&state.resource_metadata_by_path);
        let mut skills = Vec::new();
        for skill in skills_result.skills {
            let source_info = self
                .find_source_info_for_path(
                    &skill.file_path,
                    Some(&state.extension_skill_source_infos),
                    metadata_by_path,
                )?
                .unwrap_or(skill.source_info);
            skills.push(Skill { source_info, ..skill });
        }
        state.skills = skills;
        state.skill_diagnostics = skills_result.diagnostics;
        Ok(())
    }

    fn update_prompts_from_paths(
        &self,
        state: &mut ResourceLoaderState,
        prompt_paths: &[String],
    ) -> Result<(), String> {
        let mut prompts_result = if self.no_prompt_templates && prompt_paths.is_empty() {
            LoadedPrompts::default()
        } else {
            let all_prompts = load_prompt_templates(&LoadPromptTemplatesOptions {
                cwd: self.cwd.clone(),
                agent_dir: self.agent_dir.clone(),
                prompt_paths: prompt_paths.to_vec(),
                include_defaults: false,
            })
            .map_err(|error| error.to_string())?;
            dedupe_prompts(all_prompts)
        };
        if let Some(override_fn) = &self.prompts_override {
            prompts_result = override_fn(prompts_result);
        }
        let metadata_by_path = Some(&state.resource_metadata_by_path);
        let mut prompts = Vec::new();
        for prompt in prompts_result.prompts {
            let source_info = self
                .find_source_info_for_path(
                    &prompt.file_path,
                    Some(&state.extension_prompt_source_infos),
                    metadata_by_path,
                )?
                .unwrap_or(prompt.source_info);
            prompts.push(PromptTemplate { source_info, ..prompt });
        }
        state.prompts = prompts;
        state.prompt_diagnostics = prompts_result.diagnostics;
        Ok(())
    }

    fn update_themes_from_paths(&self, state: &mut ResourceLoaderState, theme_paths: &[String]) -> Result<(), String> {
        let mut themes_result = if self.no_themes && theme_paths.is_empty() {
            LoadedThemes::default()
        } else {
            let (themes, diagnostics) = self.load_themes(theme_paths, false)?;
            let deduped = dedupe_themes(themes);
            LoadedThemes {
                themes: deduped.themes,
                diagnostics: diagnostics.into_iter().chain(deduped.diagnostics).collect(),
            }
        };
        if let Some(override_fn) = &self.themes_override {
            themes_result = override_fn(themes_result);
        }
        let metadata_by_path = Some(&state.resource_metadata_by_path);
        let mut themes = Vec::new();
        for mut theme in themes_result.themes {
            if let Some(source_path) = theme.source_path.clone() {
                let source_path = source_path.to_string_lossy().into_owned();
                if let Some(source_info) = self.find_source_info_for_path(
                    &source_path,
                    Some(&state.extension_theme_source_infos),
                    metadata_by_path,
                )? {
                    theme.source_info = Some(source_info);
                }
            }
            themes.push(theme);
        }
        state.themes = themes;
        state.theme_diagnostics = themes_result.diagnostics;
        Ok(())
    }

    fn apply_extension_source_info(
        &self,
        extensions: &mut [crate::core::extensions::types::Extension],
        metadata_by_path: &IndexMap<String, PathMetadata>,
    ) -> Result<(), String> {
        for extension in extensions {
            let source_info = match self.find_source_info_for_path(&extension.path, None, Some(metadata_by_path))? {
                Some(source_info) => source_info,
                None => self.get_default_source_info_for_path(&extension.path)?,
            };
            extension.source_info = source_info.clone();
            for command in extension.commands.values_mut() {
                command.source_info = source_info.clone();
            }
            for tool in extension.tools.values_mut() {
                tool.source_info = source_info.clone();
            }
        }
        Ok(())
    }

    fn find_source_info_for_path(
        &self,
        resource_path: &str,
        extra_source_infos: Option<&IndexMap<String, SourceInfo>>,
        metadata_by_path: Option<&IndexMap<String, PathMetadata>>,
    ) -> Result<Option<SourceInfo>, String> {
        if resource_path.is_empty() {
            return Ok(None);
        }
        if resource_path.starts_with('<') {
            return self.get_default_source_info_for_path(resource_path).map(Some);
        }

        let normalized_resource_path = resolve_from_process_cwd(resource_path);
        if let Some(extra_source_infos) = extra_source_infos {
            for (source_path, source_info) in extra_source_infos {
                let normalized_source_path = resolve_from_process_cwd(source_path);
                if normalized_resource_path == normalized_source_path
                    || normalized_resource_path
                        .starts_with(&format!("{normalized_source_path}{}", std::path::MAIN_SEPARATOR))
                {
                    return Ok(Some(SourceInfo {
                        path: resource_path.to_owned(),
                        ..source_info.clone()
                    }));
                }
            }
        }

        if let Some(metadata_by_path) = metadata_by_path {
            if let Some(metadata) = metadata_by_path
                .get(&normalized_resource_path)
                .or_else(|| metadata_by_path.get(resource_path))
            {
                return Ok(Some(create_source_info(resource_path, metadata)));
            }
            for (source_path, metadata) in metadata_by_path {
                let normalized_source_path = resolve_from_process_cwd(source_path);
                if normalized_resource_path == normalized_source_path
                    || normalized_resource_path
                        .starts_with(&format!("{normalized_source_path}{}", std::path::MAIN_SEPARATOR))
                {
                    return Ok(Some(create_source_info(resource_path, metadata)));
                }
            }
        }

        Ok(None)
    }

    fn get_default_source_info_for_path(&self, file_path: &str) -> Result<SourceInfo, String> {
        if file_path.starts_with('<') && file_path.ends_with('>') {
            let inner = &file_path[1..file_path.len() - 1];
            let source = inner.split(':').next().unwrap_or_default();
            return Ok(create_synthetic_source_info(
                file_path,
                &crate::core::source_info::SyntheticSourceInfoOptions {
                    source: if source.is_empty() {
                        "temporary".to_owned()
                    } else {
                        source.to_owned()
                    },
                    scope: Some(SourceScope::Temporary),
                    origin: Some(SourceOrigin::TopLevel),
                    base_dir: None,
                },
            ));
        }

        let normalized_path = resolve_from_process_cwd(file_path);
        let agent_roots = [
            Path::new(&self.agent_dir).join("skills"),
            Path::new(&self.agent_dir).join("prompts"),
            Path::new(&self.agent_dir).join("themes"),
            Path::new(&self.agent_dir).join("extensions"),
        ];
        let project_roots = [
            Path::new(&self.cwd)
                .join(&package_config().config_dir_name)
                .join("skills"),
            Path::new(&self.cwd)
                .join(&package_config().config_dir_name)
                .join("prompts"),
            Path::new(&self.cwd)
                .join(&package_config().config_dir_name)
                .join("themes"),
            Path::new(&self.cwd)
                .join(&package_config().config_dir_name)
                .join("extensions"),
        ];

        for root in agent_roots {
            if is_under_path(&normalized_path, &root) {
                return Ok(create_synthetic_source_info(
                    file_path,
                    &crate::core::source_info::SyntheticSourceInfoOptions {
                        source: "local".to_owned(),
                        scope: Some(SourceScope::User),
                        origin: Some(SourceOrigin::TopLevel),
                        base_dir: Some(root.to_string_lossy().into_owned()),
                    },
                ));
            }
        }

        for root in project_roots {
            if is_under_path(&normalized_path, &root) {
                return Ok(create_synthetic_source_info(
                    file_path,
                    &crate::core::source_info::SyntheticSourceInfoOptions {
                        source: "local".to_owned(),
                        scope: Some(SourceScope::Project),
                        origin: Some(SourceOrigin::TopLevel),
                        base_dir: Some(root.to_string_lossy().into_owned()),
                    },
                ));
            }
        }

        let metadata = fs::metadata(&normalized_path).map_err(|error| error.to_string())?;
        let base_dir = if metadata.is_dir() {
            normalized_path.clone()
        } else {
            Path::new(&normalized_path)
                .parent()
                .map_or(normalized_path.clone(), |parent| parent.to_string_lossy().into_owned())
        };
        Ok(create_synthetic_source_info(
            file_path,
            &crate::core::source_info::SyntheticSourceInfoOptions {
                source: "local".to_owned(),
                scope: Some(SourceScope::Temporary),
                origin: Some(SourceOrigin::TopLevel),
                base_dir: Some(base_dir),
            },
        ))
    }

    fn load_themes(
        &self,
        paths: &[String],
        include_defaults: bool,
    ) -> Result<(Vec<Theme>, Vec<ResourceDiagnostic>), String> {
        let mut themes = Vec::new();
        let mut diagnostics = Vec::new();
        if include_defaults {
            let default_dirs = [
                Path::new(&self.agent_dir).join("themes"),
                Path::new(&self.cwd)
                    .join(&package_config().config_dir_name)
                    .join("themes"),
            ];
            for dir in default_dirs {
                self.load_themes_from_dir(&dir.to_string_lossy(), &mut themes, &mut diagnostics);
            }
        }

        for path in paths {
            let resolved = self.resolve_resource_path(path);
            if !Path::new(&resolved).exists() {
                diagnostics.push(ResourceDiagnostic {
                    diagnostic_type: ResourceDiagnosticType::Warning,
                    message: "theme path does not exist".to_owned(),
                    path: Some(resolved),
                    collision: None,
                });
                continue;
            }

            let Ok(metadata) = fs::metadata(&resolved) else {
                diagnostics.push(ResourceDiagnostic {
                    diagnostic_type: ResourceDiagnosticType::Warning,
                    message: "failed to read theme path".to_owned(),
                    path: Some(resolved),
                    collision: None,
                });
                continue;
            };
            if metadata.is_dir() {
                self.load_themes_from_dir(&resolved, &mut themes, &mut diagnostics);
            } else if metadata.is_file() && resolved.ends_with(".json") {
                self.load_theme_from_file(&resolved, &mut themes, &mut diagnostics);
            } else {
                diagnostics.push(ResourceDiagnostic {
                    diagnostic_type: ResourceDiagnosticType::Warning,
                    message: "theme path is not a json file".to_owned(),
                    path: Some(resolved),
                    collision: None,
                });
            }
        }

        Ok((themes, diagnostics))
    }

    fn load_themes_from_dir(&self, dir: &str, themes: &mut Vec<Theme>, diagnostics: &mut Vec<ResourceDiagnostic>) {
        if !Path::new(dir).exists() {
            return;
        }
        let Ok(entries) = fs::read_dir(dir) else {
            diagnostics.push(ResourceDiagnostic {
                diagnostic_type: ResourceDiagnosticType::Warning,
                message: "failed to read theme directory".to_owned(),
                path: Some(dir.to_owned()),
                collision: None,
            });
            return;
        };
        for entry in entries.flatten() {
            let entry_path = Path::new(dir).join(entry.file_name());
            let Ok(metadata) = fs::metadata(&entry_path) else {
                continue;
            };
            if !metadata.is_file() {
                continue;
            }
            if !entry.file_name().to_string_lossy().ends_with(".json") {
                continue;
            }
            self.load_theme_from_file(&entry_path.to_string_lossy(), themes, diagnostics);
        }
    }

    fn load_theme_from_file(
        &self,
        file_path: &str,
        themes: &mut Vec<Theme>,
        diagnostics: &mut Vec<ResourceDiagnostic>,
    ) {
        match load_theme_from_path(Path::new(file_path), None) {
            Ok(theme) => themes.push(theme),
            Err(error) => diagnostics.push(ResourceDiagnostic {
                diagnostic_type: ResourceDiagnosticType::Warning,
                message: error.to_string(),
                path: Some(file_path.to_owned()),
                collision: None,
            }),
        }
    }

    async fn load_extension_factories(
        &self,
        runtime: crate::core::extensions::extension_runtime::ExtensionRuntime,
    ) -> (
        Vec<crate::core::extensions::types::Extension>,
        Vec<crate::core::extensions::types::ExtensionLoadError>,
    ) {
        let mut extensions = Vec::new();
        let mut errors = Vec::new();
        let disabled = self.disabled_extension_names();

        for (index, input) in self.extension_factories.iter().enumerate() {
            let extension_path = input.extension_path(index);
            if self.is_extension_factory_disabled_with(&disabled, &extension_path) {
                continue;
            }
            match load_extension_from_factory(
                input.factory(),
                &self.cwd,
                Arc::clone(&self.event_bus),
                runtime.clone(),
                &extension_path,
            )
            .await
            {
                Ok(mut extension) => {
                    extension.hidden = Some(input.is_named() && input.hidden());
                    extensions.push(extension);
                }
                Err(error) => errors.push(crate::core::extensions::types::ExtensionLoadError {
                    path: extension_path,
                    error: error.message,
                }),
            }
        }

        (extensions, errors)
    }

    fn discover_system_prompt_file(&self) -> Option<String> {
        let project_path = Path::new(&self.cwd)
            .join(&package_config().config_dir_name)
            .join("SYSTEM.md");
        if self.settings().is_project_trusted() && project_path.exists() {
            return Some(project_path.to_string_lossy().into_owned());
        }
        let global_path = Path::new(&self.agent_dir).join("SYSTEM.md");
        if global_path.exists() {
            return Some(global_path.to_string_lossy().into_owned());
        }
        None
    }

    fn discover_append_system_prompt_file(&self) -> Option<String> {
        let project_path = Path::new(&self.cwd)
            .join(&package_config().config_dir_name)
            .join("APPEND_SYSTEM.md");
        if self.settings().is_project_trusted() && project_path.exists() {
            return Some(project_path.to_string_lossy().into_owned());
        }
        let global_path = Path::new(&self.agent_dir).join("APPEND_SYSTEM.md");
        if global_path.exists() {
            return Some(global_path.to_string_lossy().into_owned());
        }
        None
    }
}

impl ResourceLoader for DefaultResourceLoader {
    fn get_extensions(&self) -> LoadExtensionsResult {
        self.state().extensions_result.clone()
    }

    fn get_skills(&self) -> LoadedSkills {
        let state = self.state();
        LoadedSkills {
            skills: state.skills.clone(),
            diagnostics: state.skill_diagnostics.clone(),
        }
    }

    fn get_prompts(&self) -> LoadedPrompts {
        let state = self.state();
        LoadedPrompts {
            prompts: state.prompts.clone(),
            diagnostics: state.prompt_diagnostics.clone(),
        }
    }

    fn get_themes(&self) -> LoadedThemes {
        let state = self.state();
        LoadedThemes {
            themes: state.themes.clone(),
            diagnostics: state.theme_diagnostics.clone(),
        }
    }

    fn get_agents_files(&self) -> LoadedAgentsFiles {
        LoadedAgentsFiles {
            agents_files: self.state().agents_files.clone(),
        }
    }

    fn get_rules_files(&self) -> LoadedRulesFiles {
        LoadedRulesFiles {
            rules_files: self.state().rules_files.clone(),
        }
    }

    fn get_rules_content(&self) -> Option<String> {
        self.state().rules_content.clone()
    }

    fn get_system_prompt(&self) -> Option<String> {
        self.state().system_prompt.clone()
    }

    fn get_system_prompt_source(&self) -> Option<LoadedPathSource> {
        self.state()
            .system_prompt_source_path
            .clone()
            .map(|path| LoadedPathSource { path })
    }

    fn get_append_system_prompt(&self) -> Vec<String> {
        self.state().append_system_prompt.clone()
    }

    fn get_append_system_prompt_sources(&self) -> Vec<LoadedPathSource> {
        self.state()
            .append_system_prompt_source_paths
            .iter()
            .map(|path| LoadedPathSource { path: path.clone() })
            .collect()
    }

    fn extend_resources(&self, paths: ResourceExtensionPaths) {
        let skill_paths = self.normalize_extension_paths(&paths.skill_paths);
        let prompt_paths = self.normalize_extension_paths(&paths.prompt_paths);
        let theme_paths = self.normalize_extension_paths(&paths.theme_paths);
        let mut state = self.state();

        for entry in &skill_paths {
            state
                .extension_skill_source_infos
                .insert(entry.path.clone(), create_source_info(&entry.path, &entry.metadata));
        }
        for entry in &prompt_paths {
            state
                .extension_prompt_source_infos
                .insert(entry.path.clone(), create_source_info(&entry.path, &entry.metadata));
        }
        for entry in &theme_paths {
            state
                .extension_theme_source_infos
                .insert(entry.path.clone(), create_source_info(&entry.path, &entry.metadata));
        }

        if !skill_paths.is_empty() {
            let added: Vec<String> = skill_paths.iter().map(|entry| entry.path.clone()).collect();
            state.last_skill_paths = self.merge_paths(&state.last_skill_paths, &added);
            let last_skill_paths = state.last_skill_paths.clone();
            let _ = self.update_skills_from_paths(&mut state, &last_skill_paths);
        }

        if !prompt_paths.is_empty() {
            let added: Vec<String> = prompt_paths.iter().map(|entry| entry.path.clone()).collect();
            state.last_prompt_paths = self.merge_paths(&state.last_prompt_paths, &added);
            let last_prompt_paths = state.last_prompt_paths.clone();
            let _ = self.update_prompts_from_paths(&mut state, &last_prompt_paths);
        }

        if !theme_paths.is_empty() {
            let added: Vec<String> = theme_paths.iter().map(|entry| entry.path.clone()).collect();
            state.last_theme_paths = self.merge_paths(&state.last_theme_paths, &added);
            let last_theme_paths = state.last_theme_paths.clone();
            let _ = self.update_themes_from_paths(&mut state, &last_theme_paths);
        }
    }

    fn reload(&self) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async move { self.reload_with(None).await })
    }
}

struct ResolvedResourceSets {
    enabled_extensions: Vec<String>,
    enabled_skills: Vec<String>,
    enabled_prompts: Vec<String>,
    enabled_themes: Vec<String>,
    cli_extensions: Vec<String>,
    cli_skills: Vec<String>,
    cli_prompts: Vec<String>,
    cli_themes: Vec<String>,
    metadata_by_path: IndexMap<String, PathMetadata>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PathCheck {
    Local,
    Any,
}

fn load_rule_scope_files(shared_dir: &Path, scope: RulesScope) -> Result<Vec<RuleFile>, String> {
    let mut files = load_rules_files_from_dir(&shared_dir.to_string_lossy()).map_err(|error| error.to_string())?;
    if scope == RulesScope::Shared {
        return Ok(files);
    }
    files.extend(
        load_rules_files_from_dir(&shared_dir.join(scope.as_str()).to_string_lossy())
            .map_err(|error| error.to_string())?,
    );
    Ok(files)
}

fn enabled_resources(
    resources: &[ResolvedResource],
    metadata_by_path: &mut IndexMap<String, PathMetadata>,
) -> Vec<ResolvedResource> {
    for resource in resources {
        metadata_by_path
            .entry(resource.path.clone())
            .or_insert_with(|| resource.metadata.clone());
    }
    resources.iter().filter(|resource| resource.enabled).cloned().collect()
}

fn enabled_paths(resources: &[ResolvedResource], metadata_by_path: &mut IndexMap<String, PathMetadata>) -> Vec<String> {
    enabled_resources(resources, metadata_by_path)
        .into_iter()
        .map(|resource| resource.path)
        .collect()
}

fn map_skill_path(resource: &ResolvedResource, metadata_by_path: &mut IndexMap<String, PathMetadata>) -> String {
    if resource.metadata.source != "auto" && resource.metadata.origin != MetadataOrigin::Package {
        return resource.path.clone();
    }
    let Ok(metadata) = fs::metadata(&resource.path) else {
        return resource.path.clone();
    };
    if !metadata.is_dir() {
        return resource.path.clone();
    }
    let skill_file = Path::new(&resource.path).join("SKILL.md");
    if skill_file.exists() {
        metadata_by_path
            .entry(skill_file.to_string_lossy().into_owned())
            .or_insert_with(|| resource.metadata.clone());
        return skill_file.to_string_lossy().into_owned();
    }
    resource.path.clone()
}

fn dedupe_prompts(prompts: Vec<PromptTemplate>) -> LoadedPrompts {
    let mut seen: IndexMap<String, PromptTemplate> = IndexMap::new();
    let mut diagnostics = Vec::new();
    for prompt in prompts {
        if let Some(existing) = seen.get(&prompt.name) {
            diagnostics.push(ResourceDiagnostic {
                diagnostic_type: ResourceDiagnosticType::Collision,
                message: format!("name \"/{}\" collision", prompt.name),
                path: Some(prompt.file_path.clone()),
                collision: Some(ResourceCollision {
                    resource_type: ResourceType::Prompt,
                    name: prompt.name.clone(),
                    winner_path: existing.file_path.clone(),
                    loser_path: prompt.file_path.clone(),
                    winner_source: None,
                    loser_source: None,
                }),
            });
        } else {
            seen.insert(prompt.name.clone(), prompt);
        }
    }
    LoadedPrompts {
        prompts: seen.into_values().collect(),
        diagnostics,
    }
}

fn dedupe_themes(themes: Vec<Theme>) -> LoadedThemes {
    let mut seen: IndexMap<String, Theme> = IndexMap::new();
    let mut diagnostics = Vec::new();
    for theme in themes {
        let name = theme.name.clone().unwrap_or_else(|| "unnamed".to_owned());
        if let Some(existing) = seen.get(&name) {
            let winner_path = existing
                .source_path
                .as_ref()
                .map_or_else(|| "<builtin>".to_owned(), |path| path.to_string_lossy().into_owned());
            let loser_path = theme
                .source_path
                .as_ref()
                .map_or_else(|| "<builtin>".to_owned(), |path| path.to_string_lossy().into_owned());
            diagnostics.push(ResourceDiagnostic {
                diagnostic_type: ResourceDiagnosticType::Collision,
                message: format!("name \"{name}\" collision"),
                path: theme
                    .source_path
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned()),
                collision: Some(ResourceCollision {
                    resource_type: ResourceType::Theme,
                    name,
                    winner_path,
                    loser_path,
                    winner_source: None,
                    loser_source: None,
                }),
            });
        } else {
            seen.insert(name, theme);
        }
    }
    LoadedThemes {
        themes: seen.into_values().collect(),
        diagnostics,
    }
}

fn is_under_path(target: &str, root: &Path) -> bool {
    let normalized_root = root.to_string_lossy();
    target == normalized_root || target.starts_with(&format!("{normalized_root}{}", std::path::MAIN_SEPARATOR))
}

struct ExtensionConflict {
    path: String,
    message: String,
}

fn detect_extension_conflicts(extensions: &[crate::core::extensions::types::Extension]) -> Vec<ExtensionConflict> {
    let mut conflicts = Vec::new();
    let mut tool_owners: IndexMap<String, String> = IndexMap::new();
    let mut flag_owners: IndexMap<String, String> = IndexMap::new();

    for extension in extensions {
        for tool_name in extension.tools.keys() {
            match tool_owners.get(tool_name) {
                Some(existing_owner) if *existing_owner != extension.path => {
                    conflicts.push(ExtensionConflict {
                        path: extension.path.clone(),
                        message: format!("Tool \"{tool_name}\" conflicts with {existing_owner}"),
                    });
                }
                Some(_) => {}
                None => {
                    tool_owners.insert(tool_name.clone(), extension.path.clone());
                }
            }
        }

        for flag_name in extension.flags.keys() {
            match flag_owners.get(flag_name) {
                Some(existing_owner) if *existing_owner != extension.path => {
                    conflicts.push(ExtensionConflict {
                        path: extension.path.clone(),
                        message: format!("Flag \"--{flag_name}\" conflicts with {existing_owner}"),
                    });
                }
                Some(_) => {}
                None => {
                    flag_owners.insert(flag_name.clone(), extension.path.clone());
                }
            }
        }
    }

    conflicts
}
