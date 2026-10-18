//! Behavioral port of `packages/agent/test/harness/tools.test.ts`.

use std::collections::HashMap;
use std::os::unix::fs::symlink;
use std::sync::{Arc, Mutex};

use pi_agent_core::agent::{AgentTool, AgentToolResult, AgentToolUpdateCallback, ToolError};
use pi_agent_core::harness::env::{NodeExecutionEnv, NodeExecutionEnvOptions};
use pi_agent_core::harness::tools::bash::{BashExecution, BashPrepare, BashTool, BashToolOptions};
use pi_agent_core::harness::tools::edit::EditTool;
use pi_agent_core::harness::tools::read::{
    ImageProcessorOptions, ReadImageProcessor, ReadImageProcessorResult, ReadTool,
};
use pi_agent_core::harness::tools::tool_context::ExecutionToolContext;
use pi_agent_core::harness::tools::write::WriteTool;
use pi_agent_core::harness::types::ExecutionEnv;
use pi_ai::request::BoxFuture;
use pi_ai::types::TextOrImageContent;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

fn temp_env() -> (TempDir, Arc<ExecutionToolContext>) {
    let root = tempfile::tempdir().expect("temp dir");
    let env = NodeExecutionEnv::new(NodeExecutionEnvOptions {
        cwd: root.path().to_string_lossy().into_owned(),
        shell_path: None,
        shell_env: HashMap::new(),
    });
    let env: Arc<dyn ExecutionEnv> = Arc::new(env);
    (root, Arc::new(ExecutionToolContext::new(env)))
}

fn no_updates() -> AgentToolUpdateCallback {
    Arc::new(|_: AgentToolResult| {})
}

async fn execute(tool: &dyn AgentTool, input: Value) -> Result<AgentToolResult, ToolError> {
    tool.execute("tool-call".to_owned(), input, None, no_updates(), None)
        .await
}

async fn execute_aborted(
    tool: &dyn AgentTool,
    input: Value,
    signal: &CancellationToken,
) -> Result<AgentToolResult, ToolError> {
    tool.execute("tool-call".to_owned(), input, Some(signal.clone()), no_updates(), None)
        .await
}

