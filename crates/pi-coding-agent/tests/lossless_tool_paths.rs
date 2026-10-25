use crate::shared_ai_oracle as oracle;

use std::io;
use std::sync::{Arc, Mutex};

use pi_agent_core::agent::AgentTool;
use pi_ai::request::BoxFuture;
use pi_ai::types::TextOrImageContent;
use pi_coding_agent::core::tools::render_utils::{normalize_display_text, replace_tabs, str_value};
use pi_coding_agent::core::tools::write::{WriteOperations, WriteTool, WriteToolOptions};
use pi_js_compat::js_json::{JsString, JsValue, parse_lossless};

#[test]
fn native_operation_error_survives_the_io_carrier() {
    use pi_coding_agent::core::tools::node_fs::{operation_error, operation_error_message};
    let message = JsString::from("operation-") + JsString::from_utf16(vec![0xd800]);
    assert_eq!(operation_error_message(&operation_error(message.clone())), message);
}

#[test]
fn native_tool_renderers_match_frozen_raw_path_and_error_lines() {
    use pi_coding_agent::core::extensions::types::{ToolRenderContext, ToolRenderResultOptions, ToolRenderState};
    use pi_coding_agent::core::tools::{edit, find, grep, ls, read, write};
    use pi_coding_agent::modes::interactive::theme::{ColorMode, load_theme_from_path, set_theme_instance};
    let theme_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/pi-coding-agent/theme/dark.json");
    let mut capabilities = pi_tui::get_capabilities();
    capabilities.hyperlinks = true;
    pi_tui::set_capabilities(capabilities);
    let theme = load_theme_from_path(
        &JsString::from(theme_path.to_str().expect("utf-8 test path")),
        Some(ColorMode::Truecolor),
    )
    .unwrap();
    set_theme_instance(Arc::new(theme.clone()));
    let params = parse_lossless(
        r#"{"path":"relative-\ud800.txt","pattern":"pat-\udfff","content":"line-\ud801\t\r\nsecond-\udfff"}"#,
    )
    .unwrap();
    let cwd = JsString::from("/cwd-") + JsString::from_utf16(vec![0xd800]);
    let result = pi_agent_core::agent::AgentToolResult {
        content: vec![TextOrImageContent::Text(pi_ai::types::TextContent::new(
            JsString::from("error-")
                + JsString::from_utf16(vec![0xdfff])
                + "\r\nnext-"
                + JsString::from_utf16(vec![0xd801]),
        ))],
        details: None,
        is_error: Some(true),
        terminate: None,
    };
    let options = ToolRenderResultOptions {
        expanded: true,
        is_partial: false,
    };
    for (name, factory) in [
        ("write", "createWriteToolDefinition"),
        ("read", "createReadToolDefinition"),
        ("edit", "createEditToolDefinition"),
        ("ls", "createLsToolDefinition"),
        ("find", "createFindToolDefinition"),
        ("grep", "createGrepToolDefinition"),
    ] {
        let mut context = ToolRenderContext {
            args: params.clone(),
            tool_call_id: "raw-render".into(),
            invalidate: Arc::new(|| {}),
            last_component: None,
            call_component: None,
            state: ToolRenderState::new(),
            cwd: cwd.clone(),
            execution_started: false,
            args_complete: false,
            is_partial: false,
            expanded: true,
            show_images: false,
            is_error: true,
        };
        let call = match name {
            "write" => write::render_call(&params, &theme, &context).expect("fixture path renders"),
            "read" => read::render_call(&params, &theme, &context).expect("fixture path renders"),
            "edit" => edit::render_call(&params, &theme, &context).unwrap(),
            "ls" => ls::render_call(&params, &theme, &context).expect("fixture path renders"),
            "find" => find::render_call(&params, &theme, &context).expect("fixture path renders"),
            "grep" => grep::render_call(&params, &theme, &context).expect("fixture path renders"),
            _ => unreachable!(),
        };
        // ToolExecutionComponent carries the call component into result rendering.
        context.call_component = Some(call.clone());
        let output = match name {
            "write" => write::render_result(&result, &options, &theme, &context).expect("fixture result renders"),
            "read" => read::render_result(&result, &options, &theme, &context).expect("fixture language renders"),
            "edit" => edit::render_result(&result, &options, &theme, &context).unwrap(),
            "ls" => ls::render_result(&result, &options, &theme, &context).expect("fixture result renders"),
            "find" => find::render_result(&result, &options, &theme, &context).expect("fixture result renders"),
            "grep" => grep::render_result(&result, &options, &theme, &context).expect("fixture result renders"),
            _ => unreachable!(),
        };
        let actual =
            pi_js_compat::js_json!({"call":call.borrow_mut().render(100),"result":output.borrow_mut().render(100)});
        let module = format!("packages/coding-agent/src/core/tools/{name}.ts");
        let mut source = oracle::TsOracle::spawn("lossless-tool-paths-oracle.ts", &module);
        let expected=source.request(&pi_js_compat::js_json!({"kind":"render","factory":factory,"cwd":cwd,"params":params,"result":result,"themePath":theme_path.to_str().unwrap(),"width":100}));
        assert!(
            actual == expected,
            "{name}\nactual: {}\nexpected: {}",
            pi_js_compat::js_json::stringify_lossless(&actual),
            pi_js_compat::js_json::stringify_lossless(&expected)
        );
    }
    pi_tui::reset_capabilities_cache();
}

