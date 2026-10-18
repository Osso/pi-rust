//! Ported from `packages/coding-agent/src/core/tools/write.ts`.
//!
//! Queued whole-file writes with parent-directory creation. Renderers are
//! pending `theme.ts`.

use std::io;
use std::path::Path;
use std::sync::Arc;

use pi_agent_core::agent::{AgentTool, AgentToolResult, AgentToolUpdateCallback, ToolError};
use pi_ai::request::BoxFuture;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_js_compat::string::utf16_len;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::file_mutation_queue::with_file_mutation_queue;
use super::node_fs::node_fs_error;
use super::path_utils::resolve_to_cwd;

const DESCRIPTION: &str = "Write content to a file. Creates the file if it doesn't exist, overwrites if it does. Automatically creates parent directories.";

/// File writes used by [`WriteTool`]. Override to delegate writes to a remote system.
pub trait WriteOperations: Send + Sync {
    fn write_file<'a>(&'a self, absolute_path: &'a str, content: &'a str) -> BoxFuture<'a, io::Result<()>>;
    fn mkdir<'a>(&'a self, directory: &'a str) -> BoxFuture<'a, io::Result<()>>;
}

/// Local filesystem operations.
#[derive(Debug, Default, Clone, Copy)]
pub struct LocalWriteOperations;

impl WriteOperations for LocalWriteOperations {
    fn write_file<'a>(&'a self, absolute_path: &'a str, content: &'a str) -> BoxFuture<'a, io::Result<()>> {
        Box::pin(async move { tokio::fs::write(absolute_path, content).await })
    }

    fn mkdir<'a>(&'a self, directory: &'a str) -> BoxFuture<'a, io::Result<()>> {
        Box::pin(async move { tokio::fs::create_dir_all(directory).await })
    }
}

/// Options for [`WriteTool`].
#[derive(Clone, Default)]
pub struct WriteToolOptions {
    /// Custom file writes. Default: local filesystem.
    pub operations: Option<Arc<dyn WriteOperations>>,
}

/// Writes whole files.
pub struct WriteTool {
    cwd: String,
    parameters: Value,
    operations: Arc<dyn WriteOperations>,
}

impl WriteTool {
    pub fn new(cwd: impl Into<String>) -> Self {
        Self::with_options(cwd, WriteToolOptions::default())
    }

    pub fn with_options(cwd: impl Into<String>, options: WriteToolOptions) -> Self {
        Self {
            cwd: cwd.into(),
            parameters: write_schema(),
            operations: options.operations.unwrap_or_else(|| Arc::new(LocalWriteOperations)),
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
        _on_update: AgentToolUpdateCallback,
        _execution: Option<pi_agent_core::harness::types::AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        let cwd = self.cwd.clone();
        let operations = Arc::clone(&self.operations);
        Box::pin(async move { execute_write(&cwd, operations, params, signal).await })
    }
}

async fn execute_write(
    cwd: &str,
    operations: Arc<dyn WriteOperations>,
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
        .ok_or_else(|| ToolError::new("write tool requires a string content"))?
        .to_owned();
    let absolute_path = resolve_to_cwd(&path, cwd).map_err(|error| ToolError::new(error.to_string()))?;
    let directory = Path::new(&absolute_path)
        .parent()
        .map(|parent| parent.to_string_lossy().into_owned())
        .unwrap_or_else(|| ".".to_owned());

    with_file_mutation_queue(&absolute_path, || async {
        check_aborted(signal.as_ref())?;
        operations
            .mkdir(&directory)
            .await
            .map_err(|error| ToolError::new(node_fs_error(&error, "mkdir", &directory)))?;
        check_aborted(signal.as_ref())?;
        operations
            .write_file(&absolute_path, &content)
            .await
            .map_err(|error| ToolError::new(node_fs_error(&error, "open", &absolute_path)))?;
        check_aborted(signal.as_ref())?;
        Ok(AgentToolResult {
            content: vec![TextOrImageContent::Text(TextContent::new(format!(
                "Successfully wrote {} bytes to {path}",
                utf16_len(&content)
            )))],
            details: None,
            usage: None,
            added_tool_names: None,
            is_error: None,
            terminate: None,
        })
    })
    .await
    .map_err(|error| ToolError::new(error.to_string()))?
}

fn check_aborted(signal: Option<&CancellationToken>) -> Result<(), ToolError> {
    if signal.is_some_and(CancellationToken::is_cancelled) {
        return Err(ToolError::new("Operation aborted"));
    }
    Ok(())
}
