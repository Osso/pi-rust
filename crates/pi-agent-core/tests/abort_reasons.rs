use crate::shared_oracle as oracle;
use pi_agent_core::harness::FileSystem;
use pi_agent_core::harness::env::{NodeExecutionEnv, NodeExecutionEnvOptions};
use pi_agent_core::harness::types::OperationFailure;
use pi_js_compat::abort::{AbortController, AbortReason};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsString, JsValue};
use pi_js_compat::node_error::NativeNodeError;
use std::io::Write;

#[tokio::test]
async fn in_flight_file_abort_preserves_supplied_reason_in_nested_node_cause() {
    let mut oracle = oracle::TsOracle::spawn(
        "agent-core-node-env-oracle.ts",
        "packages/agent-core/src/harness/env/nodejs.ts",
    );
    let raw = JsString::from_utf16(vec![0xd800]);
    for reason in [
        AbortReason::Value(JsValue::Null),
        AbortReason::Value(JsValue::Bool(false)),
        AbortReason::Value(JsValue::String(raw.clone())),
        AbortReason::Error(NativeNodeError::new("Error", raw)),
    ] {
        let root = tempfile::tempdir().unwrap();
        let reference = tempfile::tempdir().unwrap();
        let descriptor = match &reason {
            AbortReason::Value(value) => json!({"kind":"value","value":value}),
            AbortReason::Error(error) => json!({"kind":"error","name":error.name,"message":error.message}),
            AbortReason::DomException(_) => unreachable!(),
        };
        let expected = oracle.request(&json!({"kind":"read-abort","method":"readTextFile","root":reference.path(),"withCauses":true,"abortReason":descriptor}));
        let path = root.path().join("read.fifo");
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        let mut writer = std::fs::OpenOptions::new().read(true).write(true).open(&path).unwrap();
        writer.write_all(b"prefix").unwrap();
        let environment = NodeExecutionEnv::new(NodeExecutionEnvOptions {
            cwd: root.path().to_str().unwrap().into(),
            ..Default::default()
        });
        let controller = AbortController::new();
        let signal = controller.signal();
        let read = tokio::spawn(async move { environment.read_text_file(&"read.fifo".into(), Some(&signal)).await });
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        controller.abort(Some(reason.clone()));
        drop(writer);
        let error = read.await.unwrap().unwrap_err();
        let OperationFailure::Returned(error) = error else {
            panic!("file abort resolves err")
        };
        let cause = error.cause.unwrap();
        let native = cause.downcast_ref::<NativeNodeError>().unwrap();
        assert_eq!(native.value()["cause"], reason.value());
        assert_eq!(native.value(), expected["error"]["cause"]);
    }
}