#[test]
fn custom_shell_missing_path_retains_original_units_in_error() {
    use pi_coding_agent::utils::shell::get_shell_config;
    let directory = tempfile::tempdir().unwrap();
    let path = JsString::from(directory.path().to_str().unwrap()) + "/shell-" + JsString::from_utf16(vec![0xd800]);
    let error = get_shell_config(Some(&path)).unwrap_err();
    let mut source = oracle::TsOracle::spawn(
        "lossless-tool-paths-oracle.ts",
        "packages/coding-agent/src/utils/shell.ts",
    );
    let expected = source.request(&pi_js_compat::js_json!({"kind":"shellMissing","path":path}));
    assert_eq!(error.message(), expected["error"].as_string().unwrap());
}

#[test]
fn ansi_and_binary_cleanup_keep_units_outside_removed_ranges() {
    use pi_coding_agent::utils::{ansi::strip_ansi, shell::sanitize_binary_output};
    let raw = JsString::from_utf16(vec![0xd800, 0xdfff, 0xd801, 0xfffd]);
    let decorated = JsString::from("\x1b]0;")
        + JsString::from_utf16(vec![0xd802])
        + "\x07\x1b[31m"
        + &raw
        + "\x1b[0m\0\t\n\r\u{fff9}";
    let actual = sanitize_binary_output(&strip_ansi(&decorated));
    let mut source = oracle::TsOracle::spawn(
        "lossless-tool-paths-oracle.ts",
        "packages/coding-agent/src/utils/ansi.ts",
    );
    let expected = source.request(&serde_json::json!({"kind":"cleanup","units":decorated.units()}));
    assert_eq!(serde_json::json!(actual.units()), expected["units"]);
    assert_eq!(actual, raw + "\t\n\r");
}

#[test]
fn display_helpers_keep_distinct_surrogates() {
    let raw = JsString::from_utf16(vec![0xd800, 9, 0xdfff, 13, 0xd83d, 0xde00]);
    let value = JsValue::String(raw.clone());
    assert_eq!(str_value(Some(&value)), Some(raw.clone()));
    let mut source = oracle::TsOracle::spawn(
        "lossless-tool-paths-oracle.ts",
        "packages/coding-agent/src/core/tools/render-utils.ts",
    );
    let expected = source.request(&serde_json::json!({"kind":"display", "units":raw.units()}));
    assert_eq!(
        serde_json::json!(normalize_display_text(&replace_tabs(&raw)).units()),
        expected["units"]
    );
    assert_eq!(
        normalize_display_text(&replace_tabs(&raw)).units(),
        &[0xd800, 32, 32, 32, 0xdfff, 0xd83d, 0xde00]
    );
}

