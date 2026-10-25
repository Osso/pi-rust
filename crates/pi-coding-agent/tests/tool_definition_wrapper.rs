use std::sync::{Arc, Mutex};

use pi_agent_core::agent::{AgentTool, AgentToolResult, AgentToolUpdateCallback, ToolError};
use pi_ai::request::BoxFuture;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_coding_agent::core::tools::tool_definition_wrapper::{
    ToolOutputSpill, cap_tool_result_output, create_tool_definition_from_agent_tool, wrap_agent_tool,
};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsString, JsValue, encode_node_utf8};
use serde_json::Value;

struct StubTool {
    parameters: Value,
    result: AgentToolResult,
}

impl AgentTool for StubTool {
    fn name(&self) -> &str {
        "stub"
    }

    fn description(&self) -> pi_js_compat::js_json::JsString {
        "stub description".into()
    }

    fn parameters(&self) -> &Value {
        &self.parameters
    }

    fn label(&self) -> pi_js_compat::js_json::JsString {
        "stub label".into()
    }

    fn prepare_execution(&self) -> Option<pi_agent_core::agent::types::AgentToolPrepareExecution> {
        Some(Arc::new(|_, _| {
            Box::pin(async { Err(ToolError::new("prepare rejected")) })
        }))
    }

    fn execute(
        &self,
        _tool_call_id: String,
        _params: JsValue,
        _signal: Option<AbortSignal>,
        on_update: AgentToolUpdateCallback,
        _execution: Option<pi_agent_core::harness::types::AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        let result = self.result.clone();
        Box::pin(async move {
            (on_update)(result.clone())?;
            Ok(result)
        })
    }
}

fn text_result(text: String) -> AgentToolResult {
    AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(text))],
        details: None,
        is_error: None,
        terminate: None,
    }
}

fn result_text(result: &AgentToolResult) -> JsString {
    JsString::join(
        result.content.iter().filter_map(|entry| match entry {
            TextOrImageContent::Text(text) => Some(text.text.clone()),
            TextOrImageContent::Image(_) => None,
        }),
        "\n",
    )
}

struct PrepareTool {
    parameters: Value,
}

impl AgentTool for PrepareTool {
    fn name(&self) -> &str {
        "prepare"
    }

    fn description(&self) -> pi_js_compat::js_json::JsString {
        JsString::from_utf16(vec![0xd800, 0x61])
    }

    fn parameters(&self) -> &Value {
        &self.parameters
    }

    fn label(&self) -> pi_js_compat::js_json::JsString {
        JsString::from_utf16(vec![0xdc00, 0x62])
    }

    fn prepare_arguments(&self, args: JsValue) -> Result<JsValue, ToolError> {
        Ok(json!({"prepared": args}))
    }

    fn execute(
        &self,
        _tool_call_id: String,
        _params: JsValue,
        _signal: Option<AbortSignal>,
        _on_update: AgentToolUpdateCallback,
        _execution: Option<pi_agent_core::harness::types::AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        Box::pin(async { Ok(text_result("done".to_owned())) })
    }
}

#[test]
fn definitions_from_agent_tools_forward_prepare_arguments() {
    let definition = create_tool_definition_from_agent_tool(Arc::new(PrepareTool {
        parameters: serde_json::json!({"type": "object"}),
    }));
    assert_eq!(definition.description.units(), &[0xd800, 0x61]);
    assert_eq!(definition.label.units(), &[0xdc00, 0x62]);
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
    assert_eq!(
        std::fs::read_to_string(&encode_node_utf8(&path)).expect("spill file"),
        full_text
    );
    let text = result_text(&capped);
    assert!(text.starts_with("line 1476\n"), "{text:?}");
    assert!(text.contains("[Showing last "));
    assert!(text.contains(&pi_js_compat::js_format!("Full output: {}]", path)));
    let details = capped.details.expect("details");
    assert_eq!(details["fullOutputPath"], json!(path));
    assert_eq!(details["truncation"]["truncated"], json!(true));
    let _ = std::fs::remove_file(encode_node_utf8(&path));
}

