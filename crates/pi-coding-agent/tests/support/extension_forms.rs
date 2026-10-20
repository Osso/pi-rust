#[path = "extensions.rs"]
mod extensions;
#[path = "oracle.rs"]
mod oracle;
#[path = "tool_context.rs"]
mod tool_context;
#[path = "../../../pi-tui/tests/support/virtual_terminal.rs"]
mod virtual_terminal;
use pi_coding_agent::core::extensions::*;
use pi_coding_agent::core::keybindings::KeybindingsManager;
use pi_coding_agent::modes::interactive::theme::{ColorMode, ColorValue, REQUIRED_COLORS, Theme};
use pi_tui::components::editor::EditorFixedCell;
use pi_tui::tui::{FixedCell, TuiMode, TuiRenderer};
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
    fn update(&self, value: &str) -> bool {
        self.cell.update(value)
    }
    fn clear(&self) {
        self.cell.clear();
    }
}
struct Backend;
impl ExtensionUiTuiBackend for Backend {
    fn create_fixed_cell(&self) -> Rc<dyn EditorFixedCell> {
        let tui = TuiRenderer::new(virtual_terminal::concrete(80, 24).1);
        let cell = tui.create_fixed_cell();
        Rc::new(OwnedCell { _tui: tui, cell })
    }
    fn request_render(&self) {}
    fn terminal_rows(&self) -> usize {
        24
    }
    fn terminal_columns(&self) -> usize {
        80
    }
    fn mode(&self) -> TuiMode {
        TuiMode::Regular
    }
    fn show_overlay(
        &self,
        _: Box<dyn ExtensionUiComponent>,
        _: Option<ExtensionUiOverlayOptions>,
    ) -> Arc<dyn ExtensionUiOverlayHandle> {
        panic!("forms do not create nested overlays")
    }
}
struct Ui {
    theme: Arc<Theme>,
    request: Value,
    renders: Mutex<Vec<Value>>,
}
impl ExtensionUiContext for Ui {
    fn theme(&self) -> Arc<Theme> {
        self.theme.clone()
    }
    fn custom<'a>(
        &'a self,
        factory: ExtensionUiCustomFactory,
        _: Option<ExtensionUiCustomOptions>,
    ) -> BoxFuture<'a, Result<Option<Value>, String>> {
        let result = Rc::new(std::cell::RefCell::new(None));
        let captured = result.clone();
        let component = factory(
            ExtensionUiFactoryContext {
                tui: ExtensionUiTui::new(Arc::new(Backend)),
                theme: self.theme.clone(),
                keybindings: KeybindingsManager::new(Vec::new(), None),
            },
            ExtensionUiDone::new(Arc::new(move |value| *captured.borrow_mut() = Some(value))),
        );
        let record = || {
            json!(
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
                component.handle_input(input);
            }
            if let Some(units) = step["inputUnits"].as_array() {
                let units = units.iter().map(|u| u.as_u64().unwrap() as u16).collect::<Vec<_>>();
                component.handle_input(&String::from_utf16_lossy(&units));
            }
            self.renders.lock().unwrap().push(record());
        }
        let value = result.borrow().clone().flatten();
        component.dispose();
        Box::pin(async move { Ok(value) })
    }
}
pub async fn compare(name: &str, register: fn(&ExtensionApi) -> Result<(), HandlerError>, mut request: Value) -> Value {
    let colors: Vec<_> = REQUIRED_COLORS
        .iter()
        .enumerate()
        .map(|(i, k)| {
            (
                k.to_string(),
                ColorValue::Text(format!(
                    "#{:02x}{:02x}{:02x}",
                    (16 + 7 * i) % 256,
                    (32 + 11 * i) % 256,
                    (48 + 13 * i) % 256
                )),
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
    let theme = Arc::new(Theme::new(&colors, &colors, ColorMode::Truecolor, None, None).unwrap());
    let runtime = create_extension_runtime();
    let extension = load_extension_from_factory(
        Arc::new(move |api| {
            let result = register(api);
            Box::pin(async move { result })
        }),
        "/fixture",
        extensions::event_bus(),
        runtime,
        name,
    )
    .await
    .unwrap();
    let tool = extension.read().tools.get(name).unwrap().definition.clone();
    let ui = Arc::new(Ui {
        theme: theme.clone(),
        request: request.clone(),
        renders: Mutex::new(Vec::new()),
    });
    let (_runner, context) =
        tool_context::TestContextBuilder::new("/fixture", tool_context::in_memory_session("/fixture", name))
            .mode(serde_json::from_value(request.get("mode").cloned().unwrap_or(json!("tui"))).unwrap())
            .ui(ui.clone())
            .build()
            .await;
    let result = (tool.execute)("fixture".into(), request["params"].clone(), None, None, context.clone())
        .await
        .unwrap();
    let render_context = ToolRenderContext {
        args: request["params"].clone(),
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
    let call = tool.render_call.as_ref().unwrap()(&request["params"], &theme, &render_context).unwrap();
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
    let mut fallback_result = pi_agent_core::AgentToolResult::text("fallback");
    fallback_result.details = None;
    let fallback = tool.render_result.as_ref().unwrap()(&fallback_result, &opts, &theme, &render_context)
        .unwrap()
        .borrow_mut()
        .render(80);
    let actual = json!({"metadata":{"name":tool.name,"label":tool.label,"description":tool.description,"parameters":tool.parameters,"executionMode":tool.execution_mode},"result":result,"renders":*ui.renders.lock().unwrap(),"renderCall":render_call,"renderResult":render_result,"fallback":fallback});
    let mut oracle = oracle::TsOracle::spawn(
        "example-forms-oracle.ts",
        &format!("packages/coding-agent/examples/extensions/{name}.ts"),
    );
    let expected = oracle.request(&request);
    assert_eq!(expected["ok"], true, "{expected}");
    assert_equal(&actual, &expected["output"], name);
    actual
}
fn assert_equal(actual: &Value, expected: &Value, path: &str) {
    match (actual, expected) {
        (Value::Object(a), Value::Object(b)) if a.keys().eq(b.keys()) => {
            for (k, v) in a {
                assert_equal(v, &b[k], &format!("{path}/{k}"));
            }
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                assert_equal(a, b, &format!("{path}/{i}"));
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}