#[tokio::test]
async fn local_write_error_matches_node_original_path_units() {
    use pi_coding_agent::core::tools::node_fs::operation_error_message;
    use pi_coding_agent::core::tools::write::LocalWriteOperations;
    let directory = tempfile::tempdir().unwrap();
    let path =
        JsString::from(directory.path().to_str().unwrap()) + "/missing/file-" + JsString::from_utf16(vec![0xd800]);
    let error = LocalWriteOperations
        .write_file(&path, &JsString::from("x"))
        .await
        .unwrap_err();
    let mut source = oracle::TsOracle::spawn(
        "lossless-tool-paths-oracle.ts",
        "packages/coding-agent/src/core/tools/write.ts",
    );
    let expected = source.request(&pi_js_compat::js_json!({"kind":"localWriteError", "path":path}));
    assert_eq!(
        operation_error_message(&error),
        expected["error"].as_string().unwrap().clone()
    );
}

#[derive(Default)]
struct MissingGrep {
    paths: Mutex<Vec<JsString>>,
}
impl pi_coding_agent::core::tools::grep::GrepOperations for MissingGrep {
    fn is_directory<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, io::Result<bool>> {
        Box::pin(async move {
            self.paths.lock().unwrap().push(path.clone());
            Err(io::Error::other("missing"))
        })
    }
    fn read_file<'a>(&'a self, _: &'a JsString) -> BoxFuture<'a, io::Result<JsString>> {
        Box::pin(async { panic!("missing path must not read") })
    }
}

#[tokio::test]
async fn grep_missing_path_keeps_original_units_before_process_boundary() {
    use pi_coding_agent::core::tools::grep::{GrepTool, GrepToolOptions};
    let cwd = JsString::from("/cwd-") + JsString::from_utf16(vec![0xd800]);
    let params = parse_lossless(r#"{"path":"sub-\udfff","pattern":"x"}"#).unwrap();
    let operations = Arc::new(MissingGrep::default());
    let tool = GrepTool::with_options(
        &cwd,
        GrepToolOptions {
            operations: Some(operations.clone()),
        },
    );
    let error = tool
        .execute("grep".into(), params.clone(), None, Arc::new(|_| Ok(())), None)
        .await
        .unwrap_err();
    let mut source = oracle::TsOracle::spawn(
        "lossless-tool-paths-oracle.ts",
        "packages/coding-agent/src/core/tools/grep.ts",
    );
    let expected = source.request(&pi_js_compat::js_json!({"kind":"grepMissing","cwd":cwd,"params":params}));
    assert_eq!(&error.message, expected["error"].as_string().unwrap());
    assert_eq!(
        JsValue::Array(
            operations
                .paths
                .lock()
                .unwrap()
                .iter()
                .cloned()
                .map(JsValue::String)
                .collect()
        ),
        expected["paths"]
    );
}

#[derive(Default)]
struct RecordingFind {
    calls: Mutex<Vec<JsValue>>,
}
impl pi_coding_agent::core::tools::find::FindOperations for RecordingFind {
    fn exists<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, io::Result<bool>> {
        Box::pin(async move {
            self.calls
                .lock()
                .unwrap()
                .push(pi_js_compat::js_json!(["exists", path]));
            Ok(true)
        })
    }
    fn supports_glob(&self) -> bool {
        true
    }
    fn glob<'a>(
        &'a self,
        pattern: &'a JsString,
        cwd: &'a JsString,
        ignore: &'a [JsString],
        limit: f64,
    ) -> BoxFuture<'a, io::Result<Vec<JsString>>> {
        Box::pin(async move {
            self.calls
                .lock()
                .unwrap()
                .push(pi_js_compat::js_json!(["glob", pattern, cwd, ignore, limit]));
            Ok(vec![cwd.clone() + "/file-" + JsString::from_utf16(vec![0xdfff])])
        })
    }
}

