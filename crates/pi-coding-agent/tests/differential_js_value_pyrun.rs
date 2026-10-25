use crate::shared_oracle as oracle;

use pi_coding_agent::bundled_extensions::pyrun::eval_tool::{
    PyrunEvalParams, format_canonical_pyrun_eval_result, format_progress_text,
};
use pi_coding_agent::bundled_extensions::pyrun::runner::{CanonicalPyrunEvalResult, CanonicalPyrunProgressUpdate};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsValue as Value};

#[test]
fn pyrun_value_text_matches_the_pinned_eval_module() {
    let mut oracle = oracle::TsOracle::spawn(
        "js-value-pyrun-oracle.ts",
        "packages/coding-agent/extensions/pyrun/src/eval-tool.ts",
    );
    for raw in [
        r#"{"2":2.0,"1":1.0}"#,
        r#"{"2":{"4294967294":2.0,"0":-0.0,"01":1e-7,"x":9007199254740993},"1":[2.0,1e21]}"#,
    ] {
        let value: Value = js_json::parse_lossless(raw).unwrap();
        let result = json!({"type":"completed", "value":value});
        let update = json!({"type":"console", "value":value});
        let expected = oracle.request(&json!({"id":raw,"params":{"code":"x"},"result":result,"update":update}));
        assert_eq!(expected["ok"], json!(true), "{expected:?}");
        let result: CanonicalPyrunEvalResult = js_json::from_value(&result).unwrap();
        let actual = format_canonical_pyrun_eval_result(
            &PyrunEvalParams {
                code: "x".into(),
                session_id: None,
            },
            result,
        );
        assert_eq!(
            js_json::to_value(&actual.content).unwrap(),
            expected["output"]["result"]["content"]
        );
        let progress = format_progress_text(&CanonicalPyrunProgressUpdate { raw: update });
        assert_eq!(json!(progress), expected["output"]["progress"][0]["content"][0]["text"]);
    }
}
