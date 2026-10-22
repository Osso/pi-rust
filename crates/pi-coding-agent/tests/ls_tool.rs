use std::sync::Arc;

use pi_agent_core::agent::{AgentTool, AgentToolResult};
use pi_ai::types::TextOrImageContent;
use pi_coding_agent::core::tools::ls::LsTool;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsString, JsValue};

fn execute(tool: &LsTool, params: JsValue) -> Result<AgentToolResult, JsString> {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    runtime
        .block_on(tool.execute("call".into(), params, None, Arc::new(|_result| {}), None))
        .map_err(|error| error.message)
}

fn text(result: &AgentToolResult) -> JsString {
    let parts = result
        .content
        .iter()
        .filter_map(|entry| match entry {
            TextOrImageContent::Text(text) => Some(text.text.clone()),
            TextOrImageContent::Image(_) => None,
        })
        .collect::<Vec<_>>();
    JsString::join(&parts, "\n")
}

#[test]
fn lists_dotfiles_and_directories_sorted_case_insensitively() {
    let temp = tempfile::tempdir().expect("temp dir");
    std::fs::write(temp.path().join("a.txt"), "a").expect("file");
    std::fs::write(temp.path().join(".hidden"), "h").expect("file");
    std::fs::write(temp.path().join("B.txt"), "b").expect("file");
    std::fs::create_dir(temp.path().join("zdir")).expect("dir");
    let result = execute(&LsTool::new(temp.path().to_str().expect("utf-8 test path")), json!({})).expect("ls result");
    assert_eq!(text(&result), ".hidden\na.txt\nB.txt\nzdir/");
    assert!(result.details.is_none());
}

#[test]
fn reports_empty_directories() {
    let temp = tempfile::tempdir().expect("temp dir");
    let result = execute(&LsTool::new(temp.path().to_str().expect("utf-8 test path")), json!({})).expect("ls result");
    assert_eq!(text(&result), "(empty directory)");
}

#[test]
fn enforces_the_entry_limit_and_reports_it() {
    let temp = tempfile::tempdir().expect("temp dir");
    for index in 0..5 {
        std::fs::write(temp.path().join(format!("file-{index}")), "x").expect("file");
    }
    let result = execute(
        &LsTool::new(temp.path().to_str().expect("utf-8 test path")),
        json!({"limit": 2}),
    )
    .expect("ls result");
    assert_eq!(
        text(&result),
        "file-0\nfile-1\n\n[2 entries limit reached. Use limit=4 for more]"
    );
    assert_eq!(result.details.expect("details")["entryLimitReached"], json!(2));
}

#[test]
fn truncates_by_byte_limit() {
    let temp = tempfile::tempdir().expect("temp dir");
    for index in 0..500 {
        std::fs::write(temp.path().join(format!("file-{index:03}-xxxxxxxxxxxxxxxx")), "x").expect("file");
    }
    let result = execute(&LsTool::new(temp.path().to_str().expect("utf-8 test path")), json!({})).expect("ls result");
    let output = text(&result);
    assert!(output.contains("[10.0KB limit reached]"), "{output:?}");
    let details = result.details.expect("details");
    assert_eq!(details["truncation"]["truncated"], json!(true));
    assert!(details.get("entryLimitReached").is_none());
}

#[test]
fn reports_missing_and_non_directory_paths() {
    let temp = tempfile::tempdir().expect("temp dir");
    let tool = LsTool::new(temp.path().to_str().expect("utf-8 test path"));
    let missing = temp.path().join("missing");
    let error = execute(&tool, json!({"path": missing.to_str().expect("utf-8 test path")})).expect_err("missing path");
    assert_eq!(
        error,
        format!("Path not found: {}", missing.to_str().expect("utf-8 test path"))
    );
    let file = temp.path().join("file.txt");
    std::fs::write(&file, "x").expect("file");
    let error = execute(&tool, json!({"path": file.to_str().expect("utf-8 test path")})).expect_err("file path");
    assert_eq!(
        error,
        format!("Not a directory: {}", file.to_str().expect("utf-8 test path"))
    );
}
