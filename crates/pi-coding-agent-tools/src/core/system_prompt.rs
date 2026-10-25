//! System prompt construction (`core/system-prompt.ts`).

use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use std::collections::{HashMap, HashSet};

use crate::config::{get_docs_path, get_examples_path, get_readme_path};
use crate::core::skills::{Skill, format_skills_for_prompt};
use crate::core::tool_detach_registry::DEFAULT_AUTO_DETACH_AFTER_MS;
use crate::core::tools::DEFAULT_ACTIVE_TOOL_NAMES;

const MILLISECONDS_PER_MINUTE: u64 = 60_000;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ContextFile {
    pub path: JsString,
    pub content: JsString,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BuildSystemPromptOptions {
    /// Custom system prompt (replaces default).
    pub custom_prompt: Option<JsString>,
    /// Tools to include in prompt. Default: `DEFAULT_ACTIVE_TOOL_NAMES`.
    pub selected_tools: Option<Vec<String>>,
    /// Optional one-line tool snippets keyed by tool name.
    pub tool_snippets: Option<HashMap<String, JsString>>,
    /// Additional guideline bullets appended to the default system prompt guidelines.
    pub prompt_guidelines: Option<Vec<JsString>>,
    /// Text to append to system prompt.
    pub append_system_prompt: Option<JsString>,
    /// Working directory.
    pub cwd: JsString,
    /// Pre-loaded context files.
    pub context_files: Option<Vec<ContextFile>>,
    /// Pre-loaded user rules.
    pub rules_content: Option<JsString>,
    /// Pre-loaded skills.
    pub skills: Option<Vec<Skill>>,
}

fn append_user_rules_section(prompt: &JsString, rules_content: Option<&JsString>) -> JsString {
    let Some(rules_content) = rules_content.filter(|content| !content.is_empty()) else {
        return prompt.to_owned();
    };

    let separator = if prompt.ends_with('\n') { "\n" } else { "\n\n" };
    js_format!("{}{}<user_rules>\n{}\n</user_rules>", prompt, separator, rules_content)
}

fn append_date_and_cwd(prompt: JsString, date: &str, cwd: &JsString) -> JsString {
    js_format!("{}\nCurrent date: {}\nCurrent working directory: {}", prompt, date, cwd)
}

fn build_pyrun_agent_guidelines(tools: &[&str]) -> Vec<JsString> {
    if !tools.contains(&"spawn_agent") || !tools.contains(&"pyrun_eval") {
        return Vec::new();
    }
    vec!["Use pi.agents.spawn(...), pi.agents.list(...), pi.agents.wait(), pi.agents.current(), pi.agents.select(agent_id), pi.messages.last(), pi.messages.enqueue(...), and pi.messages.send(...) for the supported Pi runtime bridge; pi.agents.wait() waits for any active agent and returns no agent output.".into()]
}

fn append_context_files(prompt: &mut JsString, context_files: &[ContextFile]) {
    if context_files.is_empty() {
        return;
    }
    prompt.push_str("\n\n<project_context>\n\n");
    prompt.push_str("Project-specific instructions and guidelines:\n\n");
    for file in context_files {
        prompt.push_str(&js_format!(
            "<project_instructions path=\"{}\">\n{}\n</project_instructions>\n\n",
            file.path,
            file.content
        ));
    }
    prompt.push_str("</project_context>\n");
}

/// Builds the prompt using the current local date, like the frozen Date constructor.
pub fn build_system_prompt(options: &BuildSystemPromptOptions) -> Result<JsString, crate::config::ConfigError> {
    build_system_prompt_with_clock(options, pi_js_compat::date::js_now_millis)
}

/// Rust-only clock injection for deterministic module/consumer proofs; production uses the wrapper above.
pub fn build_system_prompt_with_clock(
    options: &BuildSystemPromptOptions,
    now: impl FnOnce() -> f64,
) -> Result<JsString, crate::config::ConfigError> {
    let millis = pi_js_compat::date::time_clip_millis(now()).expect("system clock must be a valid JavaScript date");
    let fields = pi_js_compat::date::local_date_fields(millis).expect("local system date must be representable");
    let date = format!("{}-{:02}-{:02}", fields.year, fields.month + 1, fields.day);
    let prompt_cwd = options.cwd.replace('\\', "/");
    let append_section = options
        .append_system_prompt
        .as_ref()
        .filter(|text| !text.is_empty())
        .map_or_else(JsString::default, |text| js_format!("\n\n{}", text));

    let context_files = options.context_files.as_deref().unwrap_or(&[]);
    let skills = options.skills.as_deref().unwrap_or(&[]);

    if let Some(custom_prompt) = options.custom_prompt.as_ref().filter(|prompt| !prompt.is_empty()) {
        let mut prompt = custom_prompt.clone();
        prompt.push_str(&append_section);
        append_context_files(&mut prompt, context_files);
        prompt = append_user_rules_section(&prompt, options.rules_content.as_ref());

        let custom_prompt_has_read = options
            .selected_tools
            .as_ref()
            .is_none_or(|tools| tools.iter().any(|tool| tool == "read"));
        if custom_prompt_has_read && !skills.is_empty() {
            prompt.push_str(format_skills_for_prompt(skills));
        }

        return Ok(append_date_and_cwd(prompt, &date, &prompt_cwd));
    }

    let readme_path = get_readme_path()?;
    let docs_path = get_docs_path()?;
    let examples_path = get_examples_path()?;

    let tools: Vec<&str> = match &options.selected_tools {
        Some(tools) => tools.iter().map(String::as_str).collect(),
        None => DEFAULT_ACTIVE_TOOL_NAMES.iter().map(|name| name.as_str()).collect(),
    };
    let tool_snippets = options.tool_snippets.as_ref();
    let visible_tools: Vec<&str> = tools
        .iter()
        .copied()
        .filter(|name| {
            tool_snippets.is_some_and(|snippets| snippets.get(*name).is_some_and(|snippet| !snippet.is_empty()))
        })
        .collect();
    let tools_list = if visible_tools.is_empty() {
        JsString::from("(none)")
    } else {
        JsString::join(
            visible_tools
                .iter()
                .map(|name| {
                    let snippet = tool_snippets
                        .and_then(|snippets| snippets.get(*name))
                        .cloned()
                        .unwrap_or_default();
                    js_format!("- {}: {}", name, snippet)
                })
                .collect::<Vec<_>>(),
            "\n",
        )
    };

    let mut guidelines_list: Vec<JsString> = Vec::new();
    let mut guidelines_set: HashSet<JsString> = HashSet::new();
    let mut add_guideline = |guideline: JsString| {
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
        add_guideline("For codebase research, exploration, or file-reading investigation, you must use spawn_agent with agentType \"explore\" before direct file search, unless the user explicitly asks you not to delegate.".into());
        add_guideline("For scoped code changes, use spawn_agent with agentType \"implement\" unless the change is trivial or the user explicitly asks you not to delegate.".into());
        add_guideline("Before claiming work is fixed, tested, passing, or complete, you must use spawn_agent with agentType \"verifier\" to run proof commands, unless the user explicitly asks you not to delegate.".into());
        add_guideline("For code changes that affect user-facing behavior, public APIs, CLI output, configuration, specs, or changelog-worthy behavior, use spawn_agent with agentType \"documentation-update\" to audit and update relevant docs, specs, or changelogs before final completion.".into());
        add_guideline("Documentation updates do not replace verification; still use a verifier before claiming work is fixed, tested, passing, or complete.".into());
        add_guideline("For final code review or second opinions on non-trivial changes, use spawn_agent with agentType \"reviewer\".".into());
    }

    if has_backgroundable_command_tool && tools.contains(&"wait_agent") {
        let auto_background_after_minutes = DEFAULT_AUTO_DETACH_AFTER_MS / MILLISECONDS_PER_MINUTE;
        add_guideline(js_format!(
            "Supported long-running command tools such as bash and Pyrun are automatically backgrounded after {} minutes; use wait_agent to wait for any agent completion, then inspect reported background job details or attached log files instead of assuming the command stopped.",
            auto_background_after_minutes
        ));
    }

    if has_bash && !has_grep && !has_find && !has_ls {
        add_guideline("Use bash for file operations like ls, rg, find".into());
    }

    if tools.len() > 1 {
        add_guideline("Before making tool calls, identify all calls whose inputs are already known and independent. Emit those calls together in the same assistant response; do not serialize independent exploration calls. Only wait when a later call requires an earlier result.".into());
    }

    for guideline in build_pyrun_agent_guidelines(&tools)
        .iter()
        .chain(options.prompt_guidelines.as_deref().unwrap_or(&[]))
    {
        let normalized = guideline.trim();
        if !normalized.is_empty() {
            add_guideline(normalized);
        }
    }

    add_guideline("Be concise in your responses".into());
    add_guideline("Show file paths clearly when working with files".into());

    let guidelines = JsString::join(
        guidelines_list
            .iter()
            .map(|guideline| js_format!("- {}", guideline))
            .collect::<Vec<_>>(),
        "\n",
    );

    let mut prompt = js_format!(
        "You are an expert coding assistant operating inside pi, a coding agent harness. You help users by reading files, executing commands, editing code, and writing new files.

Available tools:
{}

In addition to the tools above, you may have access to other custom tools depending on the project.

Guidelines:
{}

Pi documentation (read only when the user asks about pi itself, its SDK, extensions, themes, skills, or TUI):
- Main documentation: {}
- Additional docs: {}
- Examples: {} (extensions, custom tools, SDK)
- When reading pi docs or examples, resolve docs/... under Additional docs and examples/... under Examples, not the current working directory
- When asked about: extensions (docs/extensions.md, examples/extensions/), themes (docs/themes.md), skills (docs/skills.md), prompt templates (docs/prompt-templates.md), TUI components (docs/tui.md), keybindings (docs/keybindings.md), SDK integrations (docs/sdk.md), custom providers (docs/custom-provider.md), adding models (docs/models.md), pi packages (docs/packages.md)
- When working on pi topics, read the docs and examples, and follow .md cross-references before implementing
- Always read pi .md files completely and follow links to related docs (e.g., tui.md for TUI API details)", tools_list, guidelines, readme_path, docs_path, examples_path
    );

    prompt.push_str(&append_section);
    append_context_files(&mut prompt, context_files);
    prompt = append_user_rules_section(&prompt, options.rules_content.as_ref());

    if has_read && !skills.is_empty() {
        prompt.push_str(format_skills_for_prompt(skills));
    }

    Ok(append_date_and_cwd(prompt, &date, &prompt_cwd))
}