#[tokio::test]
async fn custom_find_receives_raw_search_path_pattern_and_results() {
    use pi_coding_agent::core::tools::find::{FindTool, FindToolOptions};
    let cwd = JsString::from("/cwd-") + JsString::from_utf16(vec![0xd800]);
    let params = parse_lossless(r#"{"path":"sub-\ud801","pattern":"*\udfff"}"#).unwrap();
    let operations = Arc::new(RecordingFind::default());
    let tool = FindTool::with_options(
        &cwd,
        FindToolOptions {
            operations: Some(operations.clone()),
        },
    );
    let actual = tool
        .execute("find".into(), params.clone(), None, Arc::new(|_| Ok(())), None)
        .await
        .unwrap();
    let mut source = oracle::TsOracle::spawn(
        "lossless-tool-paths-oracle.ts",
        "packages/coding-agent/src/core/tools/find.ts",
    );
    let expected = source.request(&pi_js_compat::js_json!({"kind":"find","cwd":cwd,"params":params}));
    assert_eq!(
        JsValue::Array(operations.calls.lock().unwrap().clone()),
        expected["calls"]
    );
    let TextOrImageContent::Text(text) = &actual.content[0] else {
        panic!("text")
    };
    assert_eq!(
        &text.text,
        expected["result"]["content"].as_array().unwrap()[0]["text"]
            .as_string()
            .unwrap()
    );
}

#[derive(Default)]
struct RecordingEdit {
    calls: Mutex<Vec<JsValue>>,
}
impl pi_coding_agent::core::tools::edit::EditOperations for RecordingEdit {
    fn access<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, io::Result<()>> {
        Box::pin(async move {
            self.calls
                .lock()
                .unwrap()
                .push(pi_js_compat::js_json!(["access", path]));
            Ok(())
        })
    }
    fn read_file<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, io::Result<Vec<u8>>> {
        Box::pin(async move {
            self.calls.lock().unwrap().push(pi_js_compat::js_json!(["read", path]));
            Ok("😀".as_bytes().to_vec())
        })
    }
    fn write_file<'a>(&'a self, path: &'a JsString, content: &'a JsString) -> BoxFuture<'a, io::Result<()>> {
        Box::pin(async move {
            self.calls
                .lock()
                .unwrap()
                .push(pi_js_compat::js_json!(["write", path, content]));
            Ok(())
        })
    }
}

