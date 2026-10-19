use std::sync::{Arc, Mutex};

use pi_agent_core::agent::{AgentTool, AgentToolResult, AgentToolUpdateCallback, ToolError};
use pi_ai::request::BoxFuture;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_coding_agent::core::tools::tool_definition_wrapper::{
    ToolOutputSpill, cap_tool_result_output, create_tool_definition_from_agent_tool, wrap_agent_tool,
};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

struct StubTool {
    parameters: Value,
    result: AgentToolResult,
}

impl AgentTool for StubTool {
    fn name(&self) -> &str {
        "stub"
    }

    fn description(&self) -> &str {
        "stub description"
    }

    fn parameters(&self) -> &Value {
        &self.parameters
    }

    fn label(&self) -> &str {
        "stub label"
    }

    fn execute(
        &self,
        _tool_call_id: String,
        _params: Value,
        _signal: Option<CancellationToken>,
        on_update: AgentToolUpdateCallback,
        _execution: Option<pi_agent_core::harness::types::AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        let result = self.result.clone();
        Box::pin(async move {
            (on_update)(result.clone());
            Ok(result)
        })
    }
}

fn text_result(text: String) -> AgentToolResult {
    AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(text))],
        details: None,
        usage: None,
        added_tool_names: None,
        is_error: None,
        terminate: None,
    }
}

fn result_text(result: &AgentToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|entry| match entry {
            TextOrImageContent::Text(text) => Some(text.text.as_str()),
            TextOrImageContent::Image(_) => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

struct PrepareTool {
    parameters: Value,
}

impl AgentTool for PrepareTool {
    fn name(&self) -> &str {
        "prepare"
    }

    fn description(&self) -> &str {
        "prepare description"
    }

    fn parameters(&self) -> &Value {
        &self.parameters
    }

    fn label(&self) -> &str {
        "prepare"
    }

    fn prepare_arguments(&self, args: Value) -> Result<Value, ToolError> {
        Ok(json!({"prepared": args}))
    }

    fn execute(
        &self,
        _tool_call_id: String,
        _params: Value,
        _signal: Option<CancellationToken>,
        _on_update: AgentToolUpdateCallback,
        _execution: Option<pi_agent_core::harness::types::AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        Box::pin(async { Ok(text_result("done".to_owned())) })
    }
}

#[test]
fn definitions_from_agent_tools_forward_prepare_arguments() {
    let definition = create_tool_definition_from_agent_tool(Arc::new(PrepareTool {
        parameters: json!({"type": "object"}),
    }));
    let prepare = definition.prepare_arguments.expect("prepare arguments forwarded");
    assert_eq!(
        prepare(&json!({"value": 1})).expect("prepare arguments"),
        json!({"prepared": {"value": 1}})
    );
}

#[test]
fn small_results_pass_through_unchanged() {
    let mut spill = ToolOutputSpill::default();
    let result = text_result("hello".to_owned());
    let capped = cap_tool_result_output(result.clone(), &mut spill).expect("cap result");
    assert_eq!(capped.content, result.content);
    assert_eq!(capped.details, None);
    assert!(spill.full_output_path.is_none());
}

#[test]
fn oversized_results_spill_the_full_text_and_keep_the_tail() {
    let lines = (0..2500).map(|index| format!("line {index}")).collect::<Vec<_>>();
    let full_text = lines.join("\n");
    let mut spill = ToolOutputSpill::default();
    let capped = cap_tool_result_output(text_result(full_text.clone()), &mut spill).expect("cap result");

    let path = spill.full_output_path.clone().expect("spill path");
    assert_eq!(std::fs::read_to_string(&path).expect("spill file"), full_text);
    let text = result_text(&capped);
    assert!(text.starts_with("line 1476\n"), "{text}");
    assert!(text.contains("[Showing last "));
    assert!(text.contains(&format!("Full output: {path}]")));
    let details = capped.details.expect("details");
    assert_eq!(details["fullOutputPath"], json!(path));
    assert_eq!(details["truncation"]["truncated"], json!(true));
    let _ = std::fs::remove_file(path);
}

#[test]
fn existing_output_details_are_never_recapped() {
    let lines = (0..2500).map(|index| format!("line {index}")).collect::<Vec<_>>();
    let mut result = text_result(lines.join("\n"));
    result.details = Some(json!({"fullOutputPath": "/tmp/existing.log"}));
    let mut spill = ToolOutputSpill::default();
    let capped = cap_tool_result_output(result.clone(), &mut spill).expect("cap result");
    assert_eq!(capped.content, result.content);
    assert_eq!(capped.details, result.details);
    assert!(spill.full_output_path.is_none());
}

#[test]
fn wrapped_tools_cap_partial_updates_and_reuse_one_spill_file() {
    let lines = (0..2500).map(|index| format!("line {index}")).collect::<Vec<_>>();
    let inner: Arc<dyn AgentTool> = Arc::new(StubTool {
        parameters: json!({"type": "object"}),
        result: text_result(lines.join("\n")),
    });
    let tool = wrap_agent_tool(inner);
    assert_eq!(tool.name(), "stub");
    assert_eq!(tool.description(), "stub description");
    assert_eq!(tool.label(), "stub label");

    let updates = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&updates);
    let callback: AgentToolUpdateCallback = Arc::new(move |result| {
        captured.lock().expect("updates lock").push(result);
    });
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let final_result = runtime
        .block_on(tool.execute("call".to_owned(), json!({}), None, callback, None))
        .expect("tool result");

    let updates = updates.lock().expect("updates lock");
    assert_eq!(updates.len(), 1);
    let update_path = updates[0].details.as_ref().expect("update details")["fullOutputPath"].clone();
    let final_path = final_result.details.as_ref().expect("final details")["fullOutputPath"].clone();
    assert_eq!(update_path, final_path);
    let path = final_path.as_str().expect("path string").to_owned();
    let _ = std::fs::remove_file(path);
}

#[test]
fn spill_write_failure_returns_the_node_error() {
    let lines = (0..2500).map(|index| format!("line {index}")).collect::<Vec<_>>();
    let path = std::env::temp_dir()
        .join(format!("pi-missing-dir-{}", std::process::id()))
        .join("spill.log");
    let mut spill = ToolOutputSpill {
        full_output_path: Some(path.display().to_string()),
        error: None,
    };
    let error =
        cap_tool_result_output(text_result(lines.join("\n")), &mut spill).expect_err("unwritable spill path must fail");
    assert_eq!(
        error.message,
        format!("ENOENT: no such file or directory, open '{}'", path.display())
    );
}
