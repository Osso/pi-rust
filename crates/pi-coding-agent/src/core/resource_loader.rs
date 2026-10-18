//! Resource discovery helpers (`core/resource-loader.ts`).
//!
//! Only the parts that do not depend on the extension, package, theme, or footer-data
//! providers are ported here; `DefaultResourceLoader` and context-file discovery wait for
//! those modules.

use std::fs;
use std::path::Path;

use serde::Serialize;

use pi_js_compat::locale_compare;

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
    let input = input?;
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

// =============================================================================
// ResourceLoader contract used by AgentSession / sdk
// =============================================================================

use std::sync::Arc;

use pi_ai::request::BoxFuture;

use crate::core::diagnostics::ResourceDiagnostic;

use crate::core::package_manager::PathMetadata;
use crate::core::prompt_templates::PromptTemplate;
use crate::core::skills::Skill;
use crate::core::system_prompt::ContextFile;

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

/// Loaded context files (`AGENTS.md`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LoadedAgentsFiles {
    pub agents_files: Vec<ContextFile>,
}

/// `ResourceLoader` interface consumed by AgentSession.
///
/// `getExtensions` and `extendResources` are absent until `ca-extensions-api`
/// merges; see `docs/porting/traceability/agent-session.md`.
pub trait ResourceLoader: Send + Sync {
    /// Extension load result (`{ extensions, errors, runtime }`).
    fn get_extensions(&self) -> crate::core::extensions::types::LoadExtensionsResult;
    fn get_skills(&self) -> LoadedSkills;
    fn get_prompts(&self) -> LoadedPrompts;
    fn get_agents_files(&self) -> LoadedAgentsFiles;
    fn get_rules_content(&self) -> Option<String>;
    fn get_system_prompt(&self) -> Option<String>;
    fn get_append_system_prompt(&self) -> Vec<String>;
    fn extend_resources(&self, paths: ResourceExtensionPaths);
    fn reload(&self) -> BoxFuture<'static, ()>;
}

/// Empty loader used by tests and callers without resource discovery.
#[derive(Default)]
pub struct StaticResourceLoader {
    pub extensions: Option<crate::core::extensions::types::LoadExtensionsResult>,
    pub skills: LoadedSkills,
    pub prompts: LoadedPrompts,
    pub agents_files: LoadedAgentsFiles,
    pub rules_content: Option<String>,
    pub system_prompt: Option<String>,
    pub append_system_prompt: Vec<String>,
}

impl StaticResourceLoader {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
}

impl ResourceLoader for StaticResourceLoader {
    fn get_extensions(&self) -> crate::core::extensions::types::LoadExtensionsResult {
        self.extensions.clone().unwrap_or_else(|| {
            crate::core::extensions::types::LoadExtensionsResult {
                extensions: Vec::new(),
                errors: Vec::new(),
                runtime: crate::core::extensions::extension_runtime::create_extension_runtime(),
            }
        })
    }

    fn get_skills(&self) -> LoadedSkills {
        self.skills.clone()
    }

    fn get_prompts(&self) -> LoadedPrompts {
        self.prompts.clone()
    }

    fn get_agents_files(&self) -> LoadedAgentsFiles {
        self.agents_files.clone()
    }

    fn get_rules_content(&self) -> Option<String> {
        self.rules_content.clone()
    }

    fn get_system_prompt(&self) -> Option<String> {
        self.system_prompt.clone()
    }

    fn get_append_system_prompt(&self) -> Vec<String> {
        self.append_system_prompt.clone()
    }

    fn extend_resources(&self, _paths: ResourceExtensionPaths) {}

    fn reload(&self) -> BoxFuture<'static, ()> {
        Box::pin(async {})
    }
}
