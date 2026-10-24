#[path = "support/oracle.rs"]
mod oracle;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsString, JsValue};
fn source_contract(cwd: &str, raw: &JsString) -> JsValue {
    let mut source = oracle::TsOracle::spawn(
        "bash-environment-oracle.ts",
        "packages/coding-agent/src/core/tools/bash.ts",
    );
    let expected =
        source.request(&json!({"id":1,"cwd":cwd,"raw":raw,"cases":["custom","default","empty","undefined"]}));
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    assert_eq!(
        expected["output"][0],
        json!({"own":true,"value":"undefined","kept":raw})
    );
    for (index, text) in ["inherited:absent", "absent:absent", "absent:kept"]
        .into_iter()
        .enumerate()
    {
        assert_eq!(expected["output"][index + 1]["bytes"], json!(text.as_bytes().to_vec()));
    }
    expected["output"].clone()
}
#[test]
fn frozen_undefined_env_property_is_retained_until_spawn() {
    let root = tempfile::tempdir().unwrap();
    source_contract(root.path().to_str().unwrap(), &JsString::from_utf16(vec![0xd800]));
}

use pi_agent_core::agent::{AgentTool, ToolError};
use pi_ai::request::BoxFuture;
use pi_coding_agent::core::tools::bash::{
    BashExecOptions, BashExecResult, BashOperations, BashTool, BashToolOptions, create_local_bash_operations,
};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
struct Operations(Arc<Mutex<Option<JsValue>>>);
impl BashOperations for Operations {
    fn exec<'a>(
        &'a self,
        _: &'a JsString,
        _: &'a JsString,
        options: BashExecOptions,
    ) -> BoxFuture<'a, Result<BashExecResult, ToolError>> {
        Box::pin(async move {
            let env = options.env.unwrap();
            let kept = env.get(&JsString::from("BASH_KEEP")).unwrap().as_ref().unwrap();
            *self.0.lock().unwrap() = Some(
                json!({"own":env.contains_key(&JsString::from("BASH_REMOVE")),"value":if env.get(&JsString::from("BASH_REMOVE")).unwrap().is_none(){"undefined"}else{"defined"},"kept":kept}),
            );
            Ok(BashExecResult {
                exit_code: Some(0.0),
                detached: None,
            })
        })
    }
}
#[tokio::test]
async fn native_undefined_env_key_and_absent_empty_local_environments_match_frozen() {
    let root = tempfile::tempdir().unwrap();
    let cwd = JsString::from(root.path().to_str().unwrap());
    let raw = JsString::from_utf16(vec![0xd800]);
    let expected = source_contract(root.path().to_str().unwrap(), &raw);
    let captured = Arc::new(Mutex::new(None));
    let tool = BashTool::with_options(
        cwd.clone(),
        BashToolOptions {
            operations: Some(Arc::new(Operations(captured.clone()))),
            spawn_hook: Some(Arc::new(move |mut context| {
                context.env.insert("BASH_REMOVE".into(), None);
                context.env.insert("BASH_KEEP".into(), Some(raw.clone()));
                Ok(context)
            })),
            ..Default::default()
        },
    );
    tool.execute(
        "env".into(),
        json!({"command":"probe"}),
        None,
        Arc::new(|_| Ok(())),
        None,
    )
    .await
    .unwrap();
    assert_eq!(captured.lock().unwrap().as_ref().unwrap(), &expected[0]);
    pi_js_compat::process_env::set("BASH_REMOVE", "inherited");
    for (index, env) in [
        None,
        Some(BTreeMap::new()),
        Some(BTreeMap::from([
            ("BASH_REMOVE".into(), None),
            ("BASH_KEEP".into(), Some("kept".into())),
        ])),
    ]
    .into_iter()
    .enumerate()
    {
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let sink = bytes.clone();
        let result = create_local_bash_operations(None)
            .exec(
                &"printf '%s:%s' \"${BASH_REMOVE-absent}\" \"${BASH_KEEP-absent}\"".into(),
                &cwd,
                BashExecOptions {
                    on_data: Arc::new(move |data| {
                        sink.lock().unwrap().extend_from_slice(data);
                        Ok(())
                    }),
                    signal: None,
                    timeout: None,
                    env,
                    detach: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(
            json!({"result":{"exitCode":result.exit_code},"bytes":bytes.lock().unwrap().clone()}),
            expected[index + 1]
        );
    }
    pi_js_compat::process_env::clear();
}
