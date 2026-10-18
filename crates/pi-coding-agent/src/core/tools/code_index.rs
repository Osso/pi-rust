//! Ported from `packages/coding-agent/src/core/tools/code-index.ts`.
//!
//! Thin wrappers over the external `code-index` CLI. Renderers are pending
//! `theme.ts`.

use std::io;
use std::sync::Arc;

use pi_agent_core::agent::{AgentTool, AgentToolResult, AgentToolUpdateCallback, ToolError};
use pi_ai::request::BoxFuture;
use pi_ai::types::{TextContent, TextOrImageContent};
use serde_json::{Map, Value, json};
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;

use super::node_fs::node_error_code;
use super::truncate::{DEFAULT_MAX_BYTES, TruncationOptions, format_size, truncate_head};

const MAX_SAFE_INTEGER: usize = 9_007_199_254_740_991;
const SYMBOL_KINDS: [&str; 9] = [
    "function",
    "method",
    "class",
    "trait",
    "interface",
    "struct",
    "enum",
    "property",
    "event",
];
const REFERENCE_KINDS: [&str; 5] = ["call", "inherit", "implement", "import", "trait_impl"];

/// Result of running the external CLI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeIndexCommandResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
}

/// External CLI runner used by the code-index tools.
pub trait CodeIndexOperations: Send + Sync {
    fn run<'a>(
        &'a self,
        args: &'a [String],
        cwd: &'a str,
        signal: Option<&'a CancellationToken>,
    ) -> BoxFuture<'a, io::Result<CodeIndexCommandResult>>;
}

/// Runs the locally installed `code-index` binary.
#[derive(Debug, Default, Clone, Copy)]
pub struct LocalCodeIndexOperations;

impl CodeIndexOperations for LocalCodeIndexOperations {
    fn run<'a>(
        &'a self,
        args: &'a [String],
        cwd: &'a str,
        signal: Option<&'a CancellationToken>,
    ) -> BoxFuture<'a, io::Result<CodeIndexCommandResult>> {
        Box::pin(async move { run_code_index_command("code-index", args, cwd, signal).await })
    }
}

/// Options for the code-index tools.
#[derive(Clone, Default)]
pub struct CodeIndexToolOptions {
    /// Custom command runner. Default: local `code-index` CLI.
    pub operations: Option<Arc<dyn CodeIndexOperations>>,
}

async fn run_code_index_command(
    code_index_path: &str,
    args: &[String],
    cwd: &str,
    signal: Option<&CancellationToken>,
) -> io::Result<CodeIndexCommandResult> {
    if signal.is_some_and(CancellationToken::is_cancelled) {
        return Err(io::Error::new(io::ErrorKind::Interrupted, "aborted"));
    }
    let mut command = tokio::process::Command::new(code_index_path);
    command
        .args(args)
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn()?;
    let mut stdout = child.stdout.take().expect("code-index stdout");
    let mut stderr = child.stderr.take().expect("code-index stderr");
    let stdout_task = tokio::spawn(async move {
        let mut buffer = Vec::new();
        let _ = stdout.read_to_end(&mut buffer).await;
        buffer
    });
    let stderr_task = tokio::spawn(async move {
        let mut buffer = Vec::new();
        let _ = stderr.read_to_end(&mut buffer).await;
        buffer
    });
    let status = wait_with_cancellation(&mut child, signal).await?;
    let stdout = String::from_utf8_lossy(&stdout_task.await.unwrap_or_default()).into_owned();
    let stderr = String::from_utf8_lossy(&stderr_task.await.unwrap_or_default()).into_owned();
    Ok(CodeIndexCommandResult {
        stdout,
        stderr,
        exit_code: status.and_then(|status| status.code()),
    })
}

async fn wait_with_cancellation(
    child: &mut tokio::process::Child,
    signal: Option<&CancellationToken>,
) -> io::Result<Option<std::process::ExitStatus>> {
    let Some(signal) = signal else {
        return child.wait().await.map(Some);
    };
    tokio::select! {
        status = child.wait() => status.map(Some),
        () = signal.cancelled() => {
            let _ = child.start_kill();
            let _ = child.wait().await;
            Err(io::Error::new(io::ErrorKind::Interrupted, "aborted"))
        }
    }
}

