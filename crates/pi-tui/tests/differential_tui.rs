#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/virtual_terminal.rs"]
mod virtual_terminal;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use oracle::TsOracle;
use pi_tui::tui::{
    Component, ComponentRef, FixedCell, Focusable, OverlayAnchor, OverlayHandle, OverlayMargin, OverlayOptions,
    SizeValue, TuiExt, TuiRenderer, component,
};
use pi_tui::tui_main_screen::TuiMainScreen;
use serde_json::{Value, json};
use virtual_terminal::concrete;

type FocusFlag = Option<Rc<Cell<bool>>>;
type FocusFlags = Rc<RefCell<Vec<FocusFlag>>>;

struct ScenarioState {
    lines: Vec<String>,
    renders: usize,
    inputs: Vec<String>,
}

struct ScenarioComponent {
    state: Rc<RefCell<ScenarioState>>,
    focus: Option<Rc<Cell<bool>>>,
    focus_on_input: Vec<(String, usize)>,
    components: Rc<RefCell<Vec<ComponentRef>>>,
    set_focus: Rc<dyn Fn(Option<ComponentRef>)>,
}

impl Component for ScenarioComponent {
    fn render(&mut self, _width: usize) -> Vec<String> {
        let mut state = self.state.borrow_mut();
        state.renders += 1;
        state.lines.clone()
    }

    fn handle_input(&mut self, data: &str) {
        self.state.borrow_mut().inputs.push(data.to_string());
        if let Some((_, target)) = self.focus_on_input.iter().find(|(key, _)| key == data) {
            let component = self.components.borrow().get(*target).cloned();
            (self.set_focus)(component);
        }
    }

    fn wants_input(&self) -> bool {
        true
    }

    fn as_focusable_mut(&mut self) -> Option<&mut dyn Focusable> {
        if self.focus.is_some() { Some(self) } else { None }
    }
}

impl Focusable for ScenarioComponent {
    fn focused(&self) -> bool {
        self.focus.as_ref().is_some_and(|focus| focus.get())
    }

    fn set_focused(&mut self, focused: bool) {
        if let Some(flag) = &self.focus {
            flag.set(focused);
        }
    }
}

enum TuiHandle {
    Main(Rc<TuiMainScreen>),
    Renderer(Rc<TuiRenderer>),
}

impl TuiHandle {
    fn add_child(&self, component: ComponentRef) {
        match self {
            TuiHandle::Main(tui) => tui.add_child(component),
            TuiHandle::Renderer(tui) => tui.add_child(component),
        }
    }

    fn start(&self) {
        match self {
            TuiHandle::Main(tui) => tui.start().unwrap(),
            TuiHandle::Renderer(tui) => tui.start().unwrap(),
        }
    }

    fn stop(&self) {
        match self {
            TuiHandle::Main(tui) => tui.stop(),
            TuiHandle::Renderer(tui) => tui.stop(),
        }
    }

    fn wait(&self) {
        match self {
            TuiHandle::Main(tui) => tui.flush_pending(),
            TuiHandle::Renderer(tui) => tui.flush_pending(),
        }
    }

    fn process_events(&self) {
        match self {
            TuiHandle::Main(tui) => tui.process_events(),
            TuiHandle::Renderer(tui) => tui.process_events(),
        }
    }

    fn render_now(&self, force: bool) {
        match self {
            TuiHandle::Main(tui) => tui.render_now(force),
            TuiHandle::Renderer(tui) => tui.render_now(force),
        }
    }

    fn request_render(&self, force: bool) {
        match self {
            TuiHandle::Main(tui) => tui.request_render(force),
            TuiHandle::Renderer(tui) => tui.request_render(force),
        }
    }

    fn set_focus(&self, component: Option<ComponentRef>) {
        match self {
            TuiHandle::Main(tui) => tui.set_focus(component),
            TuiHandle::Renderer(tui) => tui.set_focus(component),
        }
    }

    fn show_overlay(&self, component: ComponentRef, options: OverlayOptions) -> OverlayHandle {
        match self {
            TuiHandle::Main(tui) => tui.show_overlay(component, Some(options)),
            TuiHandle::Renderer(tui) => tui.show_overlay(component, Some(options)),
        }
    }

