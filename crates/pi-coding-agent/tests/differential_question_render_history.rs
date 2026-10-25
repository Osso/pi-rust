use crate::shared_oracle as oracle;

use pi_coding_agent::core::extensions::types::{ToolRenderContext, ToolRenderResultOptions, ToolRenderState};
use pi_coding_agent::core::tools::ask_questions::create_ask_questions_tool_definition;
use pi_coding_agent::modes::interactive::theme::{ColorMode, load_theme_from_path};
use pi_js_compat::{
    js_json as json,
    js_json::{self, JsString, JsValue},
};
use pi_tui::{
    Text,
    tui::{Component, ComponentRef},
};
use std::{cell::RefCell, error::Error, rc::Rc, sync::Arc};

struct Other;
impl Component for Other {
    fn render(&mut self, _: usize) -> Vec<JsString> {
        vec!["other".into()]
    }
}

#[test]
fn question_results_update_call_answers_and_keep_retained_result_components_unchanged() {
    let raw = JsString::from_utf16(vec![0xd800]);
    let question = JsString::from("First-") + &raw;
    let questions = json!([{"question":question},{"question":"Second"}]);
    let mut answers = pi_js_compat::js_json::JsObject::new();
    answers.insert(question.clone(), JsValue::String(JsString::from("answer-") + &raw));
    answers.insert("Second".into(), JsValue::String(JsString::default()));
    let details = json!({"blocking":true,"questions":questions,"answers":JsValue::Object(answers),"cancelled":false,"metadata":{"raw":raw}});
    let cases = json!([
        {"kind":"result","last":"text","result":{"content":[],"details":{"blocking":true,"questions":[],"answers":{},"cancelled":false}}},
        {"kind":"call","args":{"blocking":true,"questions":questions}},
        {"kind":"result","result":{"content":[{"type":"text","text":"not a result row"}],"details":details}},
        {"kind":"call","args":null,"width":12},
        {"kind":"call","args":{"questions":[{"question":"Next?"}]}},
        {"kind":"result","result":{"content":[],"details":{"blocking":false,"queued":true,"answers":{},"cancelled":false}}},
        {"kind":"call","args":{"questions":questions}},
        {"kind":"result","result":{"content":[]}},
        {"kind":"result","last":"foreign","result":{"content":[]}},
        {"kind":"call","last":"foreign","args":{"questions":questions}},
        {"kind":"call","reset":true,"args":{"questions":questions}},
        {"kind":"result","result":{"content":[],"details":{"blocking":true,"answers":{"Second":"partial"},"cancelled":true}}},
        {"kind":"result","last":"none","result":{"content":[],"details":details}},
        {"kind":"call","last":"none","args":{"questions":[{"question":question}]}},
        {"kind":"result","last":"text","result":{"content":[],"details":details}},
        {"kind":"call","args":null}
    ]);
    let mut oracle = oracle::TsOracle::spawn_with_environment(
        "question-render-history-oracle.ts",
        "packages/coding-agent/src/core/tools/ask-questions.ts",
        &[("COLORTERM", "truecolor".into())],
    );
    let expected = oracle.request(&json!({"id":1,"cases":cases}));
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    assert_eq!(
        expected["output"][9]["message"],
        json!("text.setText is not a function")
    );
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("theme/dark.json");
    let theme = load_theme_from_path(
        &pi_js_compat::node_fs::decode_node_path(path.as_os_str()),
        Some(ColorMode::Truecolor),
    )
    .unwrap();
    let definition = create_ask_questions_tool_definition();
    let mut state = ToolRenderState::new();
    let mut call: Option<ComponentRef> = None;
    let mut result: Option<ComponentRef> = None;
    let mut args = JsValue::Null;
    for (index, step) in cases.as_array().unwrap().iter().enumerate() {
        if step["reset"].as_bool() == Some(true) {
            state = ToolRenderState::new();
            call = None;
            result = None;
            args = JsValue::Null;
        }
        let is_call = step["kind"] == json!("call");
        if is_call {
            args = step["args"].clone();
        }
        let previous = match step["last"].as_string() {
            Some(kind) if kind == "foreign" => Some(Rc::new(RefCell::new(Other)) as ComponentRef),
            Some(kind) if kind == "text" => {
                Some(Rc::new(RefCell::new(Text::new("retained result text", 0, 0))) as ComponentRef)
            }
            Some(kind) if kind == "none" => None,
            _ => {
                if is_call {
                    call.clone()
                } else {
                    result.clone()
                }
            }
        };
        let context = ToolRenderContext {
            args: args.clone(),
            tool_call_id: "fixture".into(),
            invalidate: Arc::new(|| {}),
            last_component: previous.clone(),
            call_component: call.clone(),
            state: state.clone(),
            cwd: "/repo".into(),
            execution_started: true,
            args_complete: true,
            is_partial: false,
            expanded: true,
            show_images: false,
            is_error: false,
        };
        let outcome = if is_call {
            definition.render_call.as_ref().unwrap()(&args, &theme, &context)
        } else {
            definition.render_result.as_ref().unwrap()(
                &js_json::from_value(&step["result"]).unwrap(),
                &ToolRenderResultOptions {
                    expanded: true,
                    is_partial: false,
                },
                &theme,
                &context,
            )
        };
        let actual = match outcome {
            Ok(component) => {
                let reused = previous
                    .as_ref()
                    .is_some_and(|previous| Rc::ptr_eq(previous, &component));
                if is_call {
                    call = Some(component.clone());
                } else {
                    result = Some(component.clone());
                }
                let width = step["width"].as_u64().unwrap_or(80) as usize;
                let lines = component.borrow_mut().render(width);
                let call_lines = call.as_ref().map(|call| call.borrow_mut().render(width));
                json!({"ok":true,"reused":reused,"lines":lines,"call":call_lines,"details":state.get()["details"]})
            }
            Err(error) => {
                let error = error
                    .source()
                    .and_then(|source| source.downcast_ref::<pi_js_compat::node_error::NativeNodeError>())
                    .expect("native renderer rejection");
                json!({"ok":false,"name":error.name,"message":error.message})
            }
        };
        assert_eq!(actual, expected["output"][index], "frame {index}");
    }
}
