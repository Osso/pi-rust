use std::sync::Arc;

use pi_ai::request::BoxFuture;
use pi_ai::types::{TextContent, TextOrImageContent};
use serde_json::{Value, json};

use crate::agent::{AgentTool, AgentToolResult, ToolError};
use crate::harness::tools::path_utils::resolve_tool_path;
use crate::harness::tools::tool_context::ExecutionToolContext;
use crate::harness::types::FileContent;
use tokio_util::sync::CancellationToken;

const DESCRIPTION: &str = "Write content to a file. Creates the file if it doesn't exist, overwrites if it does. Automatically creates parent directories.";

/// Writes files through the execution environment.
pub struct WriteTool {
    context: Arc<ExecutionToolContext>,
    parameters: Value,
}

impl WriteTool {
    pub fn new(context: Arc<ExecutionToolContext>) -> Self {
        Self {
            context,
            parameters: write_schema(),
        }
    }
}

fn write_schema() -> Value {
    json!({
        "type": "object",
        "required": ["path", "content"],
        "properties": {
            "path": {"type": "string", "description": "Path to the file to write (relative or absolute)"},
            "content": {"type": "string", "description": "Content to write to the file"}
        }
    })
}

impl AgentTool for WriteTool {
    fn name(&self) -> &str {
        "write"
    }

    fn description(&self) -> &str {
        DESCRIPTION
    }

    fn parameters(&self) -> &Value {
        &self.parameters
    }

    fn label(&self) -> &str {
        "write"
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: Value,
        signal: Option<CancellationToken>,
        _on_update: crate::agent::AgentToolUpdateCallback,
        _execution: Option<crate::harness::types::AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        let context = Arc::clone(&self.context);
        Box::pin(async move { execute_write(context, params, signal).await })
    }
}

async fn execute_write(
    context: Arc<ExecutionToolContext>,
    params: Value,
    signal: Option<CancellationToken>,
) -> Result<AgentToolResult, ToolError> {
    let path = params
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::new("write tool requires a string path"))?
        .to_owned();
    let content = params
        .get("content")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::new("write tool requires string content"))?
        .to_owned();

    let absolute_path = resolve_tool_path(context.env.as_ref(), &path, signal.as_ref())
        .await
        .map_err(|error| ToolError::new(error.message))?;
    let env = Arc::clone(&context.env);
    context
        .file_mutation_queue
        .run(env.as_ref(), &absolute_path, async {
            if signal.as_ref().is_some_and(CancellationToken::is_cancelled) {
                return Err(ToolError::new("Operation aborted"));
            }
            env.write_file(&absolute_path, FileContent::Text(&content), signal.as_ref())
                .await
                .map_err(|error| ToolError::new(error.message))?;
            if signal.as_ref().is_some_and(CancellationToken::is_cancelled) {
                return Err(ToolError::new("Operation aborted"));
            }
            Ok(AgentToolResult {
                content: vec![TextOrImageContent::Text(TextContent::new(format!(
                    "Successfully wrote {} bytes to {}",
                    content.encode_utf16().count(),
                    path
                )))],
                details: None,
                usage: None,
                added_tool_names: None,
                is_error: None,
                terminate: None,
            })
        })
        .await
        .map_err(|error| ToolError::new(error.message))?
}
