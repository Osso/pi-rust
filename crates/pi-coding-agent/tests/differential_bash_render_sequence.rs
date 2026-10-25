use crate::shared_oracle as oracle;

use pi_agent_core::agent::AgentToolResult;
use pi_coding_agent::core::extensions::types::{ToolRenderContext, ToolRenderResultOptions, ToolRenderState};
use pi_coding_agent::core::tools::bash::create_bash_tool_definition;
use pi_coding_agent::modes::interactive::theme::{ColorMode, load_theme_from_path, set_theme_instance};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue};
use pi_tui::tui::ComponentRef;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

// Frozen bash.ts:801-838: previous call Text/result Container reuse and native
// wrong-component errors are observable by the built-in ToolExecutionComponent.
#[test]
fn bash_renderers_reuse_components_and_keep_cross_kind_errors() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("theme/dark.json");
    let theme =
        Arc::new(load_theme_from_path(&JsString::from(path.to_str().unwrap()), Some(ColorMode::Truecolor)).unwrap());
    set_theme_instance(theme.clone());
    let raw = JsString::from_utf16(vec![0xd800]);
    let steps = vec![
        json!({"kind":"call","args":{}}),
        json!({"kind":"call","args":{"command":raw,"timeout":1.25}}),
        json!({"kind":"call","wrongComponent":true,"args":{"command":"wrong"}}),
        json!({"kind":"call","args":{"command":"recovered"}}),
        json!({"kind":"result","result":{"content":[]}}),
        json!({"kind":"result","result":{"content":[{"type":"text","text":"line\n".repeat(9)}]}}),
        json!({"kind":"result","width":20,"result":{"content":[{"type":"text","text":"wide line ".repeat(30)}]}}),
        json!({"kind":"result","expanded":true,"result":{"content":[{"type":"text","text":raw}]}}),
        json!({"kind":"result","wrongComponent":true,"result":{"content":[]}}),
        json!({"kind":"result","result":{"content":[{"type":"text","text":"recovered"}]}}),
        json!({"kind":"result","result":{"content":[{"type":"text","text":"changed"}]}}),
        json!({"kind":"result","result":{"content":[]}}),
        json!({"kind":"result","result":{"content":[{"type":"text","text":"after-empty"}]}}),
        json!({"kind":"result","width":20,"result":{"content":[{"type":"text","text":"width-change"}]}}),
        json!({"kind":"result","invalidate":true,"width":20,"result":{"content":[{"type":"text","text":"invalidated"}]}}),
        json!({"kind":"result","expanded":true,"width":20,"result":{"content":[{"type":"text","text":"expanded-new"}]}}),
        json!({"kind":"result","width":20,"result":{"content":[{"type":"text","text":"collapsed-after-expanded"}]}}),
    ];
    let mut source = oracle::TsOracle::spawn(
        "bash-render-sequence-oracle.ts",
        "packages/coding-agent/src/core/tools/bash.ts",
    );
    let expected = source.request(&json!({"id":28051,"cases":[steps]}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    let definition = create_bash_tool_definition("/repo", Default::default());
    let call_renderer = definition.render_call.unwrap();
    let result_renderer = definition.render_result.unwrap();
    let state = ToolRenderState::new();
    state.update(|value| *value = json!({"startedAt":0,"endedAt":1250}));
    let mut call: Option<ComponentRef> = None;
    let mut result: Option<ComponentRef> = None;
    let mut failures = Vec::new();
    for (index, step) in steps.iter().enumerate() {
        let is_call = step["kind"] == json!("call");
        let previous = if is_call { call.clone() } else { result.clone() };
        if step["invalidate"] == JsValue::Bool(true)
            && let Some(previous) = &previous
        {
            previous.borrow_mut().invalidate();
        }
        let wrong: ComponentRef = if is_call {
            Rc::new(RefCell::new(pi_tui::Container::new()))
        } else {
            Rc::new(RefCell::new(pi_tui::Text::new("wrong", 0, 0)))
        };
        let expanded = step["expanded"] == JsValue::Bool(true);
        let context = ToolRenderContext {
            args: step["args"].clone(),
            tool_call_id: "bash-sequence".into(),
            invalidate: Arc::new(|| {}),
            last_component: if step["wrongComponent"] == JsValue::Bool(true) {
                Some(wrong)
            } else {
                previous.clone()
            },
            call_component: None,
            state: state.clone(),
            cwd: "/repo".into(),
            execution_started: false,
            args_complete: true,
            is_partial: false,
            expanded,
            show_images: false,
            is_error: false,
        };
        let rendered = if is_call {
            call_renderer(&step["args"], &theme, &context)
        } else {
            let value: AgentToolResult = js_json::from_value(&step["result"]).unwrap();
            result_renderer(
                &value,
                &ToolRenderResultOptions {
                    expanded,
                    is_partial: false,
                },
                &theme,
                &context,
            )
        };
        let actual = match rendered {
            Ok(component) => {
                let reused = previous.as_ref().is_some_and(|old| Rc::ptr_eq(old, &component));
                let lines = component
                    .borrow_mut()
                    .render(step["width"].as_u64().unwrap_or(80) as usize);
                if is_call {
                    call = Some(component)
                } else {
                    result = Some(component)
                }
                json!({"lines":lines,"reused":reused})
            }
            Err(error) => {
                let native = std::error::Error::source(&error)
                    .unwrap()
                    .downcast_ref::<pi_js_compat::node_error::NativeNodeError>()
                    .expect("native error identity");
                json!({"error":{"name":native.name.clone(),"message":error.message.clone()}})
            }
        };
        if actual != expected["output"][0][index] {
            failures.push(json!({"frame":index,"actual":actual,"expected":expected["output"][0][index]}));
        }
    }
    assert!(failures.is_empty(), "{}", js_json::stringify_lossless(&json!(failures)));
}
