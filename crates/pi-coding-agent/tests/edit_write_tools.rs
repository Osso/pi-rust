use std::sync::Arc;

use pi_agent_core::agent::{AgentTool, AgentToolResult};
use pi_ai::types::TextOrImageContent;
use pi_coding_agent::core::tools::edit::EditTool;
use pi_coding_agent::core::tools::write::WriteTool;
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

#[test]
fn writes_file_contents_and_creates_parent_directories() {
    let temp = tempfile::tempdir().expect("temp dir");
    let tool = WriteTool::new(temp.path().to_string_lossy());
    let file = temp.path().join("nested/dir/write-test.txt");
    let result = execute(
        &tool,
        json!({"path": file.to_string_lossy(), "content": "Test content"}),
    )
    .expect("write result");
    assert_eq!(
        text(&result),
        format!("Successfully wrote 12 bytes to {}", file.to_string_lossy())
    );
    assert_eq!(std::fs::read_to_string(&file).expect("written file"), "Test content");
}

#[test]
fn write_reports_utf16_code_unit_length() {
    let temp = tempfile::tempdir().expect("temp dir");
    let tool = WriteTool::new(temp.path().to_string_lossy());
    let file = temp.path().join("emoji.txt");
    let result = execute(&tool, json!({"path": file.to_string_lossy(), "content": "a😀b"})).expect("write result");
    assert!(
        text(&result).contains("Successfully wrote 4 bytes"),
        "{}",
        text(&result)
    );
}

#[test]
fn edit_replaces_text_and_reports_diff_and_patch() {
    let temp = tempfile::tempdir().expect("temp dir");
    let file = temp.path().join("edit-test.txt");
    std::fs::write(&file, "Hello, world!").expect("fixture");
    let tool = EditTool::new(temp.path().to_string_lossy());
    let result = execute(
        &tool,
        json!({"path": file.to_string_lossy(), "edits": [{"oldText": "world", "newText": "universe"}]}),
    )
    .expect("edit result");
    assert_eq!(
        text(&result),
        format!("Successfully replaced 1 block(s) in {}.", file.to_string_lossy())
    );
    assert_eq!(std::fs::read_to_string(&file).expect("edited file"), "Hello, universe!");
    let details = result.details.expect("details");
    assert!(
        details["diff"].as_str().expect("diff").contains("+1 Hello, universe!"),
        "{}",
        details["diff"]
    );
    assert!(details["patch"].as_str().expect("patch").contains("@@"));
    assert_eq!(details["firstChangedLine"], json!(1));
}

#[test]
fn edit_fails_when_text_is_missing_or_ambiguous() {
    let temp = tempfile::tempdir().expect("temp dir");
    let file = temp.path().join("edit-test.txt");
    std::fs::write(&file, "same\nsame").expect("fixture");
    let tool = EditTool::new(temp.path().to_string_lossy());
    let error = execute(
        &tool,
        json!({"path": file.to_string_lossy(), "edits": [{"oldText": "missing", "newText": "x"}]}),
    )
    .expect_err("missing text");
    assert!(error.contains("Could not find the exact text"), "{error}");
    let error = execute(
        &tool,
        json!({"path": file.to_string_lossy(), "edits": [{"oldText": "same", "newText": "x"}]}),
    )
    .expect_err("ambiguous text");
    assert!(error.contains("Found 2 occurrences"), "{error}");
    assert_eq!(std::fs::read_to_string(&file).expect("unchanged file"), "same\nsame");
}

#[test]
fn edit_reports_enoent_for_missing_files() {
    let temp = tempfile::tempdir().expect("temp dir");
    let tool = EditTool::new(temp.path().to_string_lossy());
    let file = temp.path().join("missing.txt");
    let error = execute(
        &tool,
        json!({"path": file.to_string_lossy(), "edits": [{"oldText": "a", "newText": "b"}]}),
    )
    .expect_err("missing file");
    assert_eq!(
        error,
        format!("Could not edit file: {}. Error code: ENOENT.", file.to_string_lossy())
    );
}

#[test]
fn edit_rejects_empty_and_overlapping_edits() {
    let temp = tempfile::tempdir().expect("temp dir");
    let file = temp.path().join("edit-test.txt");
    std::fs::write(&file, "abcdef").expect("fixture");
    let tool = EditTool::new(temp.path().to_string_lossy());
    let error = execute(&tool, json!({"path": file.to_string_lossy(), "edits": []})).expect_err("empty edits");
    assert_eq!(
        error,
        "Edit tool input is invalid. edits must contain at least one replacement."
    );
    let error = execute(
        &tool,
        json!({"path": file.to_string_lossy(), "edits": [
            {"oldText": "abcd", "newText": "x"},
            {"oldText": "cdef", "newText": "y"}
        ]}),
    )
    .expect_err("overlapping edits");
    assert!(error.contains("overlap"), "{error}");
    assert_eq!(std::fs::read_to_string(&file).expect("unchanged file"), "abcdef");
}

#[test]
fn edit_applies_disjoint_edits_against_the_original_content() {
    let temp = tempfile::tempdir().expect("temp dir");
    let file = temp.path().join("edit-test.txt");
    std::fs::write(&file, "alpha\nbeta\ngamma\ndelta").expect("fixture");
    let tool = EditTool::new(temp.path().to_string_lossy());
    execute(
        &tool,
        json!({"path": file.to_string_lossy(), "edits": [
            {"oldText": "alpha", "newText": "one"},
            {"oldText": "gamma", "newText": "three"}
        ]}),
    )
    .expect("multi edit");
    assert_eq!(
        std::fs::read_to_string(&file).expect("edited file"),
        "one\nbeta\nthree\ndelta"
    );
}

#[test]
fn edit_preserves_crlf_and_bom() {
    let temp = tempfile::tempdir().expect("temp dir");
    let file = temp.path().join("edit-test.txt");
    std::fs::write(&file, "\u{feff}first\r\nsecond\r\n").expect("fixture");
    let tool = EditTool::new(temp.path().to_string_lossy());
    execute(
        &tool,
        json!({"path": file.to_string_lossy(), "edits": [{"oldText": "first\nsecond", "newText": "one\ntwo"}]}),
    )
    .expect("crlf edit");
    assert_eq!(
        std::fs::read_to_string(&file).expect("edited file"),
        "\u{feff}one\r\ntwo\r\n"
    );
}

#[test]
fn prepare_arguments_accepts_json_strings_and_legacy_fields() {
    let tool = EditTool::new(".");
    let prepared = tool
        .prepare_arguments(json!({
            "path": "file.txt",
            "edits": "[{\"oldText\":\"a\",\"newText\":\"b\"}]"
        }))
        .expect("prepare arguments");
    assert_eq!(
        prepared,
        json!({"path": "file.txt", "edits": [{"oldText": "a", "newText": "b"}]})
    );
    let prepared = tool
        .prepare_arguments(json!({
            "path": "file.txt",
            "edits": [{"oldText": "a", "newText": "b"}],
            "oldText": "c",
            "newText": "d"
        }))
        .expect("prepare arguments");
    assert_eq!(
        prepared,
        json!({"path": "file.txt", "edits": [
            {"oldText": "a", "newText": "b"},
            {"oldText": "c", "newText": "d"}
        ]})
    );
}
