use crate::shared_oracle as oracle;

use pi_agent_core::agent::AgentToolResult;
use pi_coding_agent::core::extensions::types::{ToolRenderContext, ToolRenderResultOptions, ToolRenderState};
use pi_coding_agent::core::tools::bash::render_result;
use pi_coding_agent::modes::interactive::theme::{ColorMode, load_theme_from_path, set_theme_instance};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::sync::Arc;

// Frozen bash.ts:269-271 uses Number.toFixed(1); 534-539 and 577-592
// gate stripping/warnings on path truthiness, not property presence.
#[test]
fn bash_result_duration_and_path_truthiness_match_frozen() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("theme/dark.json");
    let theme =
        Arc::new(load_theme_from_path(&JsString::from(path.to_str().unwrap()), Some(ColorMode::Truecolor)).unwrap());
    set_theme_instance(theme.clone());
    let mut cases = Vec::new();
    for duration in [1250.0, -1250.0, 2250.0, 1150.0, 0.0, 1e24] {
        cases.push(json!({"result":{"content":[]},"state":{"startedAt":0,"endedAt":duration}}));
    }
    let raw = JsString::from_utf16(vec![0xd800]);
    for path in [JsString::default(), JsString::from("/log"), raw] {
        for truncated in [false, true] {
            cases.push(json!({"result":{
                "content":[{"type":"text","text":"body\n\n[Full output: /log]"}],
                "details":{"fullOutputPath":path,"truncation":{
                    "content":"body", "truncated":truncated,"truncatedBy":"lines",
                    "totalLines":9,"totalBytes":100,"outputLines":1,"outputBytes":4,
                    "lastLinePartial":false,"firstLineExceedsLimit":false,"maxLines":1,"maxBytes":1024
                }}
            }}));
        }
    }
    let mut oracle = oracle::TsOracle::spawn(
        "bash-render-state-oracle.ts",
        "packages/coding-agent/src/core/tools/bash.ts",
    );
    let expected = oracle.request(&json!({"id":28050,"cases":cases}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    let mut mismatches = Vec::new();
    for (index, frame) in cases.iter().enumerate() {
        let state = ToolRenderState::new();
        state.update(|value| *value = frame.get("state").cloned().unwrap_or_else(|| json!({})));
        let context = ToolRenderContext {
            args: json!({}),
            tool_call_id: "bash-render-state".into(),
            invalidate: Arc::new(|| {}),
            last_component: None,
            call_component: None,
            state,
            cwd: "/repo".into(),
            execution_started: false,
            args_complete: true,
            is_partial: false,
            expanded: true,
            show_images: true,
            is_error: false,
        };
        let result: AgentToolResult = js_json::from_value(&frame["result"]).unwrap();
        let component = render_result(
            &result,
            &ToolRenderResultOptions {
                expanded: true,
                is_partial: false,
            },
            &theme,
            &context,
        )
        .expect("bash renderer");
        let actual = json!(component.borrow_mut().render(80));
        if actual != expected["output"][index] {
            mismatches.push(json!({"case":index,"actual":actual,"expected":expected["output"][index]}));
        }
    }
    assert!(
        mismatches.is_empty(),
        "{}",
        js_json::stringify_lossless(&json!(mismatches))
    );
}
