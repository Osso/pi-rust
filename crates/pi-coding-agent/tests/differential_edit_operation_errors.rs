use crate::shared_oracle as oracle;

use pi_agent_core::agent::AgentTool;
use pi_ai::request::BoxFuture;
use pi_coding_agent::core::extensions::types::HandlerError;
use pi_coding_agent::core::tools::edit::{EditOperations, EditTool, EditToolOptions};
use pi_js_compat::abort::{AbortController, AbortSignal};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsString, JsValue};
use pi_js_compat::node_error::{NativeErrorClass, NativeNodeError};
use std::io;
use std::sync::{Arc, Mutex};

struct Operations {
    input: JsValue,
    controller: AbortController,
    calls: Mutex<Vec<&'static str>>,
}
impl Operations {
    fn enter(&self, phase: &'static str) -> io::Result<()> {
        self.calls.lock().unwrap().push(phase);
        if self.input["phase"].as_string().unwrap() != phase {
            return Ok(());
        }
        let class = if self.input["class"]
            .as_string()
            .is_some_and(|class| class == "TypeError")
        {
            NativeErrorClass::TypeError
        } else {
            NativeErrorClass::Error
        };
        let mut error = NativeNodeError::from_constructor(class, self.input["message"].as_string().unwrap());
        for field in ["name", "code"] {
            if let Some(value) = self.input.get(field) {
                error = error.with_property(field, value.clone());
            }
        }
        if self.input["abort"] == JsValue::Bool(true) {
            self.controller.abort(Some(json!("ignored reason").into()));
        }
        Err(io::Error::other(HandlerError::from(error)))
    }
}
impl EditOperations for Operations {
    fn access<'a>(&'a self, _path: &'a JsString) -> BoxFuture<'a, io::Result<()>> {
        Box::pin(async move { self.enter("access") })
    }
    fn read_file<'a>(&'a self, _path: &'a JsString) -> BoxFuture<'a, io::Result<Vec<u8>>> {
        Box::pin(async move {
            self.enter("read")?;
            Ok(b"before".to_vec())
        })
    }
    fn write_file<'a>(&'a self, _path: &'a JsString, _content: &'a JsString) -> BoxFuture<'a, io::Result<()>> {
        Box::pin(async move { self.enter("write") })
    }
}

#[tokio::test]
async fn edit_operation_error_metadata_is_observed_before_the_frozen_catch_projection() {
    let mut source = oracle::TsOracle::spawn(
        "edit-operation-errors-oracle.ts",
        "packages/coding-agent/src/core/tools/edit.ts",
    );
    let cases = [
        json!({"phase":"access","message":"offline"}),
        json!({"phase":"access","message":"typed","class":"TypeError"}),
        json!({"phase":"access","message":"raw","name":JsString::from_utf16(vec![0xd800])}),
        json!({"phase":"access","message":"coded","code":"ENOENT"}),
        json!({"phase":"access","message":"typed coded","class":"TypeError","code":"ERR_CUSTOM"}),
        json!({"phase":"access","message":"null code","code":null}),
        json!({"phase":"access","message":"false code","code":false}),
        json!({"phase":"access","message":"object code","code":{}}),
        json!({"phase":"access","message":"array code","code":["x",null]}),
        json!({"phase":"access","message":"bad code","code":{"toString":1}}),
        json!({"phase":"access","message":""}),
        json!({"phase":"access","message":"empty name","name":""}),
        json!({"phase":"access","message":"abort wins","code":{"toString":1},"abort":true}),
        json!({"phase":"read","message":JsString::from_utf16(vec![0xd800])+" read wins","class":"TypeError","code":"ERR_READ","abort":true}),
        json!({"phase":"write","message":JsString::from_utf16(vec![0xdfff])+" write wins","class":"TypeError","code":false,"abort":true}),
    ];
    let mut failures = Vec::new();
    for (index, input) in cases.into_iter().enumerate() {
        let root = tempfile::tempdir().unwrap();
        let mut request = input.clone();
        request
            .as_object_mut()
            .unwrap()
            .insert("root".into(), json!(root.path()));
        let expected: JsValue = source.request(&json!({"id":index,"cases":[request]}));
        assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
        let operations = Arc::new(Operations {
            input,
            controller: AbortController::new(),
            calls: Mutex::new(Vec::new()),
        });
        let signal: AbortSignal = operations.controller.signal();
        let tool = EditTool::with_options(
            root.path().to_str().unwrap(),
            EditToolOptions {
                operations: Some(operations.clone()),
            },
        );
        let error = tool.execute("edit-errors".into(), json!({"path":JsString::from("raw-")+JsString::from_utf16(vec![0xdfff])+".txt","edits":[{"oldText":"before","newText":"after"}]}), Some(signal), Arc::new(|_| Ok(())), None).await.unwrap_err();
        let actual = json!({"calls":operations.calls.lock().unwrap().clone(),"error":error.message});
        if actual != expected["output"][0] {
            failures.push(format!(
                "case {index}: {} != {}",
                pi_js_compat::js_json::stringify_lossless(&actual),
                pi_js_compat::js_json::stringify_lossless(&expected["output"][0])
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
