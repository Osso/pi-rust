use std::sync::{Arc, Mutex};

use pi_agent_core::agent::{AgentTool, AgentToolResult};
use pi_ai::request::BoxFuture;
use pi_ai::types::TextOrImageContent;
use pi_coding_agent::core::tools::code_index::{
    CodeIndexCommandResult, CodeIndexOperations, CodeIndexToolOptions, OutlineTool, ReferencesTool, SymbolTool,
};
use serde_json::json;

fn execute(tool: &dyn AgentTool, params: serde_json::Value) -> Result<AgentToolResult, String> {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    runtime
        .block_on(tool.execute("call".to_owned(), params, None, Arc::new(|_result| {}), None))
        .map_err(|error| error.message)
}

fn text(result: &AgentToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|entry| match entry {
            TextOrImageContent::Text(text) => Some(text.text.clone()),
            TextOrImageContent::Image(_) => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Default)]
struct RecordingOperations {
    calls: Mutex<Vec<Vec<String>>>,
    stdout: String,
    stderr: String,
    exit_code: i32,
}

impl CodeIndexOperations for RecordingOperations {
    fn run<'a>(
        &'a self,
        args: &'a [String],
        cwd: &'a str,
        _signal: Option<&'a tokio_util::sync::CancellationToken>,
    ) -> BoxFuture<'a, std::io::Result<CodeIndexCommandResult>> {
        assert_eq!(cwd, "/repo");
        self.calls.lock().expect("calls").push(args.to_vec());
        Box::pin(async move {
            Ok(CodeIndexCommandResult {
                stdout: self.stdout.clone(),
                stderr: self.stderr.clone(),
                exit_code: Some(self.exit_code),
            })
        })
    }
}

fn options(operations: Arc<RecordingOperations>) -> CodeIndexToolOptions {
    CodeIndexToolOptions {
        operations: Some(operations),
    }
}

#[test]
fn outline_passes_optional_flags_and_trims_output() {
    let operations = Arc::new(RecordingOperations {
        stdout: "outline output\n".to_owned(),
        ..Default::default()
    });
    let tool = OutlineTool::with_options("/repo", options(Arc::clone(&operations)));
    let result = execute(
        &tool,
        json!({"path": "src", "digest": true, "glob": "**/*.ts", "show": "createTool"}),
    )
    .expect("outline result");
    assert_eq!(text(&result), "outline output");
    assert_eq!(
        operations.calls.lock().expect("calls")[0],
        [
            "outline",
            "src",
            "--digest",
            "--glob",
            "**/*.ts",
            "--show",
            "createTool"
        ]
    );
}

#[test]
fn symbol_and_references_pass_filters() {
    let operations = Arc::new(RecordingOperations {
        stdout: "[]\n".to_owned(),
        ..Default::default()
    });
    let symbol = SymbolTool::with_options("/repo", options(Arc::clone(&operations)));
    execute(
        &symbol,
        json!({"name": "createTool", "kind": "function", "file": "src/core/tools"}),
    )
    .expect("symbol result");
    let references = ReferencesTool::with_options("/repo", options(Arc::clone(&operations)));
    execute(&references, json!({"name": "createTool", "kind": "call"})).expect("references result");
    let calls = operations.calls.lock().expect("calls");
    assert_eq!(
        calls[0],
        ["symbol", "createTool", "--kind", "function", "--file", "src/core/tools"]
    );
    assert_eq!(calls[1], ["references", "createTool", "--kind", "call"]);
}

#[test]
fn reports_stderr_on_failure() {
    let operations = Arc::new(RecordingOperations {
        stderr: "index unavailable\n".to_owned(),
        exit_code: 1,
        ..Default::default()
    });
    let tool = SymbolTool::with_options("/repo", options(operations));
    let error = execute(&tool, json!({"name": "missing"})).expect_err("failure");
    assert_eq!(error, "index unavailable");
}

#[test]
fn empty_stdout_reports_no_results() {
    let operations = Arc::new(RecordingOperations::default());
    let tool = ReferencesTool::with_options("/repo", options(operations));
    let result = execute(&tool, json!({"name": "missing"})).expect("no results");
    assert_eq!(text(&result), "No results found");
}
