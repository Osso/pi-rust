use pi_agent_core::agent::AgentTool;
use pi_ai::request::BoxFuture;
use pi_ai::types::TextOrImageContent;
use pi_coding_agent::core::tools::code_index::{
    CodeIndexCommandResult, CodeIndexOperations, CodeIndexToolOptions, OutlineTool,
};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json::{JsString, parse_lossless};
use std::{
    io,
    sync::{Arc, Mutex},
};

#[derive(Default)]
struct RecordingOperations {
    calls: Mutex<Vec<(Vec<JsString>, JsString)>>,
}
impl CodeIndexOperations for RecordingOperations {
    fn run<'a>(
        &'a self,
        args: &'a [JsString],
        cwd: &'a JsString,
        _: Option<&'a AbortSignal>,
    ) -> BoxFuture<'a, io::Result<CodeIndexCommandResult>> {
        Box::pin(async move {
            self.calls.lock().unwrap().push((args.to_vec(), cwd.clone()));
            Ok(CodeIndexCommandResult {
                stdout: JsString::from("result-") + JsString::from_utf16(vec![0xdfff]) + "\n",
                stderr: JsString::default(),
                exit_code: Some(0),
            })
        })
    }
}

#[tokio::test]
async fn custom_code_index_keeps_cwd_args_and_result_units() {
    let cwd = JsString::from("/work-") + JsString::from_utf16(vec![0xd800]);
    let ops = Arc::new(RecordingOperations::default());
    let tool = OutlineTool::with_options(
        &cwd,
        CodeIndexToolOptions {
            operations: Some(ops.clone()),
        },
    );
    let input = parse_lossless(r#"{"path":"file-\udfff","show":"part-\ud801"}"#).unwrap();
    let result = tool
        .execute("outline".into(), input, None, Arc::new(|_| Ok(())), None)
        .await
        .unwrap();
    assert_eq!(
        *ops.calls.lock().unwrap(),
        vec![(
            vec![
                "outline".into(),
                JsString::from("file-") + JsString::from_utf16(vec![0xdfff]),
                "--show".into(),
                JsString::from("part-") + JsString::from_utf16(vec![0xd801])
            ],
            cwd
        )]
    );
    let TextOrImageContent::Text(text) = &result.content[0] else {
        panic!("text")
    };
    assert_eq!(
        text.text,
        JsString::from("result-") + JsString::from_utf16(vec![0xdfff])
    );
}
