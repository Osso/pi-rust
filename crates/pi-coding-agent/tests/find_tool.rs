use std::sync::Arc;

use pi_agent_core::agent::{AgentTool, AgentToolResult};
use pi_ai::request::BoxFuture;
use pi_ai::types::TextOrImageContent;
use pi_coding_agent::core::tools::find::{FindOperations, FindTool, FindToolOptions};
use serde_json::json;

fn execute(tool: &FindTool, params: serde_json::Value) -> Result<AgentToolResult, String> {
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

#[test]
fn includes_hidden_files_and_respects_gitignore() {
    let temp = tempfile::tempdir().expect("temp dir");
    std::fs::write(temp.path().join(".hidden.txt"), "x").expect("hidden");
    std::fs::write(temp.path().join("ignored.txt"), "x").expect("ignored");
    std::fs::write(temp.path().join(".gitignore"), "ignored.txt\n").expect("gitignore");
    std::fs::create_dir(temp.path().join(".git")).expect("git dir");
    let tool = FindTool::new(temp.path().to_string_lossy());
    let result = execute(&tool, json!({"pattern": "*hidden*"})).expect("hidden find");
    assert_eq!(text(&result), ".hidden.txt");
    let result = execute(&tool, json!({"pattern": "ignored.txt"})).expect("ignored find");
    assert_eq!(text(&result), "No files found matching pattern");
}

#[test]
fn reports_result_limits_and_flag_like_patterns() {
    let temp = tempfile::tempdir().expect("temp dir");
    std::fs::write(temp.path().join("-dash.txt"), "x").expect("dash file");
    let tool = FindTool::new(temp.path().to_string_lossy());
    let result = execute(&tool, json!({"pattern": "-dash.txt"})).expect("dash find");
    assert_eq!(text(&result), "-dash.txt");
    let result = execute(&tool, json!({"pattern": "-dash.txt", "limit": 1})).expect("limit find");
    assert_eq!(
        text(&result),
        "-dash.txt\n\n[1 results limit reached. Use limit=2 for more, or refine pattern]"
    );
    assert_eq!(result.details.expect("details")["resultLimitReached"], json!(1));
}

struct GlobOperations {
    results: Vec<String>,
    exists: bool,
}

impl FindOperations for GlobOperations {
    fn exists<'a>(&'a self, _absolute_path: &'a str) -> BoxFuture<'a, std::io::Result<bool>> {
        Box::pin(async move { Ok(self.exists) })
    }

    fn supports_glob(&self) -> bool {
        true
    }

    fn glob<'a>(
        &'a self,
        _pattern: &'a str,
        _cwd: &'a str,
        ignore: &'a [String],
        _limit: usize,
    ) -> BoxFuture<'a, std::io::Result<Vec<String>>> {
        assert_eq!(ignore, ["**/node_modules/**".to_owned(), "**/.git/**".to_owned()]);
        Box::pin(async move { Ok(self.results.clone()) })
    }
}

#[test]
fn custom_glob_operations_relativize_absolute_results() {
    let temp = tempfile::tempdir().expect("temp dir");
    let search = temp.path().join("search");
    std::fs::create_dir(&search).expect("search dir");
    let absolute = search.join("nested/file.txt");
    let operations = Arc::new(GlobOperations {
        results: vec![absolute.to_string_lossy().into_owned(), "relative.txt".to_owned()],
        exists: true,
    });
    let tool = FindTool::with_options(
        temp.path().to_string_lossy(),
        FindToolOptions {
            operations: Some(operations),
        },
    );
    let result = execute(&tool, json!({"pattern": "*.txt", "path": "search"})).expect("custom glob");
    assert_eq!(text(&result), "nested/file.txt\nrelative.txt");
}

#[test]
fn custom_glob_operations_report_missing_paths() {
    let temp = tempfile::tempdir().expect("temp dir");
    let tool = FindTool::with_options(
        temp.path().to_string_lossy(),
        FindToolOptions {
            operations: Some(Arc::new(GlobOperations {
                results: Vec::new(),
                exists: false,
            })),
        },
    );
    let missing = temp.path().join("missing");
    let error =
        execute(&tool, json!({"pattern": "*.txt", "path": missing.to_string_lossy()})).expect_err("missing path");
    assert_eq!(error, format!("Path not found: {}", missing.to_string_lossy()));
}