fn text_output(result: &AgentToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|part| match part {
            TextOrImageContent::Text(text) => Some(text.text.as_str()),
            TextOrImageContent::Image(_) => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn tool_schemas_match_typebox_json() {
    let (_root, context) = temp_env();
    let tools: Vec<Arc<dyn AgentTool>> = vec![
        Arc::new(ReadTool::new(Arc::clone(&context))),
        Arc::new(WriteTool::new(Arc::clone(&context))),
        Arc::new(EditTool::new(Arc::clone(&context))),
        Arc::new(BashTool::new(Arc::clone(&context))),
    ];
    let expected = [
        r#"{"type":"object","required":["path"],"properties":{"path":{"type":"string","description":"Path to the file to read (relative or absolute)"},"offset":{"type":"number","description":"Line number to start reading from (1-indexed)"},"limit":{"type":"number","description":"Maximum number of lines to read"}}}"#,
        r#"{"type":"object","required":["path","content"],"properties":{"path":{"type":"string","description":"Path to the file to write (relative or absolute)"},"content":{"type":"string","description":"Content to write to the file"}}}"#,
        r#"{"type":"object","required":["path","edits"],"properties":{"path":{"type":"string","description":"Path to the file to edit (relative or absolute)"},"edits":{"type":"array","items":{"type":"object","required":["oldText","newText"],"properties":{"oldText":{"type":"string","description":"Exact text for one targeted replacement. It must be unique in the original file and must not overlap with any other edits[].oldText in the same call."},"newText":{"type":"string","description":"Replacement text for this targeted edit."}}},"description":"One or more targeted replacements. Each edit is matched against the original file, not incrementally. Do not include overlapping or nested edits. If two changes touch the same block or nearby lines, merge them into one edit instead."}}}"#,
        r#"{"type":"object","required":["command"],"properties":{"command":{"type":"string","description":"Bash command to execute"},"timeout":{"type":"number","description":"Timeout in seconds (optional, no default timeout)"}}}"#,
    ];
    for (tool, expected) in tools.iter().zip(expected) {
        assert_eq!(
            serde_json::to_string(tool.parameters()).unwrap(),
            expected,
            "{}",
            tool.name()
        );
    }
    assert_eq!(tools[0].name(), "read");
    assert_eq!(tools[0].label(), "read");
    assert_eq!(
        tools[0].description(),
        "Read the contents of a file. Supports text files and images (jpg, png, gif, webp, bmp). Images are sent as attachments. For text files, output is truncated to 2000 lines or 10KB (whichever is hit first). Use offset/limit for large files. When you need the full file, continue with offset until complete."
    );
}

#[tokio::test]
async fn read_supports_offsets_limits_and_continuation_notices() {
    let (_root, context) = temp_env();
    let lines = (1..=100)
        .map(|line| format!("Line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    context
        .env
        .write_file(
            "test.txt",
            pi_agent_core::harness::types::FileContent::Text(&lines),
            None,
        )
        .await
        .unwrap();
    let result = execute(
        &ReadTool::new(Arc::clone(&context)),
        json!({"path": "test.txt", "offset": 41, "limit": 20}),
    )
    .await
    .unwrap();
    let output = text_output(&result);
    assert!(!output.contains("Line 40"));
    assert!(output.contains("Line 41"));
    assert!(output.contains("Line 60"));
    assert!(!output.contains("Line 61"));
    assert!(output.contains("[40 more lines in file. Use offset=61 to continue.]"));
    assert!(result.details.is_none());
}

#[tokio::test]
async fn read_truncates_large_text_by_bytes_and_accepts_exact_line_limits() {
    let (_root, context) = temp_env();
    let lines = (1..=2500)
        .map(|line| format!("Line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    context
        .env
        .write_file(
            "large.txt",
            pi_agent_core::harness::types::FileContent::Text(&lines),
            None,
        )
        .await
        .unwrap();
    let result = execute(&ReadTool::new(Arc::clone(&context)), json!({"path": "large.txt"}))
        .await
        .unwrap();
    assert!(
        text_output(&result).contains("[Showing lines 1-1134 of 2500 (10.0KB limit). Use offset=1135 to continue.]")
    );
    let details = result.details.unwrap();
    assert_eq!(details["truncation"]["truncated"], json!(true));
    assert_eq!(details["truncation"]["truncatedBy"], json!("bytes"));
    assert_eq!(details["truncation"]["totalLines"], json!(2500));
    assert_eq!(details["truncation"]["outputLines"], json!(1134));

    let exact = (0..2000).map(|_| "x").collect::<Vec<_>>().join("\n") + "\n";
    context
        .env
        .write_file(
            "exact.txt",
            pi_agent_core::harness::types::FileContent::Text(&exact),
            None,
        )
        .await
        .unwrap();
    let result = execute(&ReadTool::new(Arc::clone(&context)), json!({"path": "exact.txt"}))
        .await
        .unwrap();
    assert!(result.details.is_none());
    assert!(!text_output(&result).contains("Use offset="));
}

#[tokio::test]
async fn read_rejects_offsets_beyond_the_file() {
    let (_root, context) = temp_env();
    context
        .env
        .write_file(
            "short.txt",
            pi_agent_core::harness::types::FileContent::Text("one\ntwo\nthree"),
            None,
        )
        .await
        .unwrap();
    let error = execute(
        &ReadTool::new(Arc::clone(&context)),
        json!({"path": "short.txt", "offset": 100}),
    )
    .await
    .unwrap_err();
    assert_eq!(error.message, "Offset 100 is beyond end of file (3 lines total)");
}

const PNG_BASE64: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNgYGD4DwABBAEAX+XDSwAAAABJRU5ErkJggg==";

#[tokio::test]
async fn read_detects_images_and_omits_bmp_without_a_processor() {
    let (_root, context) = temp_env();
    let png = decode_base64(PNG_BASE64);
    context
        .env
        .write_file(
            "image.txt",
            pi_agent_core::harness::types::FileContent::Bytes(&png),
            None,
        )
        .await
        .unwrap();
    let result = execute(&ReadTool::new(Arc::clone(&context)), json!({"path": "image.txt"}))
        .await
        .unwrap();
    assert_eq!(text_output(&result), "Read image file [image/png]");
    assert_eq!(
        result.content[1],
        TextOrImageContent::Image(pi_ai::types::ImageContent::new(PNG_BASE64, "image/png"))
    );

    let bmp = tiny_bmp();
    context
        .env
        .write_file(
            "image.bmp",
            pi_agent_core::harness::types::FileContent::Bytes(&bmp),
            None,
        )
        .await
        .unwrap();
    let result = execute(&ReadTool::new(Arc::clone(&context)), json!({"path": "image.bmp"}))
        .await
        .unwrap();
    assert_eq!(
        text_output(&result),
        "Read image file [image/bmp]\n[Image omitted: configure an imageProcessor to convert BMP images.]"
    );
}

struct RecordingProcessor {
    received: Mutex<Option<(Vec<u8>, String, bool)>>,
}

impl ReadImageProcessor for RecordingProcessor {
    fn process<'a>(
        &'a self,
        bytes: &'a [u8],
        mime_type: &'a str,
        options: ImageProcessorOptions,
    ) -> BoxFuture<'a, ReadImageProcessorResult> {
        Box::pin(async move {
            *self.received.lock().unwrap() = Some((bytes.to_vec(), mime_type.to_owned(), options.auto_resize_images));
            ReadImageProcessorResult::Ok {
                data: "converted".to_owned(),
                mime_type: "image/png".to_owned(),
                hints: vec!["[Image converted from image/bmp to image/png.]".to_owned()],
            }
        })
    }
}

#[tokio::test]
async fn read_delegates_image_conversion_and_resizing_to_an_injected_processor() {
    let (_root, context) = temp_env();
    let bmp = tiny_bmp();
    context
        .env
        .write_file(
            "image.bmp",
            pi_agent_core::harness::types::FileContent::Bytes(&bmp),
            None,
        )
        .await
        .unwrap();
    let processor = Arc::new(RecordingProcessor {
        received: Mutex::new(None),
    });
    let tool = ReadTool::with_options(
        Arc::clone(&context),
        pi_agent_core::harness::tools::read::ReadToolOptions {
            auto_resize_images: Some(false),
            image_processor: Some(processor.clone()),
        },
    );
    let result = execute(&tool, json!({"path": "image.bmp"})).await.unwrap();
    assert_eq!(
        *processor.received.lock().unwrap(),
        Some((bmp.clone(), "image/bmp".to_owned(), false))
    );
    assert_eq!(
        text_output(&result),
        "Read image file [image/png]\n[Image converted from image/bmp to image/png.]"
    );
    assert_eq!(
        result.content[1],
        TextOrImageContent::Image(pi_ai::types::ImageContent::new("converted", "image/png"))
    );
}

#[tokio::test]
async fn write_creates_parent_directories_and_reports_utf16_length() {
    let (_root, context) = temp_env();
    let result = execute(
        &WriteTool::new(Arc::clone(&context)),
        json!({"path": "nested/dir/file.txt", "content": "hello"}),
    )
    .await
    .unwrap();
    assert_eq!(
        text_output(&result),
        "Successfully wrote 5 bytes to nested/dir/file.txt"
    );
    assert!(result.details.is_none());
    assert_eq!(
        context.env.read_text_file("nested/dir/file.txt", None).await.unwrap(),
        "hello"
    );

    let result = execute(
        &WriteTool::new(Arc::clone(&context)),
        json!({"path": "emoji.txt", "content": "\u{1f600}"}),
    )
    .await
    .unwrap();
    assert_eq!(text_output(&result), "Successfully wrote 2 bytes to emoji.txt");
}

#[tokio::test]
async fn edit_applies_disjoint_edits_and_returns_diff_and_patch() {
    let (_root, context) = temp_env();
    let original = "alpha\nbeta\ngamma\ndelta\n";
    context
        .env
        .write_file(
            "edit.txt",
            pi_agent_core::harness::types::FileContent::Text(original),
            None,
        )
        .await
        .unwrap();
    let result = execute(
        &EditTool::new(Arc::clone(&context)),
        json!({
            "path": "edit.txt",
            "edits": [
                {"oldText": "alpha\n", "newText": "ALPHA\n"},
                {"oldText": "gamma\n", "newText": "GAMMA\n"}
            ]
        }),
    )
    .await
    .unwrap();
    assert_eq!(text_output(&result), "Successfully replaced 2 block(s) in edit.txt.");
    let details = result.details.unwrap();
    let diff = details["diff"].as_str().unwrap();
    assert!(diff.contains("ALPHA"));
    assert!(diff.contains("GAMMA"));
    assert_eq!(
        details["patch"].as_str().unwrap(),
        "--- edit.txt\n+++ edit.txt\n@@ -1,4 +1,4 @@\n-alpha\n+ALPHA\n beta\n-gamma\n+GAMMA\n delta\n"
    );
    assert_eq!(details["firstChangedLine"], json!(1));
    assert_eq!(
        context.env.read_text_file("edit.txt", None).await.unwrap(),
        "ALPHA\nbeta\nGAMMA\ndelta\n"
    );
}

#[tokio::test]
async fn edit_rejects_overlaps_missing_and_duplicate_targets() {
    let (_root, context) = temp_env();
    context
        .env
        .write_file(
            "edit.txt",
            pi_agent_core::harness::types::FileContent::Text("one\ntwo\nthree\n"),
            None,
        )
        .await
        .unwrap();
    let tool = EditTool::new(Arc::clone(&context));
    let error = execute(
        &tool,
        json!({
            "path": "edit.txt",
            "edits": [
                {"oldText": "one\ntwo\n", "newText": "ONE\nTWO\n"},
                {"oldText": "two\nthree\n", "newText": "TWO\nTHREE\n"}
            ]
        }),
    )
    .await
    .unwrap_err();
    assert!(error.message.contains("overlap"));
    assert_eq!(
        context.env.read_text_file("edit.txt", None).await.unwrap(),
        "one\ntwo\nthree\n"
    );

    context
        .env
        .write_file(
            "dupes.txt",
            pi_agent_core::harness::types::FileContent::Text("foo foo foo"),
            None,
        )
        .await
        .unwrap();
    let missing = execute(
        &tool,
        json!({"path": "dupes.txt", "edits": [{"oldText": "bar", "newText": "baz"}]}),
    )
    .await
    .unwrap_err();
    assert!(missing.message.contains("Could not find the exact text"));
    let duplicate = execute(
        &tool,
        json!({"path": "dupes.txt", "edits": [{"oldText": "foo", "newText": "bar"}]}),
    )
    .await
    .unwrap_err();
    assert!(duplicate.message.contains("Found 3 occurrences"));
}

#[tokio::test]
async fn edit_serializes_through_canonical_and_symlink_paths() {
    let (root, context) = temp_env();
    context
        .env
        .write_file(
            "target.txt",
            pi_agent_core::harness::types::FileContent::Text("alpha\nbeta\ngamma\n"),
            None,
        )
        .await
        .unwrap();
    symlink("target.txt", root.path().join("link.txt")).unwrap();
    let tool = Arc::new(EditTool::new(Arc::clone(&context)));
    let first = {
        let tool = Arc::clone(&tool);
        tokio::spawn(async move {
            execute(
                tool.as_ref(),
                json!({"path": "target.txt", "edits": [{"oldText": "alpha", "newText": "ALPHA"}]}),
            )
            .await
        })
    };
    let second = {
        let tool = Arc::clone(&tool);
        tokio::spawn(async move {
            execute(
                tool.as_ref(),
                json!({"path": "link.txt", "edits": [{"oldText": "beta", "newText": "BETA"}]}),
            )
            .await
        })
    };
    first.await.unwrap().unwrap();
    second.await.unwrap().unwrap();
    assert_eq!(
        context.env.read_text_file("target.txt", None).await.unwrap(),
        "ALPHA\nBETA\ngamma\n"
    );
}

#[tokio::test]
async fn edit_preserves_bom_and_crlf_and_edits_through_symlinks() {
    let (root, context) = temp_env();
    context
        .env
        .write_file(
            "edit.txt",
            pi_agent_core::harness::types::FileContent::Text("\u{feff}one\r\ntwo\r\n"),
            None,
        )
        .await
        .unwrap();
    execute(
        &EditTool::new(Arc::clone(&context)),
        json!({"path": "edit.txt", "edits": [{"oldText": "two", "newText": "TWO"}]}),
    )
    .await
    .unwrap();
    assert_eq!(
        context.env.read_text_file("edit.txt", None).await.unwrap(),
        "\u{feff}one\r\nTWO\r\n"
    );

    context
        .env
        .write_file(
            "target.txt",
            pi_agent_core::harness::types::FileContent::Text("before\n"),
            None,
        )
        .await
        .unwrap();
    symlink("target.txt", root.path().join("link.txt")).unwrap();
    execute(
        &EditTool::new(Arc::clone(&context)),
        json!({"path": "link.txt", "edits": [{"oldText": "before", "newText": "after"}]}),
    )
    .await
    .unwrap();
    assert_eq!(context.env.read_text_file("target.txt", None).await.unwrap(), "after\n");
}

#[tokio::test]
async fn edit_fuzzy_matches_smart_quotes_and_whitespace() {
    let (_root, context) = temp_env();
    context
        .env
        .write_file(
            "fuzzy.txt",
            pi_agent_core::harness::types::FileContent::Text("keep  \nsay \u{201c}hi\u{201d}\n"),
            None,
        )
        .await
        .unwrap();
    let result = execute(
        &EditTool::new(Arc::clone(&context)),
        json!({"path": "fuzzy.txt", "edits": [{"oldText": "say \"hi\"", "newText": "say \"bye\""}]}),
    )
    .await
    .unwrap();
    assert!(result.details.unwrap()["diff"].as_str().unwrap().contains("bye"));
    assert_eq!(
        context.env.read_text_file("fuzzy.txt", None).await.unwrap(),
        "keep  \nsay \"bye\"\n"
    );
}

#[tokio::test]
async fn edit_prepare_arguments_accepts_json_strings_and_legacy_fields() {
    let (_root, context) = temp_env();
    let tool = EditTool::new(Arc::clone(&context));
    let prepared = tool.prepare_arguments(json!({
        "path": "f.txt",
        "edits": "[{\"oldText\":\"a\",\"newText\":\"A\"}]"
    }));
    assert_eq!(prepared["edits"], json!([{"oldText": "a", "newText": "A"}]));

    let legacy = tool.prepare_arguments(json!({
        "path": "f.txt",
        "edits": [{"oldText": "a", "newText": "A"}],
        "oldText": "b",
        "newText": "B"
    }));
    assert_eq!(
        legacy["edits"],
        json!([
            {"oldText": "a", "newText": "A"},
            {"oldText": "b", "newText": "B"}
        ])
    );
    assert!(legacy.get("oldText").is_none());
    assert!(legacy.get("newText").is_none());

    context
        .env
        .write_file(
            "f.txt",
            pi_agent_core::harness::types::FileContent::Text("a\nb\n"),
            None,
        )
        .await
        .unwrap();
    execute(&tool, legacy).await.unwrap();
    assert_eq!(context.env.read_text_file("f.txt", None).await.unwrap(), "A\nB\n");
}

#[tokio::test]
async fn edit_and_write_report_pre_aborted_operations() {
    let (_root, context) = temp_env();
    context
        .env
        .write_file(
            "file.txt",
            pi_agent_core::harness::types::FileContent::Text("a\n"),
            None,
        )
        .await
        .unwrap();
    let signal = CancellationToken::new();
    signal.cancel();
    let edit = execute_aborted(
        &EditTool::new(Arc::clone(&context)),
        json!({"path": "file.txt", "edits": [{"oldText": "a", "newText": "A"}]}),
        &signal,
    )
    .await
    .unwrap_err();
    assert_eq!(edit.message, "Operation aborted");
    let write = execute_aborted(
        &WriteTool::new(Arc::clone(&context)),
        json!({"path": "other.txt", "content": "x"}),
        &signal,
    )
    .await
    .unwrap_err();
    assert_eq!(write.message, "Operation aborted");
    assert!(!context.env.exists("other.txt", None).await.unwrap());
}

#[tokio::test]
async fn bash_executes_commands_and_combines_stdout_and_stderr() {
    let (_root, context) = temp_env();
    let result = execute(
        &BashTool::new(Arc::clone(&context)),
        json!({"command": "printf out; printf err >&2"}),
    )
    .await
    .unwrap();
    let output = text_output(&result);
    assert!(output.contains("out"));
    assert!(output.contains("err"));
}

#[tokio::test]
async fn bash_reports_nonzero_exits_and_timeouts() {
    let (_root, context) = temp_env();
    let tool = BashTool::new(Arc::clone(&context));
    let error = execute(&tool, json!({"command": "printf failed; exit 7"}))
        .await
        .unwrap_err();
    assert_eq!(error.message, "failed\n\nCommand exited with code 7");
    let error = execute(&tool, json!({"command": "sleep 2", "timeout": 0.01}))
        .await
        .unwrap_err();
    assert_eq!(error.message, "Command timed out after 0.01 seconds");
}

#[tokio::test]
async fn bash_reports_the_total_size_of_an_oversized_final_line() {
    let (_root, context) = temp_env();
    let result = execute(
        &BashTool::new(Arc::clone(&context)),
        json!({"command": "printf '%060000d' 0"}),
    )
    .await
    .unwrap();
    let output = text_output(&result);
    assert!(
        output.contains("Showing last 10.0KB of line 1 (line is 58.6KB). Full output: "),
        "{output}"
    );
    let details = result.details.unwrap();
    assert!(details["fullOutputPath"].as_str().unwrap().ends_with(".log"));
    assert_eq!(details["truncation"]["lastLinePartial"], json!(true));
}

#[tokio::test]
async fn bash_truncates_line_output_and_persists_the_full_output() {
    let (_root, context) = temp_env();
    let mut updates = Vec::new();
    let sink = Arc::new(Mutex::new(Vec::new()));
    let update_sink = Arc::clone(&sink);
    let on_update: AgentToolUpdateCallback = Arc::new(move |result: AgentToolResult| {
        update_sink.lock().unwrap().push(result);
    });
    let result = BashTool::new(Arc::clone(&context))
        .execute(
            "bash".to_owned(),
            json!({"command": "i=1; while [ $i -le 3000 ]; do echo line-$i; i=$((i + 1)); done"}),
            None,
            on_update,
            None,
        )
        .await
        .unwrap();
    updates.extend(sink.lock().unwrap().iter().map(text_output));
    let output = text_output(&result);
    assert!(output.contains("line-3000"));
    let details = result.details.unwrap();
    assert_eq!(details["truncation"]["truncatedBy"], json!("bytes"));
    assert_eq!(details["truncation"]["totalLines"], json!(3000));
    assert_eq!(details["truncation"]["outputLines"], json!(1024));
    let full_output_path = details["fullOutputPath"].as_str().unwrap();
    let full_output = context.env.read_text_file(full_output_path, None).await.unwrap();
    assert!(full_output.contains("line-1\nline-2"));
    assert!(full_output.contains("line-2999\nline-3000"));
    let last = updates.last().expect("final update");
    assert!(last.contains("line-3000"));
    assert!(updates.len() < 25);
}

#[tokio::test]
async fn bash_preserves_truncated_output_when_a_command_times_out() {
    let (_root, context) = temp_env();
    let error = execute(
        &BashTool::new(Arc::clone(&context)),
        json!({
            "command": "i=1; while [ $i -le 3000 ]; do echo line-$i; i=$((i + 1)); done; sleep 5",
            "timeout": 0.5
        }),
    )
    .await
    .unwrap_err();
    assert!(error.message.contains("Command timed out after 0.5 seconds"));
    let start = error.message.find("Full output: ").expect("full output notice") + "Full output: ".len();
    let end = error.message[start..].find(']').expect("notice end") + start;
    let path = &error.message[start..end];
    let full_output = context.env.read_text_file(path, None).await.unwrap();
    assert!(full_output.contains("line-1\nline-2"));
    assert!(full_output.contains("line-3000"));
}

#[tokio::test]
async fn bash_supports_command_prefixes_and_prepare_hooks() {
    let (root, context) = temp_env();
    context
        .env
        .create_dir("workspace", pi_agent_core::harness::types::CreateDirOptions::default())
        .await
        .unwrap();
    let workspace = root.path().join("workspace").to_string_lossy().into_owned();
    let tool = BashTool::with_options(
        Arc::clone(&context),
        BashToolOptions {
            command_prefix: Some("prefix=ready".to_owned()),
            prepare: Some(Arc::new(PrepareWorkspace {
                workspace: workspace.clone(),
            })),
        },
    );
    let result = execute(&tool, json!({"command": ":"})).await.unwrap();
    assert_eq!(
        text_output(&result),
        format!(
            "ready::explicit:{}",
            std::fs::canonicalize(&workspace).unwrap().display()
        )
    );

    let prefixed = execute(
        &BashTool::with_options(
            Arc::clone(&context),
            BashToolOptions {
                command_prefix: Some("value=hello".to_owned()),
                prepare: None,
            },
        ),
        json!({"command": "printf $value"}),
    )
    .await
    .unwrap();
    assert_eq!(text_output(&prefixed), "hello");
}

struct PrepareWorkspace {
    workspace: String,
}

impl BashPrepare for PrepareWorkspace {
    fn prepare<'a>(
        &'a self,
        execution: &'a mut BashExecution,
        _context: &'a ExecutionToolContext,
        _signal: Option<&'a CancellationToken>,
    ) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            execution.cwd = self.workspace.clone();
            execution.env = HashMap::from([("PI_BASH_PREPARE_EXPLICIT".to_owned(), "explicit".to_owned())]);
            execution.inherit_env = false;
            execution.command.push_str(
                "\nprintf '%s:%s:%s:%s' \"$prefix\" \"${PI_BASH_PREPARE_INHERITED-}\" \"$PI_BASH_PREPARE_EXPLICIT\" \"$PWD\"",
            );
            Ok(())
        })
    }
}

