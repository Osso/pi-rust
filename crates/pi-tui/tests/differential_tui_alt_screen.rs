//! Differential tests for `tui-alt-screen.ts` against the pinned TypeScript source.
//!
//! The oracle drives the real TypeScript alternate-screen TUI through a
//! capturing terminal; Rust drives [`TuiAltScreen`] with the same script and
//! compares the complete write stream plus viewport/scroll/search state.

#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/virtual_terminal.rs"]
mod virtual_terminal;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use oracle::TsOracle;
use pi_tui::components::h_stack::HStack;
use pi_tui::components::image::{Image, ImageOptions, ImageTheme};
use pi_tui::components::scroll_view::{ScrollFollow, ScrollState, ScrollView, ScrollViewOptions, ScrollViewScrollbar};
use pi_tui::components::stack::StackOptions;
use pi_tui::components::text::Text;
use pi_tui::components::v_stack::VStack;
use pi_tui::layout_node::{StackBasis, StackLayoutEntry};
use pi_tui::terminal_image::{
    ImageDimensions, ImageProtocol, KittyEncodeOptions, KittyImageMetadata, TerminalCapabilities, encode_kitty,
    register_kitty_image_metadata, reset_capabilities_cache, set_capabilities,
};
use pi_tui::tui::{Component, ComponentRef, Focusable, TuiExt, component};
use pi_tui::tui_alt_screen::{AltScreenEnvironment, TuiAltScreen, TuiAltScreenOptions};
use serde_json::{Value, json};
use virtual_terminal::concrete;

const SOURCE_DIRECTORY: &str = "packages/tui/src";

struct ScenarioComponent {
    lines: Vec<String>,
    inputs: Vec<String>,
    focus: Option<Rc<Cell<bool>>>,
}

impl Component for ScenarioComponent {
    fn render(&mut self, _width: usize) -> Vec<String> {
        self.lines.clone()
    }

    fn handle_input(&mut self, data: &str) {
        self.inputs.push(data.to_string());
    }

    fn wants_input(&self) -> bool {
        true
    }

