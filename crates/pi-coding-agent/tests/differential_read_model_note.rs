use crate::shared_oracle as oracle;
use crate::shared_tool_context as tool_context;

use pi_ai::request::BoxFuture;
use pi_coding_agent::core::tools::read::{ReadOperations, ReadToolOptions, create_read_tool_definition};
use pi_js_compat::js_json::{JsString, JsValue, to_value};
use serde_json::{Value, json};
use std::{io, sync::Arc};

struct ImageOperations(String);
impl ReadOperations for ImageOperations {
    fn read_file<'a>(&'a self, _: &'a JsString) -> BoxFuture<'a, io::Result<Vec<u8>>> {
        Box::pin(async { Ok(b"not-an-image".to_vec()) })
    }
    fn access<'a>(&'a self, _: &'a JsString) -> BoxFuture<'a, io::Result<()>> {
        Box::pin(async { Ok(()) })
    }
    fn detect_image_mime_type<'a>(&'a self, _: &'a JsString) -> BoxFuture<'a, io::Result<Option<String>>> {
        Box::pin(async { Ok(Some(self.0.clone())) })
    }
}

#[tokio::test]
async fn read_image_notes_use_the_live_optional_context_model() {
    let cwd = tempfile::tempdir().unwrap();
    let cwd = JsString::from(cwd.path().to_str().unwrap());
    let cases: Vec<Value> = ["image/png", "image/tiff"]
        .into_iter()
        .flat_map(|mime| {
            [
                json!({"mime":mime}),
                json!({"mime":mime,"input":["text"]}),
                json!({"mime":mime,"input":["image"]}),
                json!({"mime":mime,"input":["text","image"]}),
            ]
        })
        .collect();
    let mut oracle = oracle::TsOracle::spawn(
        "read-model-note-oracle.ts",
        "packages/coding-agent/src/core/tools/read.ts",
    );
    let expected: Value = oracle.request(&json!({"id":81,"cwd":cwd.to_utf8().unwrap(),"cases":cases}));
    assert_eq!(expected["ok"], json!(true), "{expected}");
    let mut mismatches = Vec::new();
    for (case, expected) in cases.iter().zip(expected["output"].as_array().unwrap()) {
        let context = if let Some(input) = case.get("input") {
            let model: pi_ai::types::Model = serde_json::from_value(json!({
                "id":"fixture", "name":"Fixture", "api":"openai-responses", "provider":"fixture", "baseUrl":"https://invalid.example",
                "reasoning":false,"input":input,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":100,"maxTokens":10,
            })).unwrap();
            let (_, context) = tool_context::TestContextBuilder::new(
                cwd.clone(),
                tool_context::in_memory_session(cwd.clone(), "image-note"),
            )
            .model(model)
            .build()
            .await;
            Some(context)
        } else {
            None
        };
        let definition = create_read_tool_definition(
            cwd.clone(),
            ReadToolOptions {
                auto_resize_images: Some(false),
                operations: Some(Arc::new(ImageOperations(case["mime"].as_str().unwrap().into()))),
            },
        );
        let result = (definition.execute)(
            "image-note".into(),
            JsValue::from(json!({"path":"image"})),
            None,
            None,
            context,
        )
        .await
        .unwrap();
        let actual = to_value(&result).unwrap().to_serde().unwrap();
        if actual != *expected {
            mismatches.push(format!("{case}: {actual} != {expected}"));
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}