#[tokio::test]
async fn bash_reports_aborted_commands() {
    let (_root, context) = temp_env();
    let signal = CancellationToken::new();
    signal.cancel();
    let error = execute_aborted(
        &BashTool::new(Arc::clone(&context)),
        json!({"command": "sleep 5"}),
        &signal,
    )
    .await
    .unwrap_err();
    assert_eq!(error.message, "Command aborted");
}

#[tokio::test]
async fn bash_returns_no_output_placeholder() {
    let (_root, context) = temp_env();
    let result = execute(&BashTool::new(Arc::clone(&context)), json!({"command": ":"}))
        .await
        .unwrap();
    assert_eq!(text_output(&result), "(no output)");
    assert!(result.details.is_none());
}

fn decode_base64(value: &str) -> Vec<u8> {
    let mut output = Vec::new();
    let mut accumulator = 0u32;
    let mut bits = 0u32;
    for byte in value.bytes() {
        if byte == b'=' {
            break;
        }
        let digit = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => continue,
        };
        accumulator = (accumulator << 6) | u32::from(digit);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((accumulator >> bits) as u8);
            accumulator &= (1 << bits) - 1;
        }
    }
    output
}

fn tiny_bmp() -> Vec<u8> {
    let mut bytes = vec![0u8; 58];
    bytes[0] = 0x42;
    bytes[1] = 0x4d;
    bytes[2..6].copy_from_slice(&58u32.to_le_bytes());
    bytes[10..14].copy_from_slice(&54u32.to_le_bytes());
    bytes[14..18].copy_from_slice(&40u32.to_le_bytes());
    bytes[18..22].copy_from_slice(&1i32.to_le_bytes());
    bytes[22..26].copy_from_slice(&1i32.to_le_bytes());
    bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
    bytes[28..30].copy_from_slice(&24u16.to_le_bytes());
    bytes[34..38].copy_from_slice(&4u32.to_le_bytes());
    bytes
}
