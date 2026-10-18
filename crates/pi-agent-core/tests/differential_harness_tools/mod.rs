mod support;

use pi_js_compat::base64::buffer_from_base64;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use pi_agent_core::agent::{AgentTool, AgentToolResult};
use pi_agent_core::harness::env::{NodeExecutionEnv, NodeExecutionEnvOptions};
use pi_agent_core::harness::tools::bash::{BashExecution, BashPrepare, BashTool, BashToolOptions};
use pi_agent_core::harness::tools::edit::EditTool;
use pi_agent_core::harness::tools::read::{ReadTool, ReadToolOptions};
use pi_agent_core::harness::tools::tool_context::ExecutionToolContext;
use pi_agent_core::harness::tools::write::WriteTool;
use pi_agent_core::harness::types::ExecutionEnv;
use pi_ai::request::BoxFuture;
use serde_json::{Map, Value, json};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

use support::TsOracle;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ToolName {
    Read,
    Write,
    Edit,
    Bash,
}

impl ToolName {
    fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Edit => "edit",
            Self::Bash => "bash",
        }
    }
}

struct Case {
    name: &'static str,
    setup: fn(&Path),
    tool: ToolName,
    input: Value,
    abort: bool,
    updates: bool,
    image_processor: bool,
    prepare: bool,
}

pub(crate) fn run() {
    let mut oracle = TsOracle::spawn();
    compare_schemas(&mut oracle);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("differential runtime");
    for case in cases() {
        compare_case(&mut oracle, &runtime, &case);
    }
}

fn compare_schemas(oracle: &mut TsOracle) {
    let response = oracle.request(&json!({"id": 1, "operation": "schemas"}));
    let expected = response["result"].clone();
    let (_root, context) = rust_context();
    let tools: Vec<Arc<dyn AgentTool>> = vec![
        Arc::new(ReadTool::new(Arc::clone(&context))),
        Arc::new(WriteTool::new(Arc::clone(&context))),
        Arc::new(EditTool::new(Arc::clone(&context))),
        Arc::new(BashTool::new(Arc::clone(&context))),
    ];
    let actual = Value::Array(
        tools
            .iter()
            .map(|tool| {
                json!({
                    "name": tool.name(),
                    "label": tool.label(),
                    "description": tool.description(),
                    "parameters": tool.parameters(),
                })
            })
            .collect(),
    );
    assert_eq!(actual, expected, "tool schemas must match TypeBox JSON");
}

fn compare_case(oracle: &mut TsOracle, runtime: &tokio::runtime::Runtime, case: &Case) {
    let ts_root = tempfile::tempdir().expect("TypeScript fixture root");
    let rs_root = tempfile::tempdir().expect("Rust fixture root");
    (case.setup)(ts_root.path());
    (case.setup)(rs_root.path());

    let request = json!({
        "id": 2,
        "operation": "tool",
        "root": ts_root.path().to_string_lossy(),
        "tool": case.tool.as_str(),
        "input": case.input,
        "abort": case.abort,
        "updates": case.updates,
        "imageProcessor": case.image_processor,
        "prepare": case.prepare,
    });
    let response = oracle.request(&request);
    assert_eq!(
        response["ok"],
        json!(true),
        "oracle failed for {}: {response}",
        case.name
    );
    let expected = summary(&response["result"], ts_root.path());
    let actual_result = runtime.block_on(run_tool(case, rs_root.path()));
    let actual = summary(&actual_result, rs_root.path());

    assert_eq!(actual, expected, "tool result mismatch for {}", case.name);
    assert_eq!(
        snapshot(rs_root.path()),
        snapshot(ts_root.path()),
        "fixture tree mismatch for {}",
        case.name
    );
}

fn rust_context() -> (TempDir, Arc<ExecutionToolContext>) {
    let root = tempfile::tempdir().expect("temp dir");
    let env = NodeExecutionEnv::new(NodeExecutionEnvOptions {
        cwd: root.path().to_string_lossy().into_owned(),
        shell_path: None,
        shell_env: HashMap::new(),
    });
    let env: Arc<dyn ExecutionEnv> = Arc::new(env);
    (root, Arc::new(ExecutionToolContext::new(env)))
}