fn execute_code_index(
    operations: Arc<dyn CodeIndexOperations>,
    cwd: String,
    args: Vec<String>,
    signal: Option<CancellationToken>,
) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
    Box::pin(async move {
        let result = operations.run(&args, &cwd, signal.as_ref()).await.map_err(|error| {
            if signal.as_ref().is_some_and(CancellationToken::is_cancelled) {
                ToolError::new("Operation aborted")
            } else {
                ToolError::new(format!(
                    "Failed to run code-index: spawn code-index {}",
                    node_error_code(&error)
                ))
            }
        })?;
        if signal.as_ref().is_some_and(CancellationToken::is_cancelled) {
            return Err(ToolError::new("Operation aborted"));
        }
        if result.exit_code != Some(0) {
            let message = if !result.stderr.trim().is_empty() {
                result.stderr.trim().to_owned()
            } else if !result.stdout.trim().is_empty() {
                result.stdout.trim().to_owned()
            } else {
                format!(
                    "code-index exited with code {}",
                    result.exit_code.map_or("null".to_owned(), |code| code.to_string())
                )
            };
            return Err(ToolError::new(message));
        }

        let raw_output = result.stdout.trim_end();
        let output = if raw_output.is_empty() {
            "No results found".to_owned()
        } else {
            raw_output.to_owned()
        };
        let truncation = truncate_head(
            &output,
            TruncationOptions {
                max_lines: MAX_SAFE_INTEGER,
                max_bytes: DEFAULT_MAX_BYTES,
            },
        );
        let mut text = truncation.content.clone();
        let mut details = Map::new();
        if truncation.truncated {
            text.push_str(&format!("\n\n[{} limit reached]", format_size(DEFAULT_MAX_BYTES)));
            details.insert(
                "truncation".to_owned(),
                serde_json::to_value(&truncation).expect("truncation metadata must serialize"),
            );
        }
        Ok(AgentToolResult {
            content: vec![TextOrImageContent::Text(TextContent::new(text))],
            details: (!details.is_empty()).then_some(Value::Object(details)),
            usage: None,
            added_tool_names: None,
            is_error: None,
            terminate: None,
        })
    })
}

/// `outline` tool.
pub struct OutlineTool {
    cwd: String,
    parameters: Value,
    operations: Arc<dyn CodeIndexOperations>,
}

impl OutlineTool {
    pub fn new(cwd: impl Into<String>) -> Self {
        Self::with_options(cwd, CodeIndexToolOptions::default())
    }

    pub fn with_options(cwd: impl Into<String>, options: CodeIndexToolOptions) -> Self {
        Self {
            cwd: cwd.into(),
            parameters: outline_schema(),
            operations: options.operations.unwrap_or_else(|| Arc::new(LocalCodeIndexOperations)),
        }
    }
}

fn outline_schema() -> Value {
    json!({
        "type": "object",
        "required": ["path"],
        "properties": {
            "path": {"type": "string", "description": "File or directory to outline"},
            "digest": {"type": "boolean", "description": "Show compact directory digest (default: false)"},
            "glob": {"type": "string", "description": "Filter files by glob pattern, e.g. '**/*.ts'"},
            "show": {"type": "string", "description": "Show one symbol body or section from the outline"}
        }
    })
}

impl AgentTool for OutlineTool {
    fn name(&self) -> &str {
        "outline"
    }

    fn description(&self) -> &str {
        "Show a structural outline for a file or directory using the external code-index CLI. Output is truncated to 10KB."
    }

    fn parameters(&self) -> &Value {
        &self.parameters
    }

    fn label(&self) -> &str {
        "outline"
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: Value,
        signal: Option<CancellationToken>,
        _on_update: AgentToolUpdateCallback,
        _execution: Option<pi_agent_core::harness::types::AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        let mut args = vec!["outline".to_owned()];
        args.push(string_argument(&params, "path").unwrap_or_default());
        if bool_argument(&params, "digest") {
            args.push("--digest".to_owned());
        }
        if let Some(glob) = string_argument(&params, "glob") {
            args.push("--glob".to_owned());
            args.push(glob);
        }
        if let Some(show) = string_argument(&params, "show") {
            args.push("--show".to_owned());
            args.push(show);
        }
        execute_code_index(Arc::clone(&self.operations), self.cwd.clone(), args, signal)
    }
}

