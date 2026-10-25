use crate::shared_oracle as oracle;
pub use crate::shared_render_owner as render_owner;

use pi_ai::types::{TextContent, TextOrImageContent};
use pi_coding_agent::core::export_html::RenderedToolResult;
use pi_coding_agent::core::export_html::{ToolHtmlRendererDeps, create_tool_html_renderer};
use pi_coding_agent::core::tools::edit::create_edit_tool_definition;
use pi_coding_agent::modes::interactive::theme::{ColorMode, load_theme_from_path, set_theme_instance};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsObject, JsString, JsValue};
use std::sync::Arc;

fn html_result(value: RenderedToolResult) -> JsValue {
    let mut fields = JsObject::new();
    if let Some(collapsed) = value.collapsed {
        fields.insert("collapsed".into(), JsValue::String(collapsed));
    }
    if let Some(expanded) = value.expanded {
        fields.insert("expanded".into(), JsValue::String(expanded));
    }
    JsValue::Object(fields)
}

#[test]
fn export_snapshot_and_reusable_html_owner_match_frozen_observation_boundaries() {
    let theme_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("theme/dark.json");
    let theme = Arc::new(
        load_theme_from_path(
            &JsString::from(theme_path.to_str().unwrap()),
            Some(ColorMode::Truecolor),
        )
        .unwrap(),
    );
    set_theme_instance(theme.clone());
    let mut source = oracle::TsOracle::spawn_with_environment(
        "edit-render-sequence-oracle.ts",
        "packages/coding-agent/src/core/tools/edit.ts",
        &[("COLORTERM", "truecolor".into())],
    );
    for observe in [false, true] {
        let source_root = tempfile::tempdir().unwrap();
        let native_root = tempfile::tempdir().unwrap();
        let expected: JsValue = source.request(
            &json!({"id":observe,"cases":[{"root":source_root.path(),"steps":[],"html":true,"awaitPreview":observe}]}),
        );
        assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
        let definition = create_edit_tool_definition(native_root.path().to_str().unwrap(), Default::default());
        let owner = render_owner::RenderOwner::new();
        let renderer = create_tool_html_renderer(ToolHtmlRendererDeps {
            get_tool_definition: Arc::new(move |_| Some(definition.clone())),
            theme: theme.clone(),
            cwd: native_root.path().to_str().unwrap().into(),
            width: Some(80),
            completion_executor: observe.then(|| owner.executor()),
        });
        let args = json!({"path":"missing.txt","edits":[{"oldText":"before","newText":"after"}]});
        let content = vec![TextOrImageContent::Text(TextContent::new(
            "Could not edit file: missing.txt. Error code: ENOENT.",
        ))];
        let call = renderer.render_call("edit-html", "edit", &args).unwrap();
        let immediate = renderer
            .render_result("edit-html", "edit", &content, None, true)
            .unwrap();
        let mut actual = json!({"call":call,"immediate":html_result(immediate)});
        if observe {
            owner.wait_for(|| {
                renderer
                    .render_result("edit-html", "edit", &content, None, true)
                    .unwrap()
                    .expanded
                    .as_ref()
                    .is_some_and(JsString::is_empty)
            });
            let settled = renderer
                .render_result("edit-html", "edit", &content, None, true)
                .unwrap();
            actual
                .as_object_mut()
                .unwrap()
                .insert("settled".into(), html_result(settled));
        }
        assert_eq!(actual, expected["output"][0], "observe after await: {observe}");
    }
}
