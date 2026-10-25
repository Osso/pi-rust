use crate::shared_extensions as extensions;
use crate::shared_oracle as oracle;
use crate::shared_tool_context as tool_context;
use crate::shared_tui_virtual_terminal as virtual_terminal;
use futures_util::FutureExt;
use pi_coding_agent::core::extensions::*;
use pi_coding_agent::core::keybindings::KeybindingsManager;
use pi_coding_agent::modes::interactive::theme::{ColorMode, ColorValue, REQUIRED_COLORS, Theme};
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use pi_tui::components::editor::EditorFixedCell;
use pi_tui::tui::{FixedCell, TuiRenderer};
use serde_json::{Map, Value, json};
use std::{
    rc::Rc,
    sync::{Arc, Mutex},
};

struct OwnedCell {
    _tui: Rc<TuiRenderer>,
    cell: FixedCell,
}
impl EditorFixedCell for OwnedCell {
    fn place(&self, row: i64, col: i64) {
        self.cell.place(row, col);
    }
    fn update(&self, value: &JsString) -> bool {
        self.cell.update(value)
    }
    fn clear(&self) {
        self.cell.clear();
    }
}
struct Backend(Arc<std::sync::atomic::AtomicUsize>);
impl ExtensionUiTuiBackend for Backend {
    fn request_component_render(&self, _identity: usize) {}
    fn request_render_force(&self) {
        panic!("fixture does not force redraw")
    }
    fn stop(&self) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        panic!("fixture does not release terminal")
    }
    fn start(&self) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        panic!("fixture does not acquire terminal")
    }

    fn create_fixed_cell(&self) -> Rc<dyn EditorFixedCell> {
        let tui = TuiRenderer::new(virtual_terminal::concrete(80, 24).1);
        let cell = tui.create_fixed_cell();
        Rc::new(OwnedCell { _tui: tui, cell })
    }
    fn request_render(&self) {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
    fn terminal_rows(&self) -> usize {
        24
    }
    fn terminal_columns(&self) -> usize {
        80
    }

    fn show_overlay(
        &self,
        _: Box<dyn ExtensionUiComponent>,
        _: Option<ExtensionUiOverlayOptions>,
    ) -> Rc<dyn ExtensionUiOverlayHandle> {
        panic!("forms do not create nested overlays")
    }
}
struct Ui {
    theme: Arc<Theme>,
    request: Value,
    renders: Mutex<Vec<JsValue>>,
    render_requests: Arc<std::sync::atomic::AtomicUsize>,
}
impl ExtensionUiContext for Ui {
    fn theme(&self) -> Result<Arc<Theme>, pi_ai::request::CallbackError> {
        Ok(self.theme.clone())
    }
    fn custom<'a>(
        &'a self,
        factory: ExtensionUiCustomFactory,
        _: Option<ExtensionUiCustomOptions>,
    ) -> BoxFuture<'a, Result<Option<JsValue>, pi_js_compat::callback_error::CallbackError>> {
        let result = Rc::new(std::cell::RefCell::new(None));
        let captured = result.clone();
        let component = factory(
            ExtensionUiFactoryContext {
                tui: ExtensionUiTui::new(Arc::new(Backend(self.render_requests.clone()))),
                theme: self.theme.clone(),
                keybindings: KeybindingsManager::new(Vec::new(), None),
            },
            ExtensionUiDone::new(Rc::new(move |value| *captured.borrow_mut() = Some(value))),
        )
        .expect("custom component fixture factory")
        .now_or_never()
        .expect("fixture factory must resolve immediately")
        .expect("custom component fixture factory");
        let record = || {
            pi_js_compat::js_json!(
                self.request["widths"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|w| component.render(w.as_u64().unwrap() as usize))
                    .collect::<Vec<_>>()
            )
        };
        self.renders.lock().unwrap().push(record());
        for step in self.request["steps"].as_array().into_iter().flatten() {
            if result.borrow().is_some() {
                break;
            }
            if step["invalidate"] == true {
                component.invalidate();
            }
            if let Some(input) = step["input"].as_str() {
                component.handle_input(&JsString::from(input));
            }
            if let Some(units) = step["inputUnits"].as_array() {
                let units = units.iter().map(|u| u.as_u64().unwrap() as u16).collect::<Vec<_>>();
                component.handle_input(&JsString::from_utf16(units));
            }
            self.renders.lock().unwrap().push(record());
        }
        let value = result.borrow().clone().flatten();
        component.dispose();
        Box::pin(async move { Ok(value) })
    }
}
pub async fn compare(
    name: &str,
    register: fn(&ExtensionApi) -> Result<(), HandlerError>,
    mut request: Value,
) -> JsValue {
    let colors: Vec<_> = REQUIRED_COLORS
        .iter()
        .enumerate()
        .map(|(i, k)| {
            (
                JsString::from(*k),
                ColorValue::Text(JsString::from(format!(
                    "#{:02x}{:02x}{:02x}",
                    (16 + 7 * i) % 256,
                    (32 + 11 * i) % 256,
                    (48 + 13 * i) % 256
                ))),
            )
        })
        .collect();
    request["colors"] = json!(
        REQUIRED_COLORS
            .iter()
            .enumerate()
            .map(|(i, k)| (
                k.to_string(),
                json!([(16 + 7 * i) % 256, (32 + 11 * i) % 256, (48 + 13 * i) % 256])
            ))
            .collect::<Map<_, _>>()
    );
    if let Some(expected_error) = request.get("sourceError") {
        let mut oracle = oracle::TsOracle::spawn(
            "example-forms-oracle.ts",
            &format!("packages/coding-agent/examples/extensions/{name}.ts"),
        );
        let response = oracle.request(&request);
        assert_eq!(response["ok"], false, "{response}");
        assert_eq!(&response["error"], expected_error);
        return JsValue::from(response);
    }
    let theme = Arc::new(Theme::new(&colors, &colors, ColorMode::Truecolor, None, None).unwrap());
    let runtime = create_extension_runtime();
    let extension = load_extension_from_factory(
        Arc::new(move |api| {
            let result = register(api);
            Box::pin(async move { result })
        }),
        &JsString::from("/fixture"),
        extensions::event_bus(),
        runtime,
        &JsString::from(name),
    )
    .await
    .unwrap();
    let tool = extension.read().tools.get(name).unwrap().definition.clone();
    let ui = Arc::new(Ui {
        theme: theme.clone(),
        request: request.clone(),
        renders: Mutex::new(Vec::new()),
        render_requests: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
    });
    let (_runner, context) =
        tool_context::TestContextBuilder::new("/fixture", tool_context::in_memory_session("/fixture", name))
            .mode(serde_json::from_value(request.get("mode").cloned().unwrap_or(json!("tui"))).unwrap())
            .ui(ui.clone())
            .build()
            .await;
    let result = (tool.execute)(
        "fixture".into(),
        JsValue::from(request["params"].clone()),
        None,
        None,
        Some(context.clone()),
    )
    .await
    .unwrap_or_else(|error| panic!("{name}: {request}: {error:?}"));
    let render_context = ToolRenderContext {
        args: JsValue::from(request["params"].clone()),
        tool_call_id: "fixture".into(),
        invalidate: Arc::new(|| {}),
        last_component: None,
        call_component: None,
        state: ToolRenderState::new(),
        cwd: "/fixture".into(),
        execution_started: true,
        args_complete: true,
        is_partial: false,
        expanded: false,
        show_images: false,
        is_error: false,
    };
    let call =
        tool.render_call.as_ref().unwrap()(&JsValue::from(request["params"].clone()), &theme, &render_context).unwrap();
    let render_call = call
        .borrow_mut()
        .render(request["callWidth"].as_u64().unwrap_or(80) as usize);
    let opts = ToolRenderResultOptions {
        expanded: false,
        is_partial: false,
    };
    let output = tool.render_result.as_ref().unwrap()(&result, &opts, &theme, &render_context).unwrap();
    let render_result = output
        .borrow_mut()
        .render(request["callWidth"].as_u64().unwrap_or(80) as usize);
    let render_tool_result = |result: &pi_agent_core::AgentToolResult| {
        tool.render_result.as_ref().unwrap()(result, &opts, &theme, &render_context)
            .unwrap()
            .borrow_mut()
            .render(request["callWidth"].as_u64().unwrap_or(80) as usize)
    };
    let fallback = [
        json!([]),
        json!([{"type":"image","data":"AA==","mimeType":"image/png"}]),
        json!([{"type":"text","text":"fallback"}]),
    ]
    .into_iter()
    .map(|content| {
        let mut result = pi_agent_core::AgentToolResult::text("");
        result.details = None;
        result.content = serde_json::from_value(content).unwrap();
        render_tool_result(&result)
    })
    .collect::<Vec<_>>();
    let extra_results = request["renderCases"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|case| render_tool_result(&serde_json::from_value(case.clone()).unwrap()))
        .collect::<Vec<_>>();
    let actual = pi_js_compat::js_json!({"metadata":{"name":tool.name,"label":tool.label,"description":tool.description,"parameters":tool.parameters,"executionMode":tool.execution_mode.map(|mode| mode.as_str())},"result":result,"renders":*ui.renders.lock().unwrap(),"renderRequests":ui.render_requests.load(std::sync::atomic::Ordering::SeqCst),"renderCall":render_call,"renderResult":render_result,"fallback":fallback,"extraResults":extra_results});
    let mut oracle = oracle::TsOracle::spawn(
        "example-forms-oracle.ts",
        &format!("packages/coding-agent/examples/extensions/{name}.ts"),
    );
    let expected: JsValue = oracle.request(&JsValue::from(request));
    assert_eq!(expected["ok"].as_bool(), Some(true), "{expected:?}");
    assert_equal(&actual, &expected["output"], name);
    actual
}
fn assert_equal(actual: &JsValue, expected: &JsValue, path: &str) {
    match (actual, expected) {
        (JsValue::Object(a), JsValue::Object(b)) if a.keys().eq(b.keys()) => {
            for (k, v) in a {
                assert_equal(v, &b[k], &format!("{path}/{k:?}"));
            }
        }
        (JsValue::Array(a), JsValue::Array(b)) if a.len() == b.len() => {
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                assert_equal(a, b, &format!("{path}/{i}"));
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}