    fn hide_overlay(&self) {
        match self {
            TuiHandle::Main(tui) => tui.hide_overlay(),
            TuiHandle::Renderer(tui) => tui.hide_overlay(),
        }
    }

    fn create_fixed_cell(&self) -> Option<FixedCell> {
        match self {
            TuiHandle::Main(_) => None,
            TuiHandle::Renderer(tui) => Some(tui.create_fixed_cell()),
        }
    }

    fn full_redraws(&self) -> usize {
        match self {
            TuiHandle::Main(tui) => tui.full_redraws(),
            TuiHandle::Renderer(tui) => tui.full_redraws(),
        }
    }

    fn set_show_hardware_cursor(&self, enabled: bool) {
        match self {
            TuiHandle::Main(tui) => tui.set_show_hardware_cursor(enabled),
            TuiHandle::Renderer(tui) => tui.set_show_hardware_cursor(enabled),
        }
    }

    fn set_clear_on_shrink(&self, enabled: bool) {
        match self {
            TuiHandle::Main(tui) => tui.set_clear_on_shrink(enabled),
            TuiHandle::Renderer(tui) => tui.set_clear_on_shrink(enabled),
        }
    }

    fn set_termux_session(&self, enabled: bool) {
        match self {
            TuiHandle::Main(tui) => tui.set_termux_session(enabled),
            TuiHandle::Renderer(tui) => tui.set_termux_session(enabled),
        }
    }

    fn focus_setter(self: &Rc<Self>) -> Rc<dyn Fn(Option<ComponentRef>)> {
        let handle = Rc::clone(self);
        Rc::new(move |component| handle.set_focus(component))
    }
}

fn size_value(value: &Value) -> Option<SizeValue> {
    match value {
        Value::Number(number) => Some(SizeValue::Absolute(number.as_f64().unwrap_or(0.0))),
        Value::String(text) => SizeValue::parse_percent(text),
        _ => None,
    }
}

fn overlay_options(value: &Value) -> OverlayOptions {
    let mut options = OverlayOptions::default();
    let Some(object) = value.as_object() else {
        return options;
    };
    options.width = object.get("width").and_then(size_value);
    options.min_width = object.get("minWidth").and_then(Value::as_i64);
    options.max_height = object.get("maxHeight").and_then(size_value);
    options.anchor = object.get("anchor").and_then(Value::as_str).map(|anchor| match anchor {
        "top-left" => OverlayAnchor::TopLeft,
        "top-right" => OverlayAnchor::TopRight,
        "bottom-left" => OverlayAnchor::BottomLeft,
        "bottom-right" => OverlayAnchor::BottomRight,
        "top-center" => OverlayAnchor::TopCenter,
        "bottom-center" => OverlayAnchor::BottomCenter,
        "left-center" => OverlayAnchor::LeftCenter,
        "right-center" => OverlayAnchor::RightCenter,
        _ => OverlayAnchor::Center,
    });
    options.offset_x = object.get("offsetX").and_then(Value::as_i64);
    options.offset_y = object.get("offsetY").and_then(Value::as_i64);
    options.row = object.get("row").and_then(size_value);
    options.col = object.get("col").and_then(size_value);
    options.non_capturing = object.get("nonCapturing").and_then(Value::as_bool).unwrap_or(false);
    options.margin = match object.get("margin") {
        Some(Value::Number(number)) => {
            let value = number.as_i64().unwrap_or(0);
            Some(OverlayMargin {
                top: Some(value),
                right: Some(value),
                bottom: Some(value),
                left: Some(value),
            })
        }
        Some(Value::Object(margin)) => Some(OverlayMargin {
            top: margin.get("top").and_then(Value::as_i64),
            right: margin.get("right").and_then(Value::as_i64),
            bottom: margin.get("bottom").and_then(Value::as_i64),
            left: margin.get("left").and_then(Value::as_i64),
        }),
        _ => None,
    };
    options
}

