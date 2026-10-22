//! Native adapter behavior for extension ToolDefinition executable fields.
use pi_agent_core::agent::{AgentToolResult, ToolError};
use pi_ai::request::BoxFuture;
use pi_coding_agent::core::extensions::types::ContextTool;
use pi_js_compat::js_json::JsValue;
use serde_json::{Value, json};

struct EchoTool;

impl ContextTool<Value> for EchoTool {
    fn name(&self) -> &str {
        "echo"
    }

    fn description(&self) -> pi_js_compat::js_json::JsString {
        "Echo the context".into()
    }

    fn parameters(&self) -> &Value {
        static PARAMETERS: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        PARAMETERS.get_or_init(|| json!({"type": "object", "properties": {}}))
    }

    fn label(&self) -> pi_js_compat::js_json::JsString {
        "Echo".into()
    }

    fn execute(
        &self,
        _tool_call_id: String,
        _params: pi_js_compat::js_json::JsValue,
        _signal: Option<tokio_util::sync::CancellationToken>,
        _on_update: Option<pi_agent_core::agent::AgentToolUpdateCallback>,
        context: Value,
        _execution: Option<pi_agent_core::harness::types::AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        Box::pin(async move {
            Ok(AgentToolResult {
                content: vec![pi_ai::types::TextOrImageContent::Text(pi_ai::types::TextContent::new(
                    context["label"].as_str().unwrap_or_default(),
                ))],
                details: None,
                is_error: None,
                terminate: None,
            })
        })
    }
}

#[tokio::test]
async fn context_tool_executes_with_the_resolved_context() {
    let tool = EchoTool;
    assert_eq!(tool.name(), "echo");
    assert_eq!(tool.label(), "Echo");
    assert_eq!(tool.parameters()["type"], "object");
    let result = tool
        .execute(
            "call-1".to_owned(),
            JsValue::from(json!({})),
            None,
            None,
            json!({"label": "turn-context"}),
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(result).unwrap(),
        json!({"content": [{"type": "text", "text": "turn-context"}]})
    );
}
