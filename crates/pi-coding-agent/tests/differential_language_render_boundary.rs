#[path = "support/oracle.rs"]
mod oracle;

use pi_ai::types::{TextContent, TextOrImageContent};
use pi_coding_agent::core::export_html::{ToolHtmlRendererDeps, create_tool_html_renderer};
use pi_coding_agent::core::tools::{read, write};
use pi_coding_agent::modes::interactive::components::tool_execution::{
    ToolExecutionComponent, ToolExecutionOptions, ToolExecutionResult,
};
use pi_coding_agent::modes::interactive::theme::{ColorMode, load_theme_from_path, set_theme_instance};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsString, JsValue};
use pi_tui::Component;
use std::sync::Arc;

#[test]
fn inherited_language_errors_reach_frozen_tui_and_html_catches() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("theme/dark.json");
    let theme =
        Arc::new(load_theme_from_path(&JsString::from(path.to_str().unwrap()), Some(ColorMode::Truecolor)).unwrap());
    set_theme_instance(theme.clone());
    let cases = [
        json!({"tool":"write","path":"example.constructor"}),
        json!({"tool":"write","path":"example.__proto__"}),
        json!({"tool":"read","path":"example.constructor"}),
        json!({"tool":"read","path":"example.__proto__"}),
    ];
    let mut oracle = oracle::TsOracle::spawn_with_environment(
        "language-render-boundary-oracle.ts",
        "packages/coding-agent/src/core/tools/write.ts",
        &[("COLORTERM", "truecolor".into())],
    );
    let expected: JsValue = oracle.request(&json!({"id":1,"cases":cases}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    for (index, case) in cases.iter().enumerate() {
        let name = case["tool"].as_string().unwrap().to_utf8().unwrap();
        let args = json!({"path":case["path"],"content":"plain"});
        let definition = if name == "write" {
            write::create_write_tool_definition("/repo", Default::default())
        } else {
            read::create_read_tool_definition("/repo", Default::default())
        };
        let widget = ToolExecutionComponent::new(
            &name,
            "language-error",
            args.clone(),
            ToolExecutionOptions::default(),
            Some(definition.clone()),
            Arc::new(|| {}),
            "/repo",
        );
        let exporter = create_tool_html_renderer(ToolHtmlRendererDeps {
            get_tool_definition: Arc::new(move |_| Some(definition.clone())),
            theme: theme.clone(),
            cwd: "/repo".into(),
            width: Some(80),
        });
        let mut html = exporter
            .render_call("language-error", &name, &args)
            .map(JsValue::String)
            .unwrap_or(JsValue::Null);
        if name == "read" {
            widget.borrow_mut().set_expanded(true);
            let content = vec![TextOrImageContent::Text(TextContent::new("literal body"))];
            widget.borrow_mut().update_result(
                ToolExecutionResult {
                    content: content.clone(),
                    details: None,
                    is_error: true,
                },
                false,
                Some(1_700_000_000_000.0),
            );
            assert!(
                exporter
                    .render_result("language-error", "read", &content, None, true)
                    .is_none()
            );
            html = JsValue::Null;
        }
        let actual = json!({"lines":widget.borrow_mut().render(80),"html":html});
        assert!(
            actual == expected["output"][index],
            "case {index}: {} != {}",
            json::stringify_lossless(&actual),
            json::stringify_lossless(&expected["output"][index])
        );
    }
}
