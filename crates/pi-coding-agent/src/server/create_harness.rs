//! Port of `packages/coding-agent/src/server/create-harness.ts`.
//!
//! The harness factory itself is blocked on the tool system-prompt contributions from
//! `core/tools/{bash,edit,read,write}.ts`; the coding-agent tool wrapper and system-prompt
//! assembly are ported here.

use std::collections::HashMap;

use pi_agent_core::harness::agent_harness::HarnessTool;

use crate::core::system_prompt::{BuildSystemPromptOptions, build_system_prompt};

/// A harness tool plus its coding-agent prompt contribution.
#[derive(Clone)]
pub struct CodingAgentHarnessTool {
    pub tool: HarnessTool,
    pub prompt_snippet: Option<String>,
    pub prompt_guidelines: Vec<String>,
}

impl CodingAgentHarnessTool {
    pub fn new(tool: HarnessTool, prompt_snippet: Option<String>, prompt_guidelines: Vec<String>) -> Self {
        Self {
            tool,
            prompt_snippet,
            prompt_guidelines,
        }
    }

    pub fn name(&self) -> &str {
        self.tool.name()
    }
}

/// Options for [`build_coding_agent_harness_system_prompt`].
pub struct BuildCodingAgentHarnessSystemPromptOptions<'a> {
    pub cwd: &'a str,
    pub tools: &'a [CodingAgentHarnessTool],
    pub active_tool_names: &'a [String],
    pub system_prompt_options: Option<&'a BuildSystemPromptOptions>,
}

/// `buildCodingAgentHarnessSystemPrompt`.
pub fn build_coding_agent_harness_system_prompt(options: BuildCodingAgentHarnessSystemPromptOptions<'_>) -> String {
    let active_tools: Vec<&CodingAgentHarnessTool> = options
        .active_tool_names
        .iter()
        .filter_map(|name| options.tools.iter().find(|tool| tool.name() == name))
        .collect();
    let mut tool_snippets: HashMap<String, String> = HashMap::new();
    for tool in &active_tools {
        let Some(prompt_snippet) = &tool.prompt_snippet else {
            continue;
        };
        let collapsed = prompt_snippet
            .replace(['\r', '\n'], " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if !collapsed.is_empty() {
            tool_snippets.insert(tool.name().to_owned(), collapsed);
        }
    }
    let prompt_guidelines: Vec<String> = active_tools
        .iter()
        .flat_map(|tool| tool.prompt_guidelines.iter().cloned())
        .collect();
    let base = options.system_prompt_options.cloned().unwrap_or_default();
    build_system_prompt(&BuildSystemPromptOptions {
        cwd: options.cwd.to_owned(),
        selected_tools: Some(active_tools.iter().map(|tool| tool.name().to_owned()).collect()),
        tool_snippets: Some(tool_snippets),
        prompt_guidelines: Some(prompt_guidelines),
        ..base
    })
}