fn context_at(root: &Path) -> Arc<ExecutionToolContext> {
    let env = NodeExecutionEnv::new(NodeExecutionEnvOptions {
        cwd: root.to_string_lossy().into_owned(),
        shell_path: None,
        shell_env: HashMap::new(),
    });
    let env: Arc<dyn ExecutionEnv> = Arc::new(env);
    Arc::new(ExecutionToolContext::new(env))
}

async fn run_tool(case: &Case, root: &Path) -> Value {
    let context = context_at(root);
    let updates = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&updates);
    let on_update: pi_agent_core::agent::AgentToolUpdateCallback = Arc::new(move |result: AgentToolResult| {
        sink.lock()
            .expect("update sink lock")
            .push(serde_json::to_value(&result).expect("tool result must serialize"));
    });
    let signal = CancellationToken::new();
    if case.abort {
        signal.cancel();
    }

    let tool: Arc<dyn AgentTool> = match case.tool {
        ToolName::Read => {
            let options = if case.image_processor {
                ReadToolOptions {
                    auto_resize_images: Some(false),
                    image_processor: Some(Arc::new(ConvertProcessor)),
                }
            } else {
                ReadToolOptions::default()
            };
            Arc::new(ReadTool::with_options(Arc::clone(&context), options))
        }
        ToolName::Write => Arc::new(WriteTool::new(Arc::clone(&context))),
        ToolName::Edit => Arc::new(EditTool::new(Arc::clone(&context))),
        ToolName::Bash => {
            let options = if case.prepare {
                BashToolOptions {
                    command_prefix: Some("prefix=ready".to_owned()),
                    prepare: Some(Arc::new(PrepareWorkspace {
                        workspace: root.join("workspace").to_string_lossy().into_owned(),
                    })),
                }
            } else {
                BashToolOptions::default()
            };
            Arc::new(BashTool::with_options(Arc::clone(&context), options))
        }
    };

    match tool
        .execute("call".to_owned(), case.input.clone(), Some(signal), on_update, None)
        .await
    {
        Ok(result) => json!({
            "ok": true,
            "content": result.content,
            "details": result.details.unwrap_or(Value::Null),
            "updates": updates.lock().expect("update sink lock").clone(),
        }),
        Err(error) => json!({
            "ok": false,
            "error": error.message,
            "updates": updates.lock().expect("update sink lock").clone(),
        }),
    }
}

struct ConvertProcessor;

impl pi_agent_core::harness::tools::read::ReadImageProcessor for ConvertProcessor {
    fn process<'a>(
        &'a self,
        _bytes: &'a [u8],
        _mime_type: &'a str,
        _options: pi_agent_core::harness::tools::read::ImageProcessorOptions,
    ) -> BoxFuture<'a, pi_agent_core::harness::tools::read::ReadImageProcessorResult> {
        Box::pin(async {
            pi_agent_core::harness::tools::read::ReadImageProcessorResult::Ok {
                data: "converted".to_owned(),
                mime_type: "image/png".to_owned(),
                hints: vec!["[Image converted from image/bmp to image/png.]".to_owned()],
            }
        })
    }
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

fn summary(result: &Value, root: &Path) -> Value {
    let full_output_path = full_output_path(result);
    let last_update = result
        .get("updates")
        .and_then(Value::as_array)
        .and_then(|updates| updates.last())
        .cloned()
        .unwrap_or(Value::Null);
    let mut value = json!({
        "ok": result.get("ok").cloned().unwrap_or(Value::Null),
        "content": result.get("content").cloned().unwrap_or(Value::Null),
        "details": result.get("details").cloned().unwrap_or(Value::Null),
        "error": result.get("error").cloned().unwrap_or(Value::Null),
        "lastUpdate": last_update,
    });
    let root = root.to_string_lossy().into_owned();
    let canonical_root = std::fs::canonicalize(root.as_str())
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| root.clone());
    normalize(&mut value, &root, &canonical_root, full_output_path.as_deref());
    value
}

