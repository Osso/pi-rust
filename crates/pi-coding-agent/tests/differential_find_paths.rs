use crate::shared_oracle as oracle;

use pi_agent_core::agent::AgentTool;
use pi_ai::request::BoxFuture;
use pi_coding_agent::core::tools::find::{FindOperations, FindTool, FindToolOptions};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::io;
use std::sync::Arc;

struct Operations(Vec<JsString>);
impl FindOperations for Operations {
    fn supports_glob(&self) -> bool {
        true
    }
    fn exists<'a>(&'a self, _: &'a JsString) -> BoxFuture<'a, io::Result<bool>> {
        Box::pin(async { Ok(true) })
    }
    fn glob<'a>(
        &'a self,
        _: &'a JsString,
        _: &'a JsString,
        _: &'a [JsString],
        _: f64,
    ) -> BoxFuture<'a, io::Result<Vec<JsString>>> {
        Box::pin(async { Ok(self.0.clone()) })
    }
}

fn path_cases() -> Vec<JsValue> {
    vec![
        json!({"paths":["/repo/list/alpha","/repo/list/dir/"]}),
        json!({"paths":["/repo/listx/alpha"]}),
        json!({"paths":["/repo/list"]}),
        json!({"paths":["relative.txt"]}),
        json!({"paths":["/repo/list/a\\b"]}),
        json!({"paths":["relative/"]}),
        json!({"paths":[JsString::from("/repo/list") + JsString::from_utf16(vec![0xd800,0xdc00]) + "/x"]}),
        json!({"paths":["/else/a"]}),
    ]
}

#[tokio::test]
async fn custom_find_paths_match_frozen_prefix_slicing_and_process_relative_resolution() {
    let cases = path_cases();
    let mut source = oracle::TsOracle::spawn("find-paths-oracle.ts", "packages/coding-agent/src/core/tools/find.ts");
    let expected: JsValue =
        source.request(&json!({"id":1,"cases":cases,"processCwd":std::env::current_dir().unwrap()}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    assert_eq!(expected["output"].as_array().unwrap().len(), cases.len());
    let mut failures = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let paths = case["paths"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_string().unwrap().clone())
            .collect();
        let tool = FindTool::with_options(
            "/repo",
            FindToolOptions {
                operations: Some(Arc::new(Operations(paths))),
            },
        );
        let result = tool
            .execute(
                "find-paths".into(),
                json!({"pattern":"*","path":"list"}),
                None,
                Arc::new(|_| Ok(())),
                None,
            )
            .await
            .unwrap();
        let actual = js_json::to_value(&result).unwrap();
        if actual != expected["output"][index] {
            failures.push(format!(
                "case {index}: {} != {}",
                js_json::stringify_lossless(&actual),
                js_json::stringify_lossless(&expected["output"][index])
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