    fn as_focusable_mut(&mut self) -> Option<&mut dyn Focusable> {
        self.focus.as_ref()?;
        Some(self)
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
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

fn json_lines(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|lines| {
            lines
                .iter()
                .map(|line| line.as_str().unwrap_or("").to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn scrollbar(value: &Value) -> ScrollViewScrollbar {
    match value.as_str() {
        Some("auto") => ScrollViewScrollbar::Auto,
        Some("always") => ScrollViewScrollbar::Always,
        _ => ScrollViewScrollbar::Hidden,
    }
}

fn stack_entry(components: &[ComponentRef], value: &Value) -> StackLayoutEntry {
    let component = components[value["component"].as_u64().unwrap_or(0) as usize].clone();
    let basis = match &value["basis"] {
        Value::Number(number) => Some(StackBasis::Cells(number.as_u64().unwrap_or(0) as usize)),
        Value::String(text) if text == "auto" => Some(StackBasis::Auto),
        _ => None,
    };
    StackLayoutEntry {
        component,
        basis,
        grow: value["grow"].as_u64().map(|grow| grow as usize),
        shrink: value["shrink"].as_u64().map(|shrink| shrink as usize),
        min_size: value["minSize"].as_u64().map(|size| size as usize),
        max_size: None,
        visible: None,
    }
}

fn environment_from_json(value: &Value) -> AltScreenEnvironment {
    let object = value.as_object();
    AltScreenEnvironment {
        term: object
            .and_then(|object| object.get("TERM"))
            .and_then(Value::as_str)
            .map(str::to_string),
        tmux: object.is_some_and(|object| object.contains_key("TMUX")),
        zellij: object.is_some_and(|object| object.contains_key("ZELLIJ")),
        sty: object.is_some_and(|object| object.contains_key("STY")),
        platform: object
            .and_then(|object| object.get("platform"))
            .and_then(Value::as_str)
            .unwrap_or(std::env::consts::OS)
            .to_string(),
    }
}

fn capabilities_from_name(name: &str) -> TerminalCapabilities {
    TerminalCapabilities {
        images: match name {
            "kitty" => Some(ImageProtocol::Kitty),
            "iterm2" => Some(ImageProtocol::Iterm2),
            _ => None,
        },
        true_color: true,
        hyperlinks: true,
    }
}

fn run_scenario(scenario: &Value) -> Value {
    let columns = scenario["columns"].as_u64().unwrap() as usize;
    let rows = scenario["rows"].as_u64().unwrap() as usize;
    let images = scenario["images"].as_str().unwrap_or("none");
    set_capabilities(capabilities_from_name(images));

    let (terminal, handle) = concrete(columns, rows);
    terminal.borrow_mut().set_capture_cursor_writes(true);
    let opened_urls: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let paste_count = Rc::new(Cell::new(0));
    let opened_sink = Rc::clone(&opened_urls);
    let paste_sink = Rc::clone(&paste_count);
    let options_json = &scenario["options"];
    let tui = TuiAltScreen::with_options(
        handle,
        None,
        None,
        TuiAltScreenOptions {
            wheel_scroll_lines: options_json["wheelScrollLines"].as_i64(),
            mouse: options_json["mouse"].as_bool(),
            search_match_style: (options_json["customSearchStyles"].as_bool() == Some(true))
                .then(|| Rc::new(|text: &str| format!("\x1b[41m{text}\x1b[49m")) as Rc<dyn Fn(&str) -> String>),
            search_current_match_style: (options_json["customSearchStyles"].as_bool() == Some(true))
                .then(|| Rc::new(|text: &str| format!("\x1b[42m{text}\x1b[49m")) as Rc<dyn Fn(&str) -> String>),
            open_url: Some(Rc::new(move |url: &str| {
                opened_sink.borrow_mut().push(url.to_string());
            })),
            on_right_click_paste: Some(Rc::new(move || {
                paste_sink.set(paste_sink.get() + 1);
            })),
        },
    );
    tui.set_environment(environment_from_json(&scenario["environment"]));

    let mut components: Vec<ComponentRef> = Vec::new();
    let mut scrolls: Vec<Rc<RefCell<ScrollState>>> = Vec::new();
    let mut texts: Vec<ComponentRef> = Vec::new();

    for step in scenario["steps"].as_array().unwrap() {
        match step["kind"].as_str().unwrap() {
            "add" => {
                let focus = step["focusable"]
                    .as_bool()
                    .unwrap_or(false)
                    .then(|| Rc::new(Cell::new(false)));
                let view = component(ScenarioComponent {
                    lines: json_lines(&step["lines"]),
                    inputs: Vec::new(),
                    focus,
                });
                tui.add_child(view.clone());
                components.push(view);
            }
            "add_text" => {
                let text = component(Text::new(
                    step["text"].as_str().unwrap_or(""),
                    step["paddingX"].as_u64().unwrap_or(0) as usize,
                    step["paddingY"].as_u64().unwrap_or(0) as usize,
                ));
                texts.push(text.clone());
                tui.add_child(text.clone());
                components.push(text);
            }
            "add_scroll" => {
                let child = components[step["component"].as_u64().unwrap_or(0) as usize].clone();
                let follow = match step["follow"].as_str() {
                    Some("end") => Some(ScrollFollow::End),
                    Some("none") => Some(ScrollFollow::None),
                    _ => None,
                };
                let view = ScrollView::new(
                    child,
                    ScrollViewOptions {
                        follow,
                        primary: step["primary"].as_bool().unwrap_or(false),
                        scrollbar: step["scrollbar"].as_str().map(|value| scrollbar(&json!(value))),
                        scrollbar_hide_delay_ms: step["scrollbarHideDelayMs"].as_u64(),
                        ..ScrollViewOptions::default()
                    },
                );
                scrolls.push(Rc::clone(view.state()));
                let view = component(view);
                tui.add_child(view.clone());
                components.push(view);
            }
            "add_vstack" => {
                let entries = step["entries"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|entry| stack_entry(&components, entry))
                    .collect();
                let stack = component(VStack::new(
                    entries,
                    StackOptions {
                        gap: step["gap"].as_u64().map(|gap| gap as usize),
                        ..StackOptions::default()
                    },
                ));
                tui.add_child(stack.clone());
                components.push(stack);
            }
            "add_hstack" => {
                let entries = step["entries"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|entry| stack_entry(&components, entry))
                    .collect();
                let stack = component(HStack::new(
                    entries,
                    StackOptions {
                        gap: step["gap"].as_u64().map(|gap| gap as usize),
                        ..StackOptions::default()
                    },
                ));
                tui.add_child(stack.clone());
                components.push(stack);
            }
            "add_image" => {
                let image = component(Image::new(
                    step["base64Data"].as_str().unwrap_or(""),
                    step["mimeType"].as_str().unwrap_or("image/png"),
                    Rc::new(ImageTheme {
                        fallback_color: Rc::new(|value: &str| value.to_string()),
                    }),
                    ImageOptions {
                        filename: step["filename"].as_str().map(str::to_string),
                        image_id: step["imageId"].as_u64().map(|value| value as u32),
                        ..ImageOptions::default()
                    },
                    Some(ImageDimensions {
                        width_px: step["widthPx"].as_u64().unwrap_or(800) as u32,
                        height_px: step["heightPx"].as_u64().unwrap_or(600) as u32,
                    }),
                ));
                tui.add_child(image.clone());
                components.push(image);
            }
            "kitty_metadata" => register_kitty_image_metadata(KittyImageMetadata {
                image_id: step["imageId"].as_u64().unwrap() as u32,
                columns: step["columns"].as_u64().unwrap() as u32,
                rows: step["rows"].as_u64().unwrap() as u32,
                width_px: step["widthPx"].as_u64().unwrap() as u32,
                height_px: step["heightPx"].as_u64().unwrap() as u32,
            }),
            "set_layout_root" => {
                let root = step["component"]
                    .as_u64()
                    .map(|index| components[index as usize].clone());
                tui.set_layout_root(root);
            }
            "start" => tui.start().unwrap(),
            "wait" => {
                tui.flush_pending();
                terminal.borrow_mut().flush();
            }
            "send_input" => {
                terminal.borrow_mut().send_input(step["data"].as_str().unwrap_or(""));
                tui.process_events();
            }
            "set_lines" => {
                let index = step["component"].as_u64().unwrap_or(0) as usize;
                let lines = json_lines(&step["lines"]);
                let mut borrow = components[index].borrow_mut();
                let any = borrow.as_any_mut().expect("scenario component downcast");
                any.downcast_mut::<ScenarioComponent>().unwrap().lines = lines;
            }
            "set_text" => {
                let index = step["index"].as_u64().unwrap_or(0) as usize;
                let mut borrow = texts[index].borrow_mut();
                let any = borrow.as_any_mut().expect("text downcast");
                any.downcast_mut::<Text>()
                    .unwrap()
                    .set_text(step["text"].as_str().unwrap_or(""));
            }
            "request_render" => tui.request_render(step["force"].as_bool().unwrap_or(false)),
            "render_now" => tui.render_now(step["force"].as_bool().unwrap_or(false)),
            "resize" => terminal.borrow_mut().resize(
                step["columns"].as_u64().unwrap_or(columns as u64) as usize,
                step["rows"].as_u64().unwrap_or(rows as u64) as usize,
            ),
            "scroll_by" => tui.scroll_by(step["lines"].as_i64().unwrap_or(0)),
            "scroll_to_top" => tui.scroll_to_top(),
            "scroll_to_bottom" => tui.scroll_to_bottom(),
            "flash" => tui.flash(
                step["message"].as_str().unwrap_or(""),
                step["durationMs"].as_u64().map(std::time::Duration::from_millis),
            ),
            "focus" => {
                let target = step["component"]
                    .as_u64()
                    .map(|index| components[index as usize].clone());
                tui.set_focus(target);
            }
            "stop" => tui.stop_with(pi_tui::tui::TuiStopOptions {
                preserve_screen: step["preserveScreen"].as_bool().unwrap_or(false),
            }),
            other => panic!("unsupported scenario step: {other}"),
        }
    }

    let result = json!({
        "writes": terminal.borrow().writes(),
        "fullRedraws": tui.full_redraws(),
        "viewportTop": tui.viewport_top(),
        "isFollowingOutput": tui.is_following_output(),
        "scrollTops": scrolls.iter().map(|state| state.borrow().scroll_top()).collect::<Vec<_>>(),
        "followingEnds": scrolls.iter().map(|state| state.borrow().is_following_end()).collect::<Vec<_>>(),
        "scrollbarVisible": scrolls.iter().map(|state| state.borrow().is_scrollbar_visible()).collect::<Vec<_>>(),
        "openedUrls": *opened_urls.borrow(),
        "pasteCount": paste_count.get(),
    });
    reset_capabilities_cache();
    result
}

fn assert_scenario(oracle: &mut TsOracle, scenario: Value) {
    let response = oracle.request(&json!({
        "id": "alt-screen",
        "operation": "scenario",
        "columns": scenario["columns"],
        "rows": scenario["rows"],
        "images": scenario["images"],
        "options": scenario["options"],
        "environment": scenario["environment"],
        "steps": scenario["steps"],
    }));
    assert_eq!(response["ok"], Value::Bool(true), "{response}");
    let actual = run_scenario(&scenario);
    let expected = &response["output"];
    let name = scenario["name"].as_str().unwrap_or("scenario");
    for field in [
        "fullRedraws",
        "viewportTop",
        "isFollowingOutput",
        "scrollTops",
        "followingEnds",
        "scrollbarVisible",
        "openedUrls",
        "pasteCount",
    ] {
        assert_eq!(actual[field], expected[field], "{name}: {field}");
    }
    let actual_writes = actual["writes"].as_str().unwrap();
    let expected_writes = expected["writes"].as_str().unwrap();
    if actual_writes != expected_writes {
        let bytes = actual_writes.as_bytes();
        let expected_bytes = expected_writes.as_bytes();
        let index = bytes
            .iter()
            .zip(expected_bytes)
            .position(|(left, right)| left != right)
            .unwrap_or(bytes.len().min(expected_bytes.len()));
        let start = index.saturating_sub(120);
        let end = (index + 200).min(bytes.len());
        let expected_end = (index + 200).min(expected_bytes.len());
        panic!(
            "{name}: writes differ at byte {index}\nactual:   {:?}\nexpected: {:?}",
            String::from_utf8_lossy(&bytes[start..end]),
            String::from_utf8_lossy(&expected_bytes[start..expected_end]),
        );
    }
}

fn scenario(name: &str, columns: usize, rows: usize, images: &str, steps: Value) -> Value {
    json!({
        "name": name,
        "columns": columns,
        "rows": rows,
        "images": images,
        "options": {},
        "environment": {},
        "steps": steps,
    })
}

fn kitty_line(image_id: u32, columns: u32, rows: u32) -> String {
    encode_kitty(
        "AAAA",
        &KittyEncodeOptions {
            columns: Some(columns),
            rows: Some(rows),
            image_id: Some(image_id),
            move_cursor: Some(false),
        },
    )
}

#[test]
fn alt_screen_scenarios_match_typescript() {
    let mut oracle = TsOracle::spawn("tui-alt-screen-oracle.ts", SOURCE_DIRECTORY);
    let mut scenarios: Vec<Value> = Vec::new();

    let lines_10 = (1..=10).map(|index| format!("line {index}")).collect::<Vec<_>>();
    let lines_12 = (1..=12).map(|index| format!("line {index}")).collect::<Vec<_>>();
    scenarios.push(scenario(
        "basic-scroll",
        20,
        4,
        "none",
        json!([
            {"kind": "add_text", "text": lines_10.join("\n")},
            {"kind": "start"},
            {"kind": "wait"},
            {"kind": "send_input", "data": "\u{1b}[<64;1;1M"},
            {"kind": "wait"},
            {"kind": "set_text", "index": 0, "text": lines_12.join("\n")},
            {"kind": "request_render"},
            {"kind": "wait"},
            {"kind": "scroll_to_bottom"},
            {"kind": "wait"},
            {"kind": "stop"}
        ]),
    ));

    let dock = scenario(
        "dock-layout",
        20,
        6,
        "none",
        json!([
            {"kind": "add_text", "text": (1..=8).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n")},
            {"kind": "add_scroll", "component": 0, "follow": "end", "primary": true},
            {"kind": "add_text", "text": "editor"},
            {"kind": "add_text", "text": "footer"},
            {"kind": "add_vstack", "entries": [{"component": 2}, {"component": 3}]},
            {"kind": "add_vstack", "entries": [{"component": 1, "basis": 0, "grow": 1, "minSize": 1}, {"component": 4, "basis": "auto", "minSize": 1}]},
            {"kind": "set_layout_root", "component": 5},
            {"kind": "start"},
            {"kind": "wait"},
            {"kind": "send_input", "data": "\u{1b}[<64;1;6M"},
            {"kind": "wait"},
            {"kind": "set_text", "index": 0, "text": lines_10.join("\n")},
            {"kind": "request_render"},
            {"kind": "wait"},
            {"kind": "scroll_to_bottom"},
            {"kind": "wait"},
            {"kind": "stop"}
        ]),
    );
    scenarios.push(dock);

    scenarios.push(scenario(
        "hstack-wheel",
        20,
        4,
        "none",
        json!([
            {"kind": "add_text", "text": "a1\na2\na3\na4\na5\na6\na7"},
            {"kind": "add_text", "text": "b1\nb2\nb3\nb4\nb5\nb6\nb7"},
            {"kind": "add_scroll", "component": 0, "follow": "end", "primary": true},
            {"kind": "add_scroll", "component": 1, "follow": "end"},
            {"kind": "add_hstack", "entries": [{"component": 2, "basis": 10, "shrink": 0}, {"component": 3, "basis": 10, "shrink": 0}]},
            {"kind": "set_layout_root", "component": 4},
            {"kind": "start"},
            {"kind": "wait"},
            {"kind": "send_input", "data": "\u{1b}[<64;15;1M"},
            {"kind": "wait"},
            {"kind": "stop"}
        ]),
    ));

    let mut chain = scenario(
        "wheel-chain",
        20,
        4,
        "none",
        json!([
            {"kind": "add_text", "text": "i1\ni2\ni3\ni4\ni5\ni6"},
            {"kind": "add_scroll", "component": 0},
            {"kind": "add_text", "text": "tail1\ntail2\ntail3\ntail4\ntail5"},
            {"kind": "add_vstack", "entries": [{"component": 1, "basis": 2}, {"component": 2}]},
            {"kind": "add_scroll", "component": 3, "primary": true},
            {"kind": "set_layout_root", "component": 4},
            {"kind": "start"},
            {"kind": "wait"},
            {"kind": "send_input", "data": "\u{1b}[<65;1;1M"},
            {"kind": "wait"},
            {"kind": "send_input", "data": "\u{1b}[<65;1;1M"},
            {"kind": "wait"},
            {"kind": "stop"}
        ]),
    );
    chain["options"] = json!({"wheelScrollLines": 3});
    scenarios.push(chain);

    scenarios.push(scenario(
        "keyboard-navigation",
        20,
        8,
        "none",
        json!([
            {"kind": "add_text", "text": lines_12.join("\n")},
            {"kind": "start"},
            {"kind": "wait"},
            {"kind": "send_input", "data": "\u{1b}[57421u"},
            {"kind": "send_input", "data": "\u{1b}[57421;1:3u"},
            {"kind": "wait"},
            {"kind": "send_input", "data": "\u{1b}[57422u"},
            {"kind": "send_input", "data": "\u{1b}[57422;1:3u"},
            {"kind": "wait"},
            {"kind": "send_input", "data": "\u{1b}OH"},
            {"kind": "wait"},
            {"kind": "send_input", "data": "\u{1b}OF"},
            {"kind": "wait"},
            {"kind": "stop"}
        ]),
    ));

    scenarios.push(scenario(
        "prompt-jumps",
        20,
        3,
        "none",
        json!([
            {"kind": "add_text", "text": (1..=4).flat_map(|m| [format!("\u{1b}]133;A\u{7}message {m}"), "detail".to_string()]).collect::<Vec<_>>().join("\n")},
            {"kind": "start"},
            {"kind": "wait"},
            {"kind": "send_input", "data": "\u{1b}[57419;6u"},
            {"kind": "send_input", "data": "\u{1b}[57419;6:3u"},
            {"kind": "wait"},
            {"kind": "send_input", "data": "\u{1b}[1;6A"},
            {"kind": "wait"},
            {"kind": "send_input", "data": "\u{1b}[57420;6u"},
            {"kind": "send_input", "data": "\u{1b}[57420;6:3u"},
            {"kind": "wait"},
            {"kind": "send_input", "data": "\u{1b}[1;6B"},
            {"kind": "wait"},
            {"kind": "stop"}
        ]),
    ));

    let mut search_styles = scenario(
        "search-styles",
        60,
        4,
        "none",
        json!([
            {"kind": "add_text", "text": "needle first\nmiddle\nneedle second\nend"},
            {"kind": "start"},
            {"kind": "wait"},
            {"kind": "send_input", "data": "\u{1b}[102;6u"},
            {"kind": "send_input", "data": "needle"},
            {"kind": "wait"},
            {"kind": "stop"}
        ]),
    );
    search_styles["options"] = json!({"customSearchStyles": true});
    scenarios.push(search_styles);

    let transcript = (1..=12)
        .map(|index| match index {
            5 => "line 5 needle one".to_string(),
            10 => "line 10 needle two".to_string(),
            other => format!("line {other}"),
        })
        .collect::<Vec<_>>()
        .join("\n");
    scenarios.push(scenario(
        "search-navigation",
        60,
        8,
        "none",
        json!([
            {"kind": "add_text", "text": transcript},
            {"kind": "add_scroll", "component": 0, "follow": "end", "primary": true},
            {"kind": "add", "lines": ["editor"], "focusable": true},
            {"kind": "add_vstack", "entries": [{"component": 1, "basis": 0, "grow": 1, "minSize": 1}, {"component": 2, "basis": 1, "shrink": 0}]},
            {"kind": "set_layout_root", "component": 3},
            {"kind": "focus", "component": 2},
            {"kind": "start"},
            {"kind": "wait"},
            {"kind": "send_input", "data": "\u{1b}[102;6u"},
            {"kind": "send_input", "data": "needle"},
            {"kind": "wait"},
            {"kind": "send_input", "data": "\u{1b}[<64;1;4M"},
            {"kind": "send_input", "data": "\u{1b}[<64;1;4M"},
            {"kind": "send_input", "data": "\u{1b}[<64;1;4M"},
            {"kind": "send_input", "data": "\u{1b}[<64;1;4M"},
            {"kind": "send_input", "data": "\u{1b}[<64;1;4M"},
            {"kind": "send_input", "data": "\u{1b}[<64;1;4M"},
            {"kind": "wait"},
            {"kind": "send_input", "data": "\u{7}"},
            {"kind": "wait"},
            {"kind": "send_input", "data": "\u{1b}[103;6u"},
            {"kind": "wait"},
            {"kind": "send_input", "data": "\u{1b}"},
            {"kind": "send_input", "data": "x"},
            {"kind": "wait"},
            {"kind": "stop"}
        ]),
    ));

    scenarios.push(scenario(
        "resize",
        20,
        4,
        "none",
        json!([
            {"kind": "add_text", "text": lines_10.join("\n")},
            {"kind": "start"},
            {"kind": "wait"},
            {"kind": "resize", "columns": 12, "rows": 6},
            {"kind": "wait"},
            {"kind": "stop"}
        ]),
    ));

    scenarios.push(scenario(
        "flash",
        20,
        4,
        "none",
        json!([
            {"kind": "add_text", "text": "one\ntwo\nthree\nfour"},
            {"kind": "start"},
            {"kind": "wait"},
            {"kind": "flash", "message": "First", "durationMs": 80},
            {"kind": "flash", "message": "Second", "durationMs": 500},
            {"kind": "wait"},
            {"kind": "stop"}
        ]),
    ));

    scenarios.push(scenario(
        "selection",
        20,
        4,
        "none",
        json!([
            {"kind": "add_text", "text": "\u{1b}[1mal\u{1b}[0mpha\nbeta\ngamma\ndelta"},
            {"kind": "start"},
            {"kind": "wait"},
            {"kind": "send_input", "data": "\u{1b}[<0;1;1M"},
            {"kind": "send_input", "data": "\u{1b}[<32;4;2M"},
            {"kind": "send_input", "data": "\u{1b}[<0;4;2m"},
            {"kind": "wait"},
            {"kind": "stop"}
        ]),
    ));

    scenarios.push(scenario(
        "word-selection",
        20,
        2,
        "none",
        json!([
            {"kind": "add_text", "text": "zero alpha beta\ngamma delta"},
            {"kind": "start"},
            {"kind": "wait"},
            {"kind": "send_input", "data": "\u{1b}[<0;6;1M"},
            {"kind": "send_input", "data": "\u{1b}[<0;6;1m"},
            {"kind": "send_input", "data": "\u{1b}[<0;10;1M"},
            {"kind": "send_input", "data": "\u{1b}[<0;10;1m"},
            {"kind": "wait"},
            {"kind": "send_input", "data": "\u{1b}[<0;12;1M"},
            {"kind": "send_input", "data": "\u{1b}[<0;12;1m"},
            {"kind": "send_input", "data": "\u{1b}[<0;14;1M"},
            {"kind": "send_input", "data": "\u{1b}[<32;3;2M"},
            {"kind": "send_input", "data": "\u{1b}[<0;3;2m"},
            {"kind": "wait"},
            {"kind": "send_input", "data": "\u{1b}[<0;7;2M"},
            {"kind": "send_input", "data": "\u{1b}[<0;7;2m"},
            {"kind": "send_input", "data": "\u{1b}[<0;9;2M"},
            {"kind": "send_input", "data": "\u{1b}[<0;9;2m"},
            {"kind": "send_input", "data": "\u{1b}[<0;11;2M"},
            {"kind": "send_input", "data": "\u{1b}[<0;11;2m"},
            {"kind": "wait"},
            {"kind": "stop"}
        ]),
    ));

    scenarios.push(scenario(
        "focus-loss",
        20,
        4,
        "none",
        json!([
            {"kind": "add_text", "text": "alpha\nbeta\ngamma\ndelta"},
            {"kind": "start"},
            {"kind": "wait"},
            {"kind": "send_input", "data": "\u{1b}[<0;1;1M"},
            {"kind": "send_input", "data": "\u{1b}[<32;4;2M"},
            {"kind": "wait"},
            {"kind": "send_input", "data": "\u{1b}O"},
            {"kind": "send_input", "data": "\u{1b}I"},
            {"kind": "wait"},
            {"kind": "send_input", "data": "\u{1b}[<32;4;2M"},
            {"kind": "send_input", "data": "\u{1b}[<0;4;2m"},
            {"kind": "wait"},
            {"kind": "stop"}
        ]),
    ));

    let url = "https://example.com/path?q=1";
    let bel_url = "https://example.com/bel";
    let emoji_url = "https://example.com/emoji";
    scenarios.push(scenario(
        "link-click",
        20,
        3,
        "none",
        json!([
            {"kind": "add_text", "text": format!("{}\n\u{1b}]8;;{bel_url}\u{7}link\u{1b}]8;;\u{7}\n{}", pi_tui::terminal_image::hyperlink("link", url), pi_tui::terminal_image::hyperlink("🙂", emoji_url))},
            {"kind": "start"},
            {"kind": "wait"},
            {"kind": "send_input", "data": "\u{1b}[<0;2;1M"},
            {"kind": "send_input", "data": "\u{1b}[<0;2;1m"},
            {"kind": "wait"},
            {"kind": "send_input", "data": "\u{1b}[<0;2;2M"},
            {"kind": "send_input", "data": "\u{1b}[<0;2;2m"},
            {"kind": "wait"},
            {"kind": "send_input", "data": "\u{1b}[<0;1;3M"},
            {"kind": "send_input", "data": "\u{1b}[<0;1;3m"},
            {"kind": "wait"},
            {"kind": "send_input", "data": "\u{1b}[<0;2;1M"},
            {"kind": "send_input", "data": "\u{1b}[<32;4;1M"},
            {"kind": "send_input", "data": "\u{1b}[<0;4;1m"},
            {"kind": "wait"},
            {"kind": "stop"}
        ]),
    ));

    let crop_line = kitty_line(123, 2, 3);
    scenarios.push(scenario(
        "kitty-crop",
        20,
        3,
        "kitty",
        json!([
            {"kind": "kitty_metadata", "imageId": 123, "columns": 2, "rows": 3, "widthPx": 100, "heightPx": 100},
            {"kind": "add", "lines": ["before", crop_line, "", "", "after", "end"]},
            {"kind": "start"},
            {"kind": "wait"},
            {"kind": "stop"}
        ]),
    ));

    scenarios.push(scenario(
        "kitty-reuse",
        20,
        6,
        "kitty",
        json!([
            {"kind": "add_text", "text": "left"},
            {"kind": "add_image", "base64Data": "A".repeat(8192), "mimeType": "image/png", "widthPx": 100, "heightPx": 100, "imageId": 777},
            {"kind": "add_text", "text": "header"},
            {"kind": "add_hstack", "entries": [{"component": 0, "basis": 10}, {"component": 1, "basis": 10}]},
            {"kind": "add_vstack", "entries": [{"component": 2, "basis": "auto"}, {"component": 3, "basis": 4}]},
            {"kind": "set_layout_root", "component": 4},
            {"kind": "start"},
            {"kind": "wait"},
            {"kind": "set_text", "index": 0, "text": "changed"},
            {"kind": "set_text", "index": 1, "text": "header\nsecond"},
            {"kind": "request_render"},
            {"kind": "wait"},
            {"kind": "stop"}
        ]),
    ));

    scenarios.push(scenario(
        "iterm2-fallback",
        20,
        3,
        "iterm2",
        json!([
            {"kind": "add", "lines": ["\u{1b}]133;B\u{7}\u{1b}]133;C\u{7}\u{1b}]133;A\u{7}content"]},
            {"kind": "add_image", "base64Data": "AAAA", "mimeType": "image/png", "filename": "example.png", "widthPx": 10, "heightPx": 10},
            {"kind": "start"},
            {"kind": "wait"},
            {"kind": "stop"}
        ]),
    ));

    scenarios.push(scenario(
        "iterm2-stale",
        20,
        3,
        "iterm2",
        json!([
            {"kind": "add", "lines": ["\u{1b}]1337;File=inline=1;width=2;height=auto:AAAA\u{7}", "", "", "after", "more", "end"]},
            {"kind": "start"},
            {"kind": "wait"},
            {"kind": "scroll_to_top"},
            {"kind": "wait"},
            {"kind": "scroll_by", "lines": 1},
            {"kind": "wait"},
            {"kind": "stop"}
        ]),
    ));

    let mut tmux = scenario(
        "tmux-mouse",
        20,
        4,
        "none",
        json!([
            {"kind": "add_text", "text": "one\ntwo"},
            {"kind": "start"},
            {"kind": "wait"},
            {"kind": "stop"}
        ]),
    );
    tmux["environment"] = json!({"TMUX": "/tmp/tmux/default,1,0", "TERM": "xterm-256color"});
    scenarios.push(tmux);

    let mut screen = scenario(
        "screen-mouse",
        20,
        4,
        "none",
        json!([
            {"kind": "add_text", "text": "one\ntwo"},
            {"kind": "start"},
            {"kind": "wait"},
            {"kind": "stop"}
        ]),
    );
    screen["environment"] = json!({"TERM": "screen-256color"});
    scenarios.push(screen);

    let mut preserve = scenario(
        "preserve-screen",
        20,
        4,
        "none",
        json!([
            {"kind": "add_text", "text": "first\nsecond\nthird"},
            {"kind": "start"},
            {"kind": "wait"},
            {"kind": "stop", "preserveScreen": true}
        ]),
    );
    preserve["environment"] = json!({"TERM": "xterm-256color"});
    scenarios.push(preserve);

    let mut windows = scenario(
        "right-click",
        20,
        4,
        "none",
        json!([
            {"kind": "add_text", "text": "one\ntwo"},
            {"kind": "start"},
            {"kind": "wait"},
            {"kind": "send_input", "data": "\u{1b}[<2;1;1M"},
            {"kind": "send_input", "data": "\u{1b}[<2;1;1m"},
            {"kind": "wait"},
            {"kind": "stop"}
        ]),
    );
    windows["environment"] = json!({"platform": "win32"});
    scenarios.push(windows);

    let mut mouse_disabled = scenario(
        "mouse-disabled",
        20,
        4,
        "none",
        json!([
            {"kind": "add_text", "text": "one\ntwo"},
            {"kind": "start"},
            {"kind": "wait"},
            {"kind": "stop"}
        ]),
    );
    mouse_disabled["options"] = json!({"mouse": false});
    scenarios.push(mouse_disabled);

    for scenario in scenarios {
        assert_scenario(&mut oracle, scenario);
    }
}
