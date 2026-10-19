//! Behavioral tests for `server/create-harness.ts` prompt assembly.

use std::sync::Arc;

use pi_agent_core::agent::types::{AgentTool, AgentToolResult, AgentToolUpdateCallback, ToolError};
use pi_agent_core::harness::agent_harness::HarnessTool;
use pi_agent_core::harness::types::AgentToolExecutionContext;
use pi_ai::request::BoxFuture;
use pi_coding_agent::core::system_prompt::BuildSystemPromptOptions;
use pi_coding_agent::server::create_harness::{
    BuildCodingAgentHarnessSystemPromptOptions, CodingAgentHarnessTool, build_coding_agent_harness_system_prompt,
};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

struct FakeTool {
    name: String,
    description: String,
    parameters: Value,
    label: String,
}

impl AgentTool for FakeTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters(&self) -> &Value {
        &self.parameters
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn execute(
        &self,
        _tool_call_id: String,
        _params: Value,
        _signal: Option<CancellationToken>,
        _on_update: AgentToolUpdateCallback,
        _execution: Option<AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        Box::pin(async { Ok(AgentToolResult::text("ok")) })
    }
}

fn tool(name: &str, snippet: Option<&str>, guidelines: &[&str]) -> CodingAgentHarnessTool {
    CodingAgentHarnessTool::new(
        HarnessTool::new(Arc::new(FakeTool {
            name: name.to_owned(),
            description: format!("{name} tool"),
            parameters: json!({"type": "object"}),
            label: name.to_owned(),
        })),
        snippet.map(str::to_owned),
        guidelines.iter().map(|guideline| (*guideline).to_owned()).collect(),
    )
}

#[test]
fn active_tools_contribute_collapsed_snippets_and_guidelines_in_order() {
    let tools = vec![
        tool("read", Some("Read file\ncontents"), &["Use read to examine files."]),
        tool("bash", Some("Execute bash   commands"), &["Inspect PI_* variables."]),
        tool("write", None, &[]),
    ];
    let active = vec!["bash".to_owned(), "read".to_owned(), "write".to_owned()];

    let prompt = build_coding_agent_harness_system_prompt(BuildCodingAgentHarnessSystemPromptOptions {
        cwd: "/workspace",
        tools: &tools,
        active_tool_names: &active,
        system_prompt_options: None,
    });

    assert!(prompt.contains("Execute bash commands"));
    assert!(prompt.contains("Read file contents"));
    let bash_index = prompt.find("Execute bash commands").unwrap();
    let read_index = prompt.find("Read file contents").unwrap();
    assert!(bash_index < read_index, "active-tool order drives snippet order");
    let guideline_bash = prompt.find("Inspect PI_* variables.").unwrap();
    let guideline_read = prompt.find("Use read to examine files.").unwrap();
    assert!(guideline_bash < guideline_read);
    assert!(prompt.contains("/workspace"));
}

#[test]
fn inactive_tools_are_excluded_and_blank_snippets_are_skipped() {
    let tools = vec![
        tool("read", Some("Read file contents"), &["read guideline"]),
        tool("bash", Some("   \n  "), &["bash guideline"]),
    ];
    let active = vec!["bash".to_owned()];

    let prompt = build_coding_agent_harness_system_prompt(BuildCodingAgentHarnessSystemPromptOptions {
        cwd: "/workspace",
        tools: &tools,
        active_tool_names: &active,
        system_prompt_options: Some(&BuildSystemPromptOptions {
            cwd: "/ignored".to_owned(),
            append_system_prompt: Some("appended".to_owned()),
            ..Default::default()
        }),
    });

    assert!(!prompt.contains("Read file contents"));
    assert!(!prompt.contains("read guideline"));
    assert!(prompt.contains("bash guideline"));
    assert!(prompt.contains("appended"));
    assert!(prompt.contains("/workspace"));
    assert!(!prompt.contains("/ignored"));
}
