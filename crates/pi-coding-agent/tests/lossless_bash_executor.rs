use pi_agent_core::agent::ToolError;
use pi_ai::request::BoxFuture;
use pi_coding_agent::core::bash_executor::{BashExecutorOptions, execute_bash_with_operations};
use pi_coding_agent::core::tools::bash::{BashExecOptions, BashExecResult, BashOperations};
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use std::sync::Arc;

struct Operations {
    command: JsString,
    cwd: JsString,
    output: Vec<u8>,
}
impl BashOperations for Operations {
    fn exec<'a>(
        &'a self,
        command: &'a JsString,
        cwd: &'a JsString,
        options: BashExecOptions,
    ) -> BoxFuture<'a, Result<BashExecResult, ToolError>> {
        Box::pin(async move {
            assert_eq!(command, &self.command);
            assert_eq!(cwd, &self.cwd);
            (options.on_data)(&self.output);
            Ok(BashExecResult {
                exit_code: Some(0.0),
                detached: None,
            })
        })
    }
}

#[tokio::test]
async fn executor_passes_raw_cwd_and_returns_native_spill_path_with_full_bytes() {
    let raw = JsString::from_utf16(vec![0xd800]);
    let command = JsString::from("command-") + &raw;
    let cwd = JsString::from("/cwd-") + &raw;
    let output = vec![b'x'; 100_000];
    let operations = Arc::new(Operations {
        command: command.clone(),
        cwd: cwd.clone(),
        output: output.clone(),
    });
    let result = execute_bash_with_operations(&command, &cwd, operations, &BashExecutorOptions::default())
        .await
        .unwrap();
    assert_eq!(result.exit_code, Some(0.0));
    assert!(result.truncated);
    let path: JsString = result.full_output_path.unwrap();
    assert_eq!(std::fs::read(encode_node_utf8(&path)).unwrap(), output);
    std::fs::remove_file(encode_node_utf8(&path)).unwrap();
}