#[test]
fn existing_output_details_are_never_recapped() {
    let lines = (0..2500).map(|index| format!("line {index}")).collect::<Vec<_>>();
    let mut result = text_result(lines.join("\n"));
    result.details = Some(json!(json!({"fullOutputPath": "/tmp/existing.log"})));
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
        parameters: serde_json::json!({"type": "object"}),
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
        Ok(())
    });
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let final_result = runtime
        .block_on(tool.execute("call".to_owned(), json!(json!({})), None, callback, None))
        .expect("tool result");

    let updates = updates.lock().expect("updates lock");
    assert_eq!(updates.len(), 1);
    let update_path = updates[0].details.as_ref().expect("update details")["fullOutputPath"].clone();
    let final_path = final_result.details.as_ref().expect("final details")["fullOutputPath"].clone();
    assert_eq!(update_path, final_path);
    let path = final_path.as_string().expect("path string").to_owned();
    let _ = std::fs::remove_file(encode_node_utf8(&path));
}

#[test]
fn spill_write_failure_returns_the_node_error() {
    let lines = (0..2500).map(|index| format!("line {index}")).collect::<Vec<_>>();
    let path = std::env::temp_dir()
        .join(format!("pi-missing-dir-{}", std::process::id()))
        .join("spill.log");
    let mut spill = ToolOutputSpill {
        full_output_path: Some(path.to_str().expect("utf-8 test path").to_owned().into()),
    };
    let error =
        cap_tool_result_output(text_result(lines.join("\n")), &mut spill).expect_err("unwritable spill path must fail");
    assert_eq!(
        error.message,
        format!("ENOENT: no such file or directory, open '{}'", path.display())
    );
}

#[tokio::test]
async fn wrapped_agent_tools_keep_spills_local_to_each_execution() {
    let tool = wrap_agent_tool(Arc::new(StubTool {
        parameters: serde_json::json!({}),
        result: text_result("line\n".repeat(2500)),
    }));
    let mut paths = Vec::new();
    for _ in 0..2 {
        let result = tool
            .execute("call".into(), json!({}), None, Arc::new(|_| Ok(())), None)
            .await
            .unwrap();
        paths.push(result.details.unwrap()["fullOutputPath"].as_string().unwrap().clone());
    }
    assert_ne!(paths[0], paths[1], "one spill per invocation, not per tool instance");
    for path in &paths {
        std::fs::remove_file(encode_node_utf8(path)).unwrap();
    }
}

#[tokio::test]
async fn wrapped_agent_tools_forward_prepare_execution_failure() {
    let tool = wrap_agent_tool(Arc::new(StubTool {
        parameters: serde_json::json!({}),
        result: text_result("done".into()),
    }));
    let error = tool.prepare_execution().expect("prepare hook")("call".into(), None)
        .await
        .expect_err("prepare must be forwarded");
    assert_eq!(error.message, "prepare rejected");
}

#[test]
fn spill_names_use_random_bytes_without_a_uuid_version_nibble() {
    let mut observed = std::collections::BTreeSet::new();
    for _ in 0..32 {
        let mut spill = ToolOutputSpill::default();
        cap_tool_result_output(text_result("line\n".repeat(2500)), &mut spill).unwrap();
        let path = encode_node_utf8(&spill.full_output_path.unwrap());
        let name = std::path::Path::new(&path).file_name().unwrap().to_str().unwrap();
        let id = name.strip_prefix("pi-tool-").unwrap().strip_suffix(".log").unwrap();
        assert_eq!(id.len(), 16);
        assert!(id.bytes().all(|byte| byte.is_ascii_hexdigit()));
        observed.insert(id.as_bytes()[12]);
        std::fs::remove_file(path).unwrap();
    }
    assert!(observed.len() > 1, "randomBytes does not force UUID version bits");
}

#[tokio::test]
async fn default_agent_tool_preparation_does_not_create_context() {
    use pi_coding_agent::core::tools::tool_definition_wrapper::wrap_definition;
    let definition = create_tool_definition_from_agent_tool(Arc::new(PrepareTool {
        parameters: serde_json::json!({}),
    }));
    let tool = wrap_definition(
        definition,
        Some(Arc::new(|| panic!("absent preparation must not invoke ctxFactory"))),
    );
    let result = match tool.prepare_execution() {
        Some(prepare) => prepare("call".into(), None).await.unwrap(),
        None => None,
    };
    assert!(result.is_none());
}