/// `symbol` tool.
pub struct SymbolTool {
    cwd: String,
    parameters: Value,
    operations: Arc<dyn CodeIndexOperations>,
}

impl SymbolTool {
    pub fn new(cwd: impl Into<String>) -> Self {
        Self::with_options(cwd, CodeIndexToolOptions::default())
    }

    pub fn with_options(cwd: impl Into<String>, options: CodeIndexToolOptions) -> Self {
        Self {
            cwd: cwd.into(),
            parameters: symbol_schema(),
            operations: options.operations.unwrap_or_else(|| Arc::new(LocalCodeIndexOperations)),
        }
    }
}

fn symbol_schema() -> Value {
    json!({
        "type": "object",
        "required": ["name"],
        "properties": {
            "name": {"type": "string", "description": "Symbol name to find"},
            "kind": {"type": "string", "enum": SYMBOL_KINDS, "description": "Symbol kind filter"},
            "file": {"type": "string", "description": "Filter by file path substring"}
        }
    })
}

impl AgentTool for SymbolTool {
    fn name(&self) -> &str {
        "symbol"
    }

    fn description(&self) -> &str {
        "Find symbol definitions by name using the external code-index CLI. Output is truncated to 10KB."
    }

    fn parameters(&self) -> &Value {
        &self.parameters
    }

    fn label(&self) -> &str {
        "symbol"
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: Value,
        signal: Option<CancellationToken>,
        _on_update: AgentToolUpdateCallback,
        _execution: Option<pi_agent_core::harness::types::AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        let mut args = vec!["symbol".to_owned()];
        args.push(string_argument(&params, "name").unwrap_or_default());
        if let Some(kind) = string_argument(&params, "kind") {
            args.push("--kind".to_owned());
            args.push(kind);
        }
        if let Some(file) = string_argument(&params, "file") {
            args.push("--file".to_owned());
            args.push(file);
        }
        execute_code_index(Arc::clone(&self.operations), self.cwd.clone(), args, signal)
    }
}

/// `references` tool.
pub struct ReferencesTool {
    cwd: String,
    parameters: Value,
    operations: Arc<dyn CodeIndexOperations>,
}

impl ReferencesTool {
    pub fn new(cwd: impl Into<String>) -> Self {
        Self::with_options(cwd, CodeIndexToolOptions::default())
    }

    pub fn with_options(cwd: impl Into<String>, options: CodeIndexToolOptions) -> Self {
        Self {
            cwd: cwd.into(),
            parameters: references_schema(),
            operations: options.operations.unwrap_or_else(|| Arc::new(LocalCodeIndexOperations)),
        }
    }
}

fn references_schema() -> Value {
    json!({
        "type": "object",
        "required": ["name"],
        "properties": {
            "name": {"type": "string", "description": "Symbol name to find references for"},
            "kind": {"type": "string", "enum": REFERENCE_KINDS, "description": "Reference kind filter"}
        }
    })
}

impl AgentTool for ReferencesTool {
    fn name(&self) -> &str {
        "references"
    }

    fn description(&self) -> &str {
        "Find structural references to a symbol using the external code-index CLI. Output is truncated to 10KB."
    }

    fn parameters(&self) -> &Value {
        &self.parameters
    }

    fn label(&self) -> &str {
        "references"
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: Value,
        signal: Option<CancellationToken>,
        _on_update: AgentToolUpdateCallback,
        _execution: Option<pi_agent_core::harness::types::AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        let mut args = vec!["references".to_owned()];
        args.push(string_argument(&params, "name").unwrap_or_default());
        if let Some(kind) = string_argument(&params, "kind") {
            args.push("--kind".to_owned());
            args.push(kind);
        }
        execute_code_index(Arc::clone(&self.operations), self.cwd.clone(), args, signal)
    }
}

fn string_argument(params: &Value, name: &str) -> Option<String> {
    params.get(name).and_then(Value::as_str).map(str::to_owned)
}

fn bool_argument(params: &Value, name: &str) -> bool {
    params.get(name).and_then(Value::as_bool).unwrap_or(false)
}
