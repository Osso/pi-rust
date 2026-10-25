use crate::shared_oracle as oracle;

use pi_coding_agent::core::tools::write::create_write_tool_definition;
use pi_coding_agent::modes::interactive::components::tool_execution::{
    ToolExecutionComponent, ToolExecutionOptions, ToolExecutionResult,
};
use pi_coding_agent::modes::interactive::theme::{ColorMode, load_theme_from_path, set_theme_instance};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue};
use pi_tui::Component;
use std::sync::Arc;

#[test]
fn write_result_kind_changes_reach_frozen_interactive_catch_and_reset() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("theme/dark.json");
    let theme =
        Arc::new(load_theme_from_path(&JsString::from(path.to_str().unwrap()), Some(ColorMode::Truecolor)).unwrap());
    set_theme_instance(theme);
    let input = json!({"tool":"write","path":"example.txt","results":[
        {"content":[{"type":"text","text":"hidden partial"}],"isError":false,"partial":true},
        {"content":[{"type":"text","text":"first failure"}],"isError":true,"partial":true},
        {"content":[{"type":"text","text":"second failure"}],"isError":true,"partial":true},
        {"content":[],"isError":false,"partial":true},
        {"content":[{"type":"text","text":"hidden recovery"}],"isError":false,"partial":true},
        {"content":[{"type":"text","text":"final failure"}],"isError":true,"partial":false},
    ]});
    let mut oracle = oracle::TsOracle::spawn_with_environment(
        "language-render-boundary-oracle.ts",
        "packages/coding-agent/src/core/tools/write.ts",
        &[("COLORTERM", "truecolor".into())],
    );
    let expected: JsValue = oracle.request(&json!({"id":1,"cases":[input.clone()]}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    let widget = ToolExecutionComponent::new(
        "write",
        "language-error",
        json!({"path":"example.txt","content":"plain"}),
        ToolExecutionOptions::default(),
        Some(create_write_tool_definition("/repo", Default::default())),
        Arc::new(|| {}),
        "/repo",
    );
    for (index, step) in input["results"].as_array().unwrap().iter().enumerate() {
        widget.borrow_mut().update_result(
            ToolExecutionResult {
                content: js_json::from_value(&step["content"]).unwrap(),
                details: None,
                is_error: step["isError"] == JsValue::Bool(true),
            },
            step["partial"] == JsValue::Bool(true),
            Some(1_700_000_000_000.0),
        );
        let actual = json!(widget.borrow_mut().render(80));
        assert_eq!(actual, expected["output"][0]["frames"][index], "result frame {index}");
    }
}
