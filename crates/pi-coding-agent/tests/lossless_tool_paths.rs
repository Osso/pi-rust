#[path = "../../pi-ai/tests/support/oracle.rs"]
mod oracle;

use std::io;
use std::sync::{Arc, Mutex};

use pi_agent_core::agent::AgentTool;
use pi_ai::request::BoxFuture;
use pi_ai::types::TextOrImageContent;
use pi_coding_agent::core::tools::render_utils::{normalize_display_text, replace_tabs, str_value};
use pi_coding_agent::core::tools::write::{WriteOperations, WriteTool, WriteToolOptions};
use pi_js_compat::js_json::{JsString, JsValue, parse_lossless};

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
    let expected = source.request_lossless(&pi_js_compat::js_json!({"kind":"localWriteError", "path":path}));
    assert_eq!(
        operation_error_message(&error),
        expected["error"].as_string().unwrap().clone()
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
        .execute("edit".into(), params.clone(), None, Arc::new(|_| {}), None)
        .await
        .unwrap();
    let mut source = oracle::TsOracle::spawn(
        "lossless-tool-paths-oracle.ts",
        "packages/coding-agent/src/core/tools/edit.ts",
    );
    let expected = source.request_lossless(&pi_js_compat::js_json!({"kind":"edit","cwd":cwd,"params":params}));
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
        .execute("read".into(), params.clone(), None, Arc::new(|_| {}), None)
        .await
        .unwrap();
    let mut source = oracle::TsOracle::spawn(
        "lossless-tool-paths-oracle.ts",
        "packages/coding-agent/src/core/tools/read.ts",
    );
    let expected = source.request_lossless(&pi_js_compat::js_json!({"kind":"read","cwd":cwd,"params":params}));
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
            .execute("ls".into(), params.clone(), None, Arc::new(|_| {}), None)
            .await;
        let expected = source.request_lossless(
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
        .execute("write".into(), params.clone(), None, Arc::new(|_| {}), None)
        .await
        .unwrap();
    let mut source = oracle::TsOracle::spawn(
        "lossless-tool-paths-oracle.ts",
        "packages/coding-agent/src/core/tools/write.ts",
    );
    let expected = source.request_lossless(&pi_js_compat::js_json!({"kind":"write", "cwd":cwd, "params":params}));
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
