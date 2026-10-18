//! System prompt construction (`core/system-prompt.ts`).

use std::collections::{HashMap, HashSet};

use crate::config::{get_docs_path, get_examples_path, get_readme_path};
use crate::core::skills::{Skill, format_skills_for_prompt};
use crate::core::tool_detach_registry::DEFAULT_AUTO_DETACH_AFTER_MS;
use crate::core::tools::DEFAULT_ACTIVE_TOOL_NAMES;

const MILLISECONDS_PER_MINUTE: u64 = 60_000;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ContextFile {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BuildSystemPromptOptions {
    /// Custom system prompt (replaces default).
    pub custom_prompt: Option<String>,
    /// Tools to include in prompt. Default: `DEFAULT_ACTIVE_TOOL_NAMES`.
    pub selected_tools: Option<Vec<String>>,
    /// Optional one-line tool snippets keyed by tool name.
    pub tool_snippets: Option<HashMap<String, String>>,
    /// Additional guideline bullets appended to the default system prompt guidelines.
    pub prompt_guidelines: Option<Vec<String>>,
    /// Text to append to system prompt.
    pub append_system_prompt: Option<String>,
    /// Working directory.
    pub cwd: String,
    /// Pre-loaded context files.
    pub context_files: Option<Vec<ContextFile>>,
    /// Pre-loaded user rules.
    pub rules_content: Option<String>,
    /// Pre-loaded skills.
    pub skills: Option<Vec<Skill>>,
}

fn append_user_rules_section(prompt: &str, rules_content: Option<&str>) -> String {
    let Some(rules_content) = rules_content.filter(|content| !content.is_empty()) else {
        return prompt.to_owned();
    };

    let separator = if prompt.ends_with('\n') { "\n" } else { "\n\n" };
    format!("{prompt}{separator}<user_rules>\n{rules_content}\n</user_rules>")
}

fn append_context_files(prompt: &mut String, context_files: &[ContextFile]) {
    if context_files.is_empty() {
        return;
    }
    prompt.push_str("\n\n<project_context>\n\n");
    prompt.push_str("Project-specific instructions and guidelines:\n\n");
    for file in context_files {
        prompt.push_str(&format!(
            "<project_instructions path=\"{}\">\n{}\n</project_instructions>\n\n",
            file.path, file.content
        ));
    }
    prompt.push_str("</project_context>\n");
}

/// Builds the system prompt with tools, guidelines, and context.
pub fn build_system_prompt(options: &BuildSystemPromptOptions) -> String {
    let prompt_cwd = options.cwd.replace('\\', "/");
    let append_section = options
        .append_system_prompt
        .as_ref()
        .map_or_else(String::new, |text| format!("\n\n{text}"));

    let context_files = options.context_files.as_deref().unwrap_or(&[]);
    let skills = options.skills.as_deref().unwrap_or(&[]);

    if let Some(custom_prompt) = options.custom_prompt.as_ref().filter(|prompt| !prompt.is_empty()) {
        let mut prompt = custom_prompt.clone();
        prompt.push_str(&append_section);
        append_context_files(&mut prompt, context_files);
        prompt = append_user_rules_section(&prompt, options.rules_content.as_deref());

        let custom_prompt_has_read = options
            .selected_tools
            .as_ref()
            .is_none_or(|tools| tools.iter().any(|tool| tool == "read"));
        if custom_prompt_has_read && !skills.is_empty() {
            prompt.push_str(&format_skills_for_prompt(skills));
        }

        prompt.push_str(&format!("\nCurrent working directory: {prompt_cwd}\n"));
        return prompt;
    }

    let readme_path = get_readme_path().to_string_lossy().into_owned();
    let docs_path = get_docs_path().to_string_lossy().into_owned();
    let examples_path = get_examples_path().to_string_lossy().into_owned();

    let tools: Vec<&str> = match &options.selected_tools {
        Some(tools) => tools.iter().map(String::as_str).collect(),
        None => DEFAULT_ACTIVE_TOOL_NAMES.to_vec(),
    };
    let tool_snippets = options.tool_snippets.as_ref();
    let visible_tools: Vec<&str> = tools
        .iter()
        .copied()
        .filter(|name| tool_snippets.is_some_and(|snippets| snippets.contains_key(*name)))
        .collect();
    let tools_list = if visible_tools.is_empty() {
        "(none)".to_owned()
    } else {
        visible_tools
            .iter()
            .map(|name| {
                let snippet = tool_snippets
                    .and_then(|snippets| snippets.get(*name))
                    .map_or("", String::as_str);
                format!("- {name}: {snippet}")
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    let mut guidelines_list: Vec<String> = Vec::new();
    let mut guidelines_set: HashSet<String> = HashSet::new();
    let mut add_guideline = |guideline: &str| {
        if guidelines_set.insert(guideline.to_owned()) {
            guidelines_list.push(guideline.to_owned());
        }
    };

    let has_bash = tools.contains(&"bash");
    let has_backgroundable_command_tool = has_bash || tools.contains(&"pyrun_eval");
    let has_grep = tools.contains(&"grep");
    let has_find = tools.contains(&"find");
    let has_ls = tools.contains(&"ls");
    let has_read = tools.contains(&"read");
    let has_spawn_agent = tools.contains(&"spawn_agent");

    if has_spawn_agent {
        add_guideline(
            "For codebase research, exploration, or file-reading investigation, you must use spawn_agent with agentType \"explore\" before direct file search, unless the user explicitly asks you not to delegate.",
        );
        add_guideline(
            "For scoped code changes, use spawn_agent with agentType \"implement\" unless the change is trivial or the user explicitly asks you not to delegate.",
        );
        add_guideline(
            "Before claiming work is fixed, tested, passing, or complete, you must use spawn_agent with agentType \"verifier\" to run proof commands, unless the user explicitly asks you not to delegate.",
        );
        add_guideline(
            "For code changes that affect user-facing behavior, public APIs, CLI output, configuration, specs, or changelog-worthy behavior, use spawn_agent with agentType \"documentation-update\" to audit and update relevant docs, specs, or changelogs before final completion.",
        );
        add_guideline(
            "Documentation updates do not replace verification; still use a verifier before claiming work is fixed, tested, passing, or complete.",
        );
        add_guideline(
            "For final code review or second opinions on non-trivial changes, use spawn_agent with agentType \"reviewer\".",
        );
    }

    if has_backgroundable_command_tool && tools.contains(&"wait_agent") {
        let auto_background_after_minutes = DEFAULT_AUTO_DETACH_AFTER_MS / MILLISECONDS_PER_MINUTE;
        add_guideline(&format!(
            "Supported long-running command tools such as bash and Pyrun are automatically backgrounded after {auto_background_after_minutes} minutes; use wait_agent to wait for any agent completion, then inspect reported background job details or attached log files instead of assuming the command stopped."
        ));
    }

    if has_bash && !has_grep && !has_find && !has_ls {
        add_guideline("Use bash for file operations like ls, rg, find");
    }

    if tools.len() > 1 {
        add_guideline(
            "Before making tool calls, identify all calls whose inputs are already known and independent. Emit those calls together in the same assistant response; do not serialize independent exploration calls. Only wait when a later call requires an earlier result.",
        );
    }

    for guideline in options.prompt_guidelines.as_deref().unwrap_or(&[]) {
        let normalized = guideline.trim();
        if !normalized.is_empty() {
            add_guideline(normalized);
        }
    }

    add_guideline("Be concise in your responses");
    add_guideline("Show file paths clearly when working with files");

    let guidelines = guidelines_list
        .iter()
        .map(|guideline| format!("- {guideline}"))
        .collect::<Vec<_>>()
        .join("\n");

    let mut prompt = format!(
        "You are an expert coding assistant operating inside pi, a coding agent harness. You help users by reading files, executing commands, editing code, and writing new files.

Available tools:
{tools_list}

In addition to the tools above, you may have access to other custom tools depending on the project.

Guidelines:
{guidelines}

Pi documentation (read only when the user asks about pi itself, its SDK, extensions, themes, skills, or TUI):
- Main documentation: {readme_path}
- Additional docs: {docs_path}
- Examples: {examples_path} (extensions, custom tools, SDK)
- When reading pi docs or examples, resolve docs/... under Additional docs and examples/... under Examples, not the current working directory
- When asked about: extensions (docs/extensions.md, examples/extensions/), themes (docs/themes.md), skills (docs/skills.md), prompt templates (docs/prompt-templates.md), TUI components (docs/tui.md), keybindings (docs/keybindings.md), SDK integrations (docs/sdk.md), custom providers (docs/custom-provider.md), adding models (docs/models.md), pi packages (docs/packages.md), environment variables (docs/environment-variables.md)
- When working on pi topics, read the docs and examples, and follow .md cross-references before implementing
- Always read pi .md files completely and follow links to related docs (e.g., tui.md for TUI API details)"
    );

    prompt.push_str(&append_section);
    append_context_files(&mut prompt, context_files);
    prompt = append_user_rules_section(&prompt, options.rules_content.as_deref());

    if has_read && !skills.is_empty() {
        prompt.push_str(&format_skills_for_prompt(skills));
    }

    prompt.push_str(&format!("\nCurrent working directory: {prompt_cwd}"));
    prompt
}
