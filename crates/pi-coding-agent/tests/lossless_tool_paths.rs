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
    assert_eq!(
        normalize_display_text(&replace_tabs(&raw)).units(),
        &[0xd800, 32, 32, 32, 0xdfff, 0xd83d, 0xde00]
    );
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