fn full_output_path(result: &Value) -> Option<String> {
    if let Some(path) = result.pointer("/details/fullOutputPath").and_then(Value::as_str) {
        return Some(path.to_owned());
    }
    result
        .get("updates")
        .and_then(Value::as_array)
        .and_then(|updates| updates.last())
        .and_then(|update| update.pointer("/details/fullOutputPath"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn normalize(value: &mut Value, root: &str, canonical_root: &str, full_output_path: Option<&str>) {
    match value {
        Value::String(text) => {
            let mut normalized = text.clone();
            if let Some(path) = full_output_path {
                normalized = normalized.replace(path, "<full-output>");
            }
            normalized = normalized.replace(root, "<root>");
            normalized = normalized.replace(canonical_root, "<root>");
            *text = normalized;
        }
        Value::Array(values) => {
            for value in values {
                normalize(value, root, canonical_root, full_output_path);
            }
        }
        Value::Object(map) => {
            for value in map.values_mut() {
                normalize(value, root, canonical_root, full_output_path);
            }
        }
        _ => {}
    }
}

fn snapshot(root: &Path) -> Value {
    let mut entries = Map::new();
    collect_snapshot(root, root, &mut entries);
    Value::Object(entries)
}

fn collect_snapshot(root: &Path, directory: &Path, entries: &mut Map<String, Value>) {
    let mut children = std::fs::read_dir(directory)
        .expect("fixture directory must be readable")
        .collect::<Result<Vec<_>, _>>()
        .expect("fixture entries must be readable");
    children.sort_by_key(|entry| entry.file_name());
    for child in children {
        let path = child.path();
        let relative = path
            .strip_prefix(root)
            .expect("fixture path must be under root")
            .to_string_lossy()
            .into_owned();
        let metadata = std::fs::symlink_metadata(&path).expect("fixture metadata must be readable");
        if metadata.file_type().is_symlink() {
            let target = std::fs::read_link(&path).expect("symlink target must be readable");
            entries.insert(relative, json!({"kind": "symlink", "target": target.to_string_lossy()}));
        } else if metadata.is_dir() {
            entries.insert(relative, json!({"kind": "directory"}));
            collect_snapshot(root, &path, entries);
        } else {
            let bytes = std::fs::read(&path).expect("fixture file must be readable");
            entries.insert(
                relative,
                json!({"kind": "file", "content": String::from_utf8_lossy(&bytes)}),
            );
        }
    }
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "read-offset-limit",
            setup: setup_read_text,
            tool: ToolName::Read,
            input: json!({"path": "test.txt", "offset": 41, "limit": 20}),
            abort: false,
            updates: false,
            image_processor: false,
            prepare: false,
        },
        Case {
            name: "read-large-truncation",
            setup: setup_read_large,
            tool: ToolName::Read,
            input: json!({"path": "large.txt"}),
            abort: false,
            updates: false,
            image_processor: false,
            prepare: false,
        },
        Case {
            name: "read-exact-line-limit",
            setup: setup_read_exact,
            tool: ToolName::Read,
            input: json!({"path": "exact.txt"}),
            abort: false,
            updates: false,
            image_processor: false,
            prepare: false,
        },
        Case {
            name: "read-missing-file",
            setup: setup_none,
            tool: ToolName::Read,
            input: json!({"path": "missing.txt"}),
            abort: false,
            updates: false,
            image_processor: false,
            prepare: false,
        },
        Case {
            name: "read-offset-beyond-file",
            setup: setup_read_short,
            tool: ToolName::Read,
            input: json!({"path": "short.txt", "offset": 100}),
            abort: false,
            updates: false,
            image_processor: false,
            prepare: false,
        },
        Case {
            name: "read-png-image",
            setup: setup_png,
            tool: ToolName::Read,
            input: json!({"path": "image.txt"}),
            abort: false,
            updates: false,
            image_processor: false,
            prepare: false,
        },
        Case {
            name: "read-bmp-without-processor",
            setup: setup_bmp,
            tool: ToolName::Read,
            input: json!({"path": "image.bmp"}),
            abort: false,
            updates: false,
            image_processor: false,
            prepare: false,
        },
        Case {
            name: "read-bmp-with-processor",
            setup: setup_bmp,
            tool: ToolName::Read,
            input: json!({"path": "image.bmp"}),
            abort: false,
            updates: false,
            image_processor: true,
            prepare: false,
        },
        Case {
            name: "read-aborted",
            setup: setup_read_short,
            tool: ToolName::Read,
            input: json!({"path": "short.txt"}),
            abort: true,
            updates: false,
            image_processor: false,
            prepare: false,
        },
        Case {
            name: "write-nested",
            setup: setup_none,
            tool: ToolName::Write,
            input: json!({"path": "nested/dir/file.txt", "content": "hello"}),
            abort: false,
            updates: false,
            image_processor: false,
            prepare: false,
        },
        Case {
            name: "write-utf16-length",
            setup: setup_none,
            tool: ToolName::Write,
            input: json!({"path": "emoji.txt", "content": "\u{1f600}"}),
            abort: false,
            updates: false,
            image_processor: false,
            prepare: false,
        },
        Case {
            name: "write-aborted",
            setup: setup_none,
            tool: ToolName::Write,
            input: json!({"path": "other.txt", "content": "x"}),
            abort: true,
            updates: false,
            image_processor: false,
            prepare: false,
        },
        Case {
            name: "edit-disjoint",
            setup: setup_edit,
            tool: ToolName::Edit,
            input: json!({
                "path": "edit.txt",
                "edits": [
                    {"oldText": "alpha\n", "newText": "ALPHA\n"},
                    {"oldText": "gamma\n", "newText": "GAMMA\n"}
                ]
            }),
            abort: false,
            updates: false,
            image_processor: false,
            prepare: false,
        },
        Case {
            name: "edit-overlap",
            setup: setup_edit_overlap,
            tool: ToolName::Edit,
            input: json!({
                "path": "edit.txt",
                "edits": [
                    {"oldText": "one\ntwo\n", "newText": "ONE\nTWO\n"},
                    {"oldText": "two\nthree\n", "newText": "TWO\nTHREE\n"}
                ]
            }),
            abort: false,
            updates: false,
            image_processor: false,
            prepare: false,
        },
        Case {
            name: "edit-missing",
            setup: setup_edit_dupes,
            tool: ToolName::Edit,
            input: json!({"path": "dupes.txt", "edits": [{"oldText": "bar", "newText": "baz"}]}),
            abort: false,
            updates: false,
            image_processor: false,
            prepare: false,
        },
        Case {
            name: "edit-duplicate",
            setup: setup_edit_dupes,
            tool: ToolName::Edit,
            input: json!({"path": "dupes.txt", "edits": [{"oldText": "foo", "newText": "bar"}]}),
            abort: false,
            updates: false,
            image_processor: false,
            prepare: false,
        },
        Case {
            name: "edit-fuzzy",
            setup: setup_edit_fuzzy,
            tool: ToolName::Edit,
            input: json!({"path": "fuzzy.txt", "edits": [{"oldText": "say \"hi\"", "newText": "say \"bye\""}]}),
            abort: false,
            updates: false,
            image_processor: false,
            prepare: false,
        },
        Case {
            name: "edit-bom-crlf",
            setup: setup_edit_bom,
            tool: ToolName::Edit,
            input: json!({"path": "edit.txt", "edits": [{"oldText": "two", "newText": "TWO"}]}),
            abort: false,
            updates: false,
            image_processor: false,
            prepare: false,
        },
        Case {
            name: "edit-symlink",
            setup: setup_edit_symlink,
            tool: ToolName::Edit,
            input: json!({"path": "link.txt", "edits": [{"oldText": "before", "newText": "after"}]}),
            abort: false,
            updates: false,
            image_processor: false,
            prepare: false,
        },
        Case {
            name: "edit-no-change",
            setup: setup_edit,
            tool: ToolName::Edit,
            input: json!({"path": "edit.txt", "edits": [{"oldText": "alpha", "newText": "alpha"}]}),
            abort: false,
            updates: false,
            image_processor: false,
            prepare: false,
        },
        Case {
            name: "edit-empty-old-text",
            setup: setup_edit,
            tool: ToolName::Edit,
            input: json!({"path": "edit.txt", "edits": [{"oldText": "", "newText": "x"}]}),
            abort: false,
            updates: false,
            image_processor: false,
            prepare: false,
        },
        Case {
            name: "edit-multi-missing",
            setup: setup_edit,
            tool: ToolName::Edit,
            input: json!({
                "path": "edit.txt",
                "edits": [
                    {"oldText": "alpha", "newText": "ALPHA"},
                    {"oldText": "missing", "newText": "x"}
                ]
            }),
            abort: false,
            updates: false,
            image_processor: false,
            prepare: false,
        },
        Case {
            name: "edit-aborted",
            setup: setup_edit,
            tool: ToolName::Edit,
            input: json!({"path": "edit.txt", "edits": [{"oldText": "alpha", "newText": "ALPHA"}]}),
            abort: true,
            updates: false,
            image_processor: false,
            prepare: false,
        },
        Case {
            name: "bash-stdout",
            setup: setup_none,
            tool: ToolName::Bash,
            input: json!({"command": "printf out"}),
            abort: false,
            updates: true,
            image_processor: false,
            prepare: false,
        },
        Case {
            name: "bash-nonzero-exit",
            setup: setup_none,
            tool: ToolName::Bash,
            input: json!({"command": "printf failed; exit 7"}),
            abort: false,
            updates: true,
            image_processor: false,
            prepare: false,
        },
        Case {
            name: "bash-timeout",
            setup: setup_none,
            tool: ToolName::Bash,
            input: json!({"command": "sleep 5", "timeout": 0.05}),
            abort: false,
            updates: true,
            image_processor: false,
            prepare: false,
        },
        Case {
            name: "bash-oversized-final-line",
            setup: setup_none,
            tool: ToolName::Bash,
            input: json!({"command": "printf '%060000d' 0"}),
            abort: false,
            updates: true,
            image_processor: false,
            prepare: false,
        },
        Case {
            name: "bash-truncated-lines",
            setup: setup_none,
            tool: ToolName::Bash,
            input: json!({"command": "i=1; while [ $i -le 3000 ]; do echo line-$i; i=$((i + 1)); done"}),
            abort: false,
            updates: true,
            image_processor: false,
            prepare: false,
        },
        Case {
            name: "bash-aborted",
            setup: setup_none,
            tool: ToolName::Bash,
            input: json!({"command": "sleep 5"}),
            abort: true,
            updates: true,
            image_processor: false,
            prepare: false,
        },
        Case {
            name: "bash-prepare",
            setup: setup_workspace,
            tool: ToolName::Bash,
            input: json!({"command": ":"}),
            abort: false,
            updates: true,
            image_processor: false,
            prepare: true,
        },
    ]
}

fn setup_none(_root: &Path) {}

fn setup_read_text(root: &Path) {
    let lines = (1..=100)
        .map(|line| format!("Line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(root.join("test.txt"), lines).expect("fixture write");
}

fn setup_read_large(root: &Path) {
    let lines = (1..=2500)
        .map(|line| format!("Line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(root.join("large.txt"), lines).expect("fixture write");
}

fn setup_read_exact(root: &Path) {
    let content = (0..2000).map(|_| "x").collect::<Vec<_>>().join("\n") + "\n";
    std::fs::write(root.join("exact.txt"), content).expect("fixture write");
}

fn setup_read_short(root: &Path) {
    std::fs::write(root.join("short.txt"), "one\ntwo\nthree").expect("fixture write");
}

fn setup_png(root: &Path) {
    let bytes = buffer_from_base64(
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNgYGD4DwABBAEAX+XDSwAAAABJRU5ErkJggg==",
    );
    std::fs::write(root.join("image.txt"), bytes).expect("fixture write");
}

fn setup_bmp(root: &Path) {
    std::fs::write(root.join("image.bmp"), tiny_bmp()).expect("fixture write");
}

fn setup_edit(root: &Path) {
    std::fs::write(root.join("edit.txt"), "alpha\nbeta\ngamma\ndelta\n").expect("fixture write");
}

fn setup_edit_overlap(root: &Path) {
    std::fs::write(root.join("edit.txt"), "one\ntwo\nthree\n").expect("fixture write");
}

fn setup_edit_dupes(root: &Path) {
    std::fs::write(root.join("dupes.txt"), "foo foo foo").expect("fixture write");
}

fn setup_edit_fuzzy(root: &Path) {
    std::fs::write(root.join("fuzzy.txt"), "keep  \nsay \u{201c}hi\u{201d}\n").expect("fixture write");
}

fn setup_edit_bom(root: &Path) {
    std::fs::write(root.join("edit.txt"), "\u{feff}one\r\ntwo\r\n").expect("fixture write");
}

fn setup_edit_symlink(root: &Path) {
    std::fs::write(root.join("target.txt"), "before\n").expect("fixture write");
    std::os::unix::fs::symlink("target.txt", root.join("link.txt")).expect("fixture symlink");
}

fn setup_workspace(root: &Path) {
    std::fs::create_dir(root.join("workspace")).expect("fixture directory");
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