#[tokio::test]
async fn custom_edit_receives_raw_path_and_inside_pair_replacement_content() {
    use pi_coding_agent::core::tools::edit::{EditTool, EditToolOptions};
    let directory = tempfile::tempdir().unwrap();
    let cwd = JsString::from(directory.path().to_str().unwrap()) + "/cwd-" + JsString::from_utf16(vec![0xd800]);
    let params = parse_lossless(r#"{"path":"file-\udfff","edits":[{"oldText":"\ude00","newText":"x"}]}"#).unwrap();
    let operations = Arc::new(RecordingEdit::default());
    let tool = EditTool::with_options(
        &cwd,
        EditToolOptions {
            operations: Some(operations.clone()),
        },
    );
    let actual = tool
        .execute("edit".into(), params.clone(), None, Arc::new(|_| Ok(())), None)
        .await
        .unwrap();
    let mut source = oracle::TsOracle::spawn(
        "lossless-tool-paths-oracle.ts",
        "packages/coding-agent/src/core/tools/edit.ts",
    );
    let expected = source.request(&pi_js_compat::js_json!({"kind":"edit","cwd":cwd,"params":params}));
    assert_eq!(
        JsValue::Array(operations.calls.lock().unwrap().clone()),
        expected["calls"]
    );
    let TextOrImageContent::Text(text) = &actual.content[0] else {
        panic!("text")
    };
    assert_eq!(
        &text.text,
        expected["result"]["content"].as_array().unwrap()[0]["text"]
            .as_string()
            .unwrap()
    );
    assert_eq!(actual.details.as_ref().unwrap(), &expected["result"]["details"]);
}

struct RecordingRead {
    calls: Mutex<Vec<JsValue>>,
    bytes: Vec<u8>,
}

impl pi_coding_agent::core::tools::read::ReadOperations for RecordingRead {
    fn access<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, io::Result<()>> {
        Box::pin(async move {
            self.calls
                .lock()
                .unwrap()
                .push(pi_js_compat::js_json!(["access", path]));
            Ok(())
        })
    }
    fn read_file<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, io::Result<Vec<u8>>> {
        Box::pin(async move {
            self.calls.lock().unwrap().push(pi_js_compat::js_json!(["read", path]));
            Ok(self.bytes.clone())
        })
    }
}

#[tokio::test]
async fn custom_read_retains_path_in_operations_and_first_line_notice() {
    use pi_coding_agent::core::tools::read::{ReadTool, ReadToolOptions};
    let directory = tempfile::tempdir().unwrap();
    let cwd = JsString::from(directory.path().to_str().unwrap()) + "/cwd-" + JsString::from_utf16(vec![0xd800]);
    let params = parse_lossless(r#"{"path":"file-\udfff"}"#).unwrap();
    let operations = Arc::new(RecordingRead {
        calls: Mutex::new(Vec::new()),
        bytes: vec![b'x'; 10241],
    });
    let tool = ReadTool::with_options(
        &cwd,
        ReadToolOptions {
            operations: Some(operations.clone()),
            auto_resize_images: None,
        },
    );
    let actual = tool
        .execute("read".into(), params.clone(), None, Arc::new(|_| Ok(())), None)
        .await
        .unwrap();
    let mut source = oracle::TsOracle::spawn(
        "lossless-tool-paths-oracle.ts",
        "packages/coding-agent/src/core/tools/read.ts",
    );
    let expected = source.request(&pi_js_compat::js_json!({"kind":"read","cwd":cwd,"params":params}));
    assert_eq!(
        JsValue::Array(operations.calls.lock().unwrap().clone()),
        expected["calls"]
    );
    let TextOrImageContent::Text(text) = &actual.content[0] else {
        panic!("text")
    };
    assert_eq!(
        &text.text,
        expected["result"]["content"].as_array().unwrap()[0]["text"]
            .as_string()
            .unwrap()
    );
    assert_eq!(actual.details.as_ref().unwrap(), &expected["result"]["details"]);
}

struct RecordingLs {
    calls: Mutex<Vec<JsValue>>,
    entries: Vec<JsString>,
    exists: bool,
}

impl pi_coding_agent::core::tools::ls::LsOperations for RecordingLs {
    fn exists<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, io::Result<bool>> {
        Box::pin(async move {
            self.calls
                .lock()
                .unwrap()
                .push(pi_js_compat::js_json!(["exists", path]));
            Ok(self.exists)
        })
    }
    fn is_directory<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, io::Result<bool>> {
        Box::pin(async move {
            self.calls
                .lock()
                .unwrap()
                .push(pi_js_compat::js_json!(["isDirectory", path]));
            Ok(!path.ends_with(JsString::from("file-") + JsString::from_utf16(vec![0xdfff])))
        })
    }
    fn readdir<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, io::Result<Vec<JsString>>> {
        Box::pin(async move {
            self.calls
                .lock()
                .unwrap()
                .push(pi_js_compat::js_json!(["readdir", path]));
            Ok(self.entries.clone())
        })
    }
}

#[tokio::test]
async fn custom_ls_retains_paths_entries_and_missing_path_error() {
    use pi_coding_agent::core::tools::ls::{LsTool, LsToolOptions};
    let cwd = JsString::from("/work-") + JsString::from_utf16(vec![0xd800]);
    let entries = vec![
        JsString::from("file-") + JsString::from_utf16(vec![0xdfff]),
        JsString::from("dir-") + JsString::from_utf16(vec![0xd801]),
    ];
    let params = parse_lossless(r#"{"path":"sub-\ud802"}"#).unwrap();
    let mut source = oracle::TsOracle::spawn(
        "lossless-tool-paths-oracle.ts",
        "packages/coding-agent/src/core/tools/ls.ts",
    );
    for exists in [true, false] {
        let operations = Arc::new(RecordingLs {
            calls: Mutex::new(Vec::new()),
            entries: entries.clone(),
            exists,
        });
        let tool = LsTool::with_options(
            &cwd,
            LsToolOptions {
                operations: Some(operations.clone()),
            },
        );
        let actual = tool
            .execute("ls".into(), params.clone(), None, Arc::new(|_| Ok(())), None)
            .await;
        let expected = source.request(
            &pi_js_compat::js_json!({"kind":"ls", "cwd":cwd, "params":params, "entries":entries, "exists":exists}),
        );
        assert_eq!(
            JsValue::Array(operations.calls.lock().unwrap().clone()),
            expected["calls"]
        );
        match actual {
            Ok(result) => {
                let TextOrImageContent::Text(text) = &result.content[0] else {
                    panic!("text")
                };
                assert_eq!(
                    &text.text,
                    expected["result"]["content"].as_array().unwrap()[0]["text"]
                        .as_string()
                        .unwrap()
                );
            }
            Err(error) => assert_eq!(&error.message, expected["error"].as_string().unwrap()),
        }
    }
}

#[derive(Default)]
struct RecordingWrite {
    calls: Mutex<Vec<(JsString, Option<JsString>)>>,
}

impl WriteOperations for RecordingWrite {
    fn write_file<'a>(&'a self, path: &'a JsString, content: &'a JsString) -> BoxFuture<'a, io::Result<()>> {
        Box::pin(async move {
            self.calls.lock().unwrap().push((path.clone(), Some(content.clone())));
            Ok(())
        })
    }

    fn mkdir<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, io::Result<()>> {
        Box::pin(async move {
            self.calls.lock().unwrap().push((path.clone(), None));
            Ok(())
        })
    }
}

#[tokio::test]
async fn custom_write_receives_original_path_cwd_and_content_units() {
    let directory = tempfile::tempdir().unwrap();
    let cwd = JsString::from(directory.path().to_str().unwrap()) + "/cwd-" + JsString::from_utf16(vec![0xd800]);
    let operations = Arc::new(RecordingWrite::default());
    let tool = WriteTool::with_options(
        &cwd,
        WriteToolOptions {
            operations: Some(operations.clone()),
        },
    );
    let params = parse_lossless(r#"{"path":"sub-\udfff/file-\ud801","content":"\ud802\udfff"}"#).unwrap();
    let result = tool
        .execute("write".into(), params.clone(), None, Arc::new(|_| Ok(())), None)
        .await
        .unwrap();
    let mut source = oracle::TsOracle::spawn(
        "lossless-tool-paths-oracle.ts",
        "packages/coding-agent/src/core/tools/write.ts",
    );
    let expected = source.request(&pi_js_compat::js_json!({"kind":"write", "cwd":cwd, "params":params}));
    let calls = operations.calls.lock().unwrap();
    let actual_calls = calls
        .iter()
        .map(|(path, content)| match content {
            Some(content) => pi_js_compat::js_json!(["write", path, content]),
            None => pi_js_compat::js_json!(["mkdir", path]),
        })
        .collect::<Vec<_>>();
    assert_eq!(JsValue::Array(actual_calls), expected["calls"]);
    let TextOrImageContent::Text(text) = &result.content[0] else {
        panic!("text result")
    };
    assert_eq!(text.text, expected["text"].as_string().unwrap().clone());
}

// Shared support is compiled once per integration target.
#[path = "../../pi-ai/tests/support/oracle.rs"]
pub mod shared_ai_oracle;