fn run_scenario(scenario: &Value) -> Value {
    let kind = scenario["kind"].as_str().unwrap();
    let columns = scenario["columns"].as_u64().unwrap() as usize;
    let rows = scenario["rows"].as_u64().unwrap() as usize;
    let (terminal, handle) = concrete(columns, rows);
    terminal.borrow_mut().set_capture_cursor_writes(true);
    let tui = if kind == "renderer" {
        TuiHandle::Renderer(TuiRenderer::new(handle))
    } else {
        TuiHandle::Main(TuiMainScreen::new(handle))
    };
    if let Some(enabled) = scenario.get("showHardwareCursor").and_then(Value::as_bool) {
        tui.set_show_hardware_cursor(enabled);
    }
    if let Some(enabled) = scenario.get("clearOnShrink").and_then(Value::as_bool) {
        tui.set_clear_on_shrink(enabled);
    }
    if let Some(enabled) = scenario.get("termux").and_then(Value::as_bool) {
        tui.set_termux_session(enabled);
    }
    let tui = Rc::new(tui);
    let set_focus = tui.focus_setter();
    let components: Rc<RefCell<Vec<ComponentRef>>> = Rc::new(RefCell::new(Vec::new()));
    let states: Rc<RefCell<Vec<Rc<RefCell<ScenarioState>>>>> = Rc::new(RefCell::new(Vec::new()));
    let focus_flags: FocusFlags = Rc::new(RefCell::new(Vec::new()));
    let mut overlay_handles: Vec<OverlayHandle> = Vec::new();
    let mut fixed_cells: Vec<FixedCell> = Vec::new();

    for step in scenario["steps"].as_array().unwrap() {
        match step["kind"].as_str().unwrap() {
            "add" => {
                let state = Rc::new(RefCell::new(ScenarioState {
                    lines: step["lines"]
                        .as_array()
                        .map(|lines| {
                            lines
                                .iter()
                                .map(|line| line.as_str().unwrap_or("").to_string())
                                .collect()
                        })
                        .unwrap_or_default(),
                    renders: 0,
                    inputs: Vec::new(),
                }));
                let focusable = step["focusable"].as_bool().unwrap_or(false);
                let focus = focusable.then(|| Rc::new(Cell::new(false)));
                let focus_on_input = step["focusOnInput"]
                    .as_array()
                    .map(|bindings| {
                        bindings
                            .iter()
                            .map(|binding| {
                                (
                                    binding[0].as_str().unwrap().to_string(),
                                    binding[1].as_u64().unwrap() as usize,
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let component: ComponentRef = component(ScenarioComponent {
                    state: Rc::clone(&state),
                    focus: focus.clone(),
                    focus_on_input,
                    components: Rc::clone(&components),
                    set_focus: Rc::clone(&set_focus),
                });
                components.borrow_mut().push(component.clone());
                states.borrow_mut().push(state);
                focus_flags.borrow_mut().push(focus);
                tui.add_child(component);
            }
            "start" => tui.start(),
            "wait" => tui.wait(),
            "render_now" => tui.render_now(step["force"].as_bool().unwrap_or(false)),
            "request_render" => tui.request_render(step["force"].as_bool().unwrap_or(false)),
            "set_lines" => {
                let index = step["component"].as_u64().unwrap_or(0) as usize;
                let lines: Vec<String> = step["lines"]
                    .as_array()
                    .map(|lines| {
                        lines
                            .iter()
                            .map(|line| line.as_str().unwrap_or("").to_string())
                            .collect()
                    })
                    .unwrap_or_default();
                states.borrow()[index].borrow_mut().lines = lines;
            }
            "send_input" => {
                terminal.borrow_mut().send_input(step["data"].as_str().unwrap_or(""));
                tui.process_events();
            }
            "focus" => {
                let component = step["component"]
                    .as_u64()
                    .map(|index| components.borrow()[index as usize].clone());
                tui.set_focus(component);
            }
            "resize" => {
                terminal.borrow_mut().resize(
                    step["columns"].as_u64().unwrap_or(columns as u64) as usize,
                    step["rows"].as_u64().unwrap_or(rows as u64) as usize,
                );
            }
            "overlay" => {
                let index = step["component"].as_u64().unwrap_or(0) as usize;
                let component = components.borrow()[index].clone();
                let options = overlay_options(&step["options"]);
                overlay_handles.push(tui.show_overlay(component, options));
            }
            "hide_overlay" => tui.hide_overlay(),
            "overlay_op" => {
                let index = step["index"].as_u64().unwrap_or(0) as usize;
                if step["operation"].as_str() == Some("hide") {
                    overlay_handles[index].hide();
                }
            }
            "fixed_cell_create" => {
                if let Some(cell) = tui.create_fixed_cell() {
                    fixed_cells.push(cell);
                }
            }
            "fixed_cell_place" => {
                let index = step["index"].as_u64().unwrap_or(0) as usize;
                let place = step["place"].as_array().unwrap();
                fixed_cells[index].place(place[0].as_i64().unwrap_or(0), place[1].as_i64().unwrap_or(0));
            }
            "fixed_cell_update" => {
                let index = step["index"].as_u64().unwrap_or(0) as usize;
                fixed_cells[index].update(step["update"].as_str().unwrap_or(""));
            }
            "stop" => tui.stop(),
            "snapshot" => {}
            other => panic!("unsupported scenario step: {other}"),
        }
    }

    let writes = terminal.borrow().writes();
    let states = states.borrow();
    let focus_flags = focus_flags.borrow();
    let component_output: Vec<Value> = states
        .iter()
        .enumerate()
        .map(|(index, state)| {
            let state = state.borrow();
            json!({
                "renders": state.renders,
                "inputs": state.inputs,
                "focused": focus_flags[index]
                    .as_ref()
                    .is_some_and(|flag| flag.get()),
            })
        })
        .collect();
    let focus = focus_flags
        .iter()
        .position(|flag| flag.as_ref().is_some_and(|flag| flag.get()))
        .map(|index| index as i64)
        .unwrap_or(-1);
    json!({
        "writes": writes,
        "fullRedraws": tui.full_redraws(),
        "components": component_output,
        "focus": focus,
    })
}

fn assert_scenario(oracle: &mut TsOracle, scenario: Value) {
    let mut request = json!({
        "id": "tui",
        "operation": "tui_scenario",
        "kind": scenario["kind"],
        "columns": scenario["columns"],
        "rows": scenario["rows"],
        "steps": scenario["steps"],
    });
    for key in ["showHardwareCursor", "clearOnShrink", "termux"] {
        if let Some(value) = scenario.get(key) {
            request[key] = value.clone();
        }
    }
    let response = oracle.request(&request);
    assert_eq!(response["ok"], Value::Bool(true), "oracle failed: {response}");
    let actual = run_scenario(&scenario);
    let expected = &response["output"];
    if actual["writes"] != expected["writes"] {
        let actual_writes = actual["writes"].as_str().unwrap_or("");
        let expected_writes = expected["writes"].as_str().unwrap_or("");
        let mismatch = actual_writes
            .bytes()
            .zip(expected_writes.bytes())
            .position(|(left, right)| left != right)
            .unwrap_or(actual_writes.len().min(expected_writes.len()));
        panic!(
            "write stream differs at byte {mismatch}\nscenario: {scenario}\nrust: {:?}\nts:   {:?}",
            &actual_writes[mismatch.saturating_sub(40)..(mismatch + 80).min(actual_writes.len())],
            &expected_writes[mismatch.saturating_sub(40)..(mismatch + 80).min(expected_writes.len())],
        );
    }
    assert_eq!(actual["fullRedraws"], expected["fullRedraws"], "scenario: {scenario}");
    assert_eq!(actual["components"], expected["components"], "scenario: {scenario}");
    assert_eq!(actual["focus"], expected["focus"], "scenario: {scenario}");
}

#[test]
fn tui_render_scenarios_match_typescript_byte_streams() {
    let mut oracle = TsOracle::spawn("tui-core-oracle.ts", "packages/tui/src");
    let scenarios = vec![
        json!({
            "kind": "main", "columns": 20, "rows": 5,
            "steps": [
                {"kind": "add", "lines": ["one", "two", "three"]},
                {"kind": "start"}, {"kind": "wait"},
                {"kind": "set_lines", "component": 0, "lines": ["one", "CHANGED", "three"]},
                {"kind": "request_render"}, {"kind": "wait"},
                {"kind": "set_lines", "component": 0, "lines": ["one", "CHANGED", "three", "four"]},
                {"kind": "request_render"}, {"kind": "wait"},
                {"kind": "set_lines", "component": 0, "lines": ["one"]},
                {"kind": "request_render"}, {"kind": "wait"},
                {"kind": "stop"}
            ]
        }),
        json!({
            "kind": "renderer", "columns": 24, "rows": 6,
            "steps": [
                {"kind": "add", "lines": ["header", "body", "footer"]},
                {"kind": "start"}, {"kind": "wait"},
                {"kind": "render_now", "force": true}, {"kind": "wait"},
                {"kind": "resize", "columns": 30, "rows": 8}, {"kind": "wait"},
                {"kind": "set_lines", "component": 0, "lines": ["header", "body two", "footer", "extra"]},
                {"kind": "request_render"}, {"kind": "wait"},
                {"kind": "render_now", "force": true}, {"kind": "wait"},
                {"kind": "stop"}
            ]
        }),
        json!({
            "kind": "main", "columns": 20, "rows": 4, "showHardwareCursor": true,
            "steps": [
                {"kind": "add", "lines": ["editor \u{1b}_pi:c\u{7}", "footer"]},
                {"kind": "start"}, {"kind": "wait"},
                {"kind": "set_lines", "component": 0, "lines": ["longer editor \u{1b}_pi:c\u{7}", "footer"]},
                {"kind": "request_render"}, {"kind": "wait"},
                {"kind": "stop"}
            ]
        }),
        json!({
            "kind": "main", "columns": 30, "rows": 8,
            "steps": [
                {"kind": "add", "lines": []},
                {"kind": "add", "lines": ["OVERLAY"]},
                {"kind": "add", "lines": ["SECOND"]},
                {"kind": "start"}, {"kind": "wait"},
                {"kind": "overlay", "component": 1, "options": {"width": 12, "anchor": "center"}},
                {"kind": "request_render"}, {"kind": "wait"},
                {"kind": "overlay", "component": 2, "options": {"width": 8, "row": 1, "col": 2}},
                {"kind": "request_render"}, {"kind": "wait"},
                {"kind": "overlay_op", "index": 1, "operation": "hide"},
                {"kind": "request_render"}, {"kind": "wait"},
                {"kind": "hide_overlay"}, {"kind": "request_render"}, {"kind": "wait"},
                {"kind": "stop"}
            ]
        }),
        json!({
            "kind": "main", "columns": 20, "rows": 5,
            "steps": [
                {"kind": "add", "lines": ["initial"], "focusable": true},
                {"kind": "start"}, {"kind": "wait"},
                {"kind": "focus", "component": 0},
                {"kind": "send_input", "data": "\u{1b}[O"}, {"kind": "wait"},
                {"kind": "set_lines", "component": 0, "lines": ["hidden update"]},
                {"kind": "request_render"}, {"kind": "wait"},
                {"kind": "send_input", "data": "\u{1b}[I"}, {"kind": "wait"},
                {"kind": "send_input", "data": "x"}, {"kind": "wait"},
                {"kind": "stop"}
            ]
        }),
        json!({
            "kind": "renderer", "columns": 20, "rows": 5,
            "steps": [
                {"kind": "add", "lines": ["body", "\u{203a} prompt", "footer"]},
                {"kind": "start"}, {"kind": "wait"},
                {"kind": "fixed_cell_create"},
                {"kind": "fixed_cell_place", "place": [1, 0]},
                {"kind": "fixed_cell_update", "update": "\u{280b}"},
                {"kind": "wait"},
                {"kind": "set_lines", "component": 0, "lines": ["body", "\u{280b} prompt", "footer"]},
                {"kind": "request_render"}, {"kind": "wait"},
                {"kind": "fixed_cell_place", "place": [2, 0]},
                {"kind": "fixed_cell_update", "update": "X"},
                {"kind": "wait"},
                {"kind": "stop"}
            ]
        }),
        json!({
            "kind": "main", "columns": 20, "rows": 5, "clearOnShrink": true,
            "steps": [
                {"kind": "add", "lines": ["0", "1", "2", "3", "4", "5", "6", "7"]},
                {"kind": "start"}, {"kind": "wait"},
                {"kind": "set_lines", "component": 0, "lines": ["0", "1"]},
                {"kind": "request_render"}, {"kind": "wait"},
                {"kind": "set_lines", "component": 0, "lines": ["0", "1", "2"]},
                {"kind": "request_render"}, {"kind": "wait"},
                {"kind": "stop"}
            ]
        }),
        json!({
            "kind": "main", "columns": 20, "rows": 5,
            "steps": [
                {"kind": "add", "lines": ["first"], "focusable": true},
                {"kind": "add", "lines": ["replacement"], "focusable": true},
                {"kind": "start"}, {"kind": "wait"},
                {"kind": "focus", "component": 0},
                {"kind": "overlay", "component": 1, "options": {"width": 15}},
                {"kind": "request_render"}, {"kind": "wait"},
                {"kind": "send_input", "data": "b"}, {"kind": "wait"},
                {"kind": "send_input", "data": "\r"}, {"kind": "wait"},
                {"kind": "stop"}
            ]
        }),
        json!({
            "kind": "renderer", "columns": 20, "rows": 5,
            "steps": [
                {"kind": "add", "lines": ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11"]},
                {"kind": "start"}, {"kind": "wait"},
                {"kind": "set_lines", "component": 0, "lines": ["0", "1", "2", "3", "4", "5", "6"]},
                {"kind": "request_render"}, {"kind": "wait"},
                {"kind": "stop"}
            ]
        }),
        json!({
            "kind": "main", "columns": 20, "rows": 10, "termux": true,
            "steps": [
                {"kind": "add", "lines": ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15", "16", "17", "18", "19"]},
                {"kind": "start"}, {"kind": "wait"},
                {"kind": "resize", "rows": 15}, {"kind": "wait"},
                {"kind": "resize", "rows": 8}, {"kind": "wait"},
                {"kind": "resize", "rows": 14}, {"kind": "wait"},
                {"kind": "resize", "rows": 11}, {"kind": "wait"},
                {"kind": "stop"}
            ]
        }),
        json!({
            "kind": "main", "columns": 40, "rows": 12,
            "steps": [
                {"kind": "add", "lines": []},
                {"kind": "add", "lines": ["L1", "L2", "L3", "L4", "L5", "L6"]},
                {"kind": "add", "lines": ["EDGE"]},
                {"kind": "start"}, {"kind": "wait"},
                {"kind": "overlay", "component": 1, "options": {"width": "50%", "maxHeight": "50%", "margin": 2, "nonCapturing": true}},
                {"kind": "request_render"}, {"kind": "wait"},
                {"kind": "overlay", "component": 2, "options": {"width": 8, "row": "100%", "col": "50%", "offsetY": -1, "anchor": "top-right"}},
                {"kind": "request_render"}, {"kind": "wait"},
                {"kind": "stop"}
            ]
        }),
        json!({
            "kind": "main", "columns": 24, "rows": 6,
            "steps": [
                {"kind": "add", "lines": ["EDITOR"], "focusable": true},
                {"kind": "add", "lines": ["OVERLAY"], "focusable": true, "focusOnInput": [["b", 0]]},
                {"kind": "start"}, {"kind": "wait"},
                {"kind": "focus", "component": 0},
                {"kind": "overlay", "component": 1, "options": {"width": 12}},
                {"kind": "request_render"}, {"kind": "wait"},
                {"kind": "send_input", "data": "b"}, {"kind": "wait"},
                {"kind": "send_input", "data": "x"}, {"kind": "wait"},
                {"kind": "stop"}
            ]
        }),
        json!({
            "kind": "main", "columns": 20, "rows": 5,
            "steps": [
                {"kind": "add", "lines": ["a"]},
                {"kind": "start"}, {"kind": "wait"},
                {"kind": "start"}, {"kind": "wait"},
                {"kind": "stop"}, {"kind": "stop"}
            ]
        }),
    ];
    for scenario in scenarios {
        assert_scenario(&mut oracle, scenario);
    }
}
