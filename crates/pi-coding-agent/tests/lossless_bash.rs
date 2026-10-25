use indexmap::IndexMap;
use pi_agent_core::agent::{AgentTool, ToolError};
use pi_ai::request::BoxFuture;
use pi_coding_agent::core::tools::bash::{BashExecOptions, BashExecResult, BashOperations, BashTool, BashToolOptions};
use pi_js_compat::js_json::{JsString, parse_lossless};
use std::sync::{Arc, Mutex};

type RecordedCall = (JsString, JsString, IndexMap<JsString, Option<JsString>>);

#[derive(Default)]
struct RecordingOperations {
    calls: Mutex<Vec<RecordedCall>>,
}
impl BashOperations for RecordingOperations {
    fn exec<'a>(
        &'a self,
        command: &'a JsString,
        cwd: &'a JsString,
        options: BashExecOptions,
    ) -> BoxFuture<'a, Result<BashExecResult, ToolError>> {
        Box::pin(async move {
            self.calls
                .lock()
                .unwrap()
                .push((command.clone(), cwd.clone(), options.env.expect("tool passes env")));
            Ok(BashExecResult {
                exit_code: Some(0.0),
                detached: None,
            })
        })
    }
}

#[tokio::test]
async fn spawn_hook_and_custom_exec_keep_command_cwd_and_env_units() {
    let operations = Arc::new(RecordingOperations::default());
    let cwd = JsString::from("/cwd-") + JsString::from_utf16(vec![0xd800]);
    let prefix = JsString::from("prefix-") + JsString::from_utf16(vec![0xdfff]);
    let key = JsString::from("ENV_") + JsString::from_utf16(vec![0xd801]);
    let value = JsString::from("value-") + JsString::from_utf16(vec![0xd802]);
    let hook_key = key.clone();
    let hook_value = value.clone();
    let tool = BashTool::with_options(
        &cwd,
        BashToolOptions {
            operations: Some(operations.clone()),
            command_prefix: Some(prefix.clone()),
            spawn_hook: Some(Arc::new(move |mut context| {
                context.env.insert(hook_key.clone(), Some(hook_value.clone()));
                Ok(context)
            })),
            ..BashToolOptions::default()
        },
    );
    tool.execute(
        "bash".into(),
        parse_lossless(r#"{"command":"command-\ud803"}"#).unwrap(),
        None,
        Arc::new(|_| Ok(())),
        None,
    )
    .await
    .unwrap();
    let calls = operations.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, prefix + "\ncommand-" + JsString::from_utf16(vec![0xd803]));
    assert_eq!(calls[0].1, cwd);
    assert_eq!(calls[0].2.get(&key), Some(&Some(value)));
}
