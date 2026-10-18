#[path = "support/oracle.rs"]
mod oracle;

use std::cell::{Cell, RefMut};
use std::collections::HashMap;
use std::rc::Rc;

use oracle::TsOracle;
use pi_tui::components::h_stack::HStack;
use pi_tui::components::scroll_view::{
    ScrollFollow, ScrollOverscroll, ScrollView, ScrollViewOptions, ScrollViewScrollToOptions, ScrollViewScrollbar,
};
use pi_tui::components::stack::StackOptions;
use pi_tui::components::text::{StyleFn, Text};
use pi_tui::components::v_stack::VStack;
use pi_tui::layout::{LayoutFrame, LayoutRect, render_layout_frame};
use pi_tui::layout_node::{StackAlign, StackBasis, StackLayoutEntry};
use pi_tui::terminal_image::{KittyImageMetadata, register_kitty_image_metadata};
use pi_tui::tui::{Component, ComponentRef, component};
use serde_json::{Value, json};

const SOURCE_DIRECTORY: &str = "packages/tui/src";

struct StaticLines {
    lines: Vec<String>,
}

impl Component for StaticLines {
    fn render(&mut self, _width: usize) -> Vec<String> {
        self.lines.clone()
    }
}

fn build_node(spec: &Value, path: &str, by_path: &mut HashMap<String, ComponentRef>) -> ComponentRef {
    let kind = spec["kind"].as_str().expect("node kind");
    let built = match kind {
        "text" => {
            let text = spec["text"].as_str().unwrap();
            let padding_x = spec["paddingX"].as_u64().unwrap() as usize;
            let padding_y = spec["paddingY"].as_u64().unwrap() as usize;
            let background = spec["background"].as_bool().unwrap_or(false);
            if background {
                component(Text::with_background(
                    text,
                    padding_x,
                    padding_y,
                    Rc::new(|text: &str| format!("[{text}]")),
                ))
            } else {
                component(Text::new(text, padding_x, padding_y))
            }
        }
        "lines" => {
            if let Some(metadata) = spec.get("registerKitty") {
                register_kitty_image_metadata(KittyImageMetadata {
                    image_id: metadata["imageId"].as_u64().unwrap() as u32,
                    columns: metadata["columns"].as_u64().unwrap() as u32,
                    rows: metadata["rows"].as_u64().unwrap() as u32,
                    width_px: metadata["widthPx"].as_u64().unwrap() as u32,
                    height_px: metadata["heightPx"].as_u64().unwrap() as u32,
                });
            }
            component(StaticLines {
                lines: spec["lines"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|line| line.as_str().unwrap().to_string())
                    .collect(),
            })
        }
        "vstack" | "hstack" => {
            let entries: Vec<StackLayoutEntry> = spec["entries"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .map(|(index, entry)| {
                    let child = build_node(&entry["component"], &format!("{path}.{index}"), by_path);
                    let mut stack_entry = StackLayoutEntry::new(child);
                    if let Some(basis) = entry.get("basis") {
                        stack_entry.basis = Some(match basis {
                            Value::String(_) => StackBasis::Auto,
                            Value::Number(number) => StackBasis::Cells(number.as_u64().unwrap() as usize),
                            other => panic!("unsupported basis: {other}"),
                        });
                    }
                    stack_entry.grow = entry["grow"].as_u64().map(|value| value as usize);
                    stack_entry.shrink = entry["shrink"].as_u64().map(|value| value as usize);
                    stack_entry.min_size = entry["minSize"].as_u64().map(|value| value as usize);
                    stack_entry.max_size = entry["maxSize"].as_u64().map(|value| value as usize);
                    stack_entry.visible = match entry.get("visible") {
                        Some(Value::Bool(false)) => Some(Rc::new(|_: pi_tui::layout_node::LayoutViewport| false)),
                        Some(Value::Object(object)) => {
                            let min_width = object["minWidth"].as_u64().unwrap() as usize;
                            Some(Rc::new(move |viewport: pi_tui::layout_node::LayoutViewport| {
                                viewport.width >= min_width
                            }))
                        }
                        _ => None,
                    };
                    stack_entry
                })
                .collect();
            let options = StackOptions {
                gap: spec["gap"].as_u64().map(|value| value as usize),
                align: match spec["align"].as_str() {
                    Some("start") => Some(StackAlign::Start),
                    Some("center") => Some(StackAlign::Center),
                    Some("end") => Some(StackAlign::End),
                    Some("stretch") => Some(StackAlign::Stretch),
                    Some(other) => panic!("unsupported align: {other}"),
                    None => None,
                },
            };
            if kind == "vstack" {
                component(VStack::new(entries, options))
            } else {
                component(HStack::new(entries, options))
            }
        }
        "scroll" => {
            let child = build_node(&spec["component"], &format!("{path}.0"), by_path);
            let scrollbar_style: Option<StyleFn> = (spec["scrollbarStyle"].as_str() == Some("red"))
                .then(|| Rc::new(|text: &str| format!("\x1b[48;5;1m{text}\x1b[49m")) as StyleFn);
            let options = ScrollViewOptions {
                follow: match spec["follow"].as_str() {
                    Some("end") => Some(ScrollFollow::End),
                    Some("none") => Some(ScrollFollow::None),
                    Some(other) => panic!("unsupported follow: {other}"),
                    None => None,
                },
                primary: spec["primary"].as_bool().unwrap_or(false),
                overscroll: match spec["overscroll"].as_str() {
                    Some("contain") => Some(ScrollOverscroll::Contain),
                    Some("chain") => Some(ScrollOverscroll::Chain),
                    Some(other) => panic!("unsupported overscroll: {other}"),
                    None => None,
                },
                scrollbar: match spec["scrollbar"].as_str() {
                    Some("hidden") => Some(ScrollViewScrollbar::Hidden),
                    Some("auto") => Some(ScrollViewScrollbar::Auto),
                    Some("always") => Some(ScrollViewScrollbar::Always),
                    Some(other) => panic!("unsupported scrollbar: {other}"),
                    None => None,
                },
                scrollbar_style,
                scrollbar_hide_delay_ms: spec["scrollbarHideDelayMs"].as_u64(),
            };
            component(ScrollView::new(child, options))
        }
        other => panic!("unsupported layout node: {other}"),
    };
    by_path.insert(path.to_string(), Rc::clone(&built));
    built
}

fn rect_json(rect: LayoutRect) -> Value {
    json!({"x": rect.x, "y": rect.y, "width": rect.width, "height": rect.height})
}

fn serialize_frame(frame: &LayoutFrame) -> Value {
    let mut boxes = Vec::new();
    let mut scroll_views = Vec::new();
    fn visit(frame_box: &pi_tui::layout::LayoutBox, path: &str, boxes: &mut Vec<Value>, scroll_views: &mut Vec<Value>) {
        boxes.push(json!({
            "path": path,
            "x": frame_box.rect.x,
            "y": frame_box.rect.y,
            "width": frame_box.rect.width,
            "height": frame_box.rect.height,
            "clip": rect_json(frame_box.clip),
        }));
        if let Some(scroll_view) = &frame_box.scroll_view {
            let state = scroll_view.borrow();
            scroll_views.push(json!({
                "path": path,
                "scrollTop": state.scroll_top(),
                "followingEnd": state.is_following_end(),
                "viewportHeight": state.viewport_height(),
                "scrollbarVisible": state.is_scrollbar_visible(),
                "contentWidth": state.get_content_width(frame_box.rect.width.max(0) as usize),
            }));
        }
        for (index, child) in frame_box.children.iter().enumerate() {
            visit(child, &format!("{path}.{index}"), boxes, scroll_views);
        }
    }
    visit(&frame.root, "0", &mut boxes, &mut scroll_views);
    json!({"lines": frame.lines.clone(), "boxes": boxes, "scrollViews": scroll_views})
}

fn scroll_mut(component_ref: &ComponentRef) -> RefMut<'_, ScrollView> {
    RefMut::map(component_ref.borrow_mut(), |value| {
        value
            .as_any_mut()
            .and_then(|value| value.downcast_mut::<ScrollView>())
            .expect("scroll view component")
    })
}

fn text_mut(component_ref: &ComponentRef) -> RefMut<'_, Text> {
    RefMut::map(component_ref.borrow_mut(), |value| {
        value
            .as_any_mut()
            .and_then(|value| value.downcast_mut::<Text>())
            .expect("text component")
    })
}

fn execute_case(case: &Value) -> Value {
    let mut by_path = HashMap::new();
    let root = build_node(&case["root"], "0", &mut by_path);
    let render_requests = Rc::new(Cell::new(0usize));
    let mut snapshots = Vec::new();

    for operation in case["operations"].as_array().unwrap() {
        let kind = operation["kind"].as_str().unwrap();
        if kind == "frame" {
            let counter = Rc::clone(&render_requests);
            let frame = render_layout_frame(
                &root,
                operation["width"].as_u64().unwrap() as usize,
                operation["height"].as_u64().unwrap() as usize,
                Rc::new(move || counter.set(counter.get() + 1)),
            );
            snapshots.push(serialize_frame(&frame));
            continue;
        }
        let target = by_path
            .get(operation["target"].as_str().unwrap())
            .expect("layout target");
        match kind {
            "scrollBy" => {
                scroll_mut(target).scroll_by(operation["lines"].as_i64().unwrap());
            }
            "scrollTo" => {
                scroll_mut(target).scroll_to(
                    operation["top"].as_i64().unwrap(),
                    ScrollViewScrollToOptions {
                        disable_follow: operation["disableFollow"].as_bool().unwrap_or(false),
                    },
                );
            }
            "scrollToEnd" => scroll_mut(target).scroll_to_end(),
            "scrollToStart" => scroll_mut(target).scroll_to_start(),
            "setText" => text_mut(target).set_text(operation["text"].as_str().unwrap()),
            "setScrollbar" => {
                let scrollbar = match operation["scrollbar"].as_str().unwrap() {
                    "hidden" => ScrollViewScrollbar::Hidden,
                    "auto" => ScrollViewScrollbar::Auto,
                    "always" => ScrollViewScrollbar::Always,
                    other => panic!("unsupported scrollbar: {other}"),
                };
                scroll_mut(target).set_scrollbar(scrollbar);
            }
            other => panic!("unsupported layout operation: {other}"),
        }
    }

    json!({"snapshots": snapshots, "renderRequests": render_requests.get()})
}

fn scroll_node(child: Value, options: Value) -> Value {
    let mut node = json!({"kind": "scroll", "component": child});
    if let Some(object) = options.as_object() {
        for (key, value) in object {
            node[key] = value.clone();
        }
    }
    node
}

fn text_node(text: &str) -> Value {
    json!({"kind": "text", "text": text, "paddingX": 0, "paddingY": 0, "background": false})
}

#[test]
fn layout_geometry_and_rendering_match_typescript() {
    let cases = json!([
        {
            "root": {
                "kind": "vstack",
                "entries": [
                    {"component": text_node("top"), "basis": 1, "shrink": 0},
                    {"component": text_node("body"), "basis": 0, "grow": 1}
                ]
            },
            "operations": [{"kind": "frame", "width": 10, "height": 4}]
        },
        {
            "root": {
                "kind": "vstack",
                "entries": [
                    {"component": text_node("top1\ntop2\ntop3")},
                    {"component": text_node("selector"), "minSize": 3},
                    {"component": text_node("below")},
                    {"component": text_node("footer"), "minSize": 1}
                ]
            },
            "operations": [{"kind": "frame", "width": 10, "height": 9}]
        },
        {
            "root": {
                "kind": "vstack",
                "gap": 1,
                "entries": [
                    {"component": text_node("one")},
                    {"component": text_node("hidden"), "visible": false},
                    {"component": text_node("two")}
                ]
            },
            "operations": [{"kind": "frame", "width": 10, "height": 4}]
        },
        {
            "root": {
                "kind": "hstack",
                "entries": [
                    {"component": text_node("left"), "basis": 6, "shrink": 0},
                    {"component": text_node("right"), "basis": 6, "shrink": 0}
                ]
            },
            "operations": [{"kind": "frame", "width": 12, "height": 1}]
        },
        {
            "root": {
                "kind": "hstack",
                "entries": [
                    {"component": text_node("hidden"), "basis": 0, "shrink": 0},
                    {"component": text_node("shown"), "basis": 0, "grow": 1}
                ]
            },
            "operations": [{"kind": "frame", "width": 5, "height": 1}]
        },
        {
            "root": scroll_node(
                text_node("1\n2\n3\n4\n5\n6"),
                json!({"follow": "end", "primary": true})
            ),
            "operations": [
                {"kind": "frame", "width": 10, "height": 3},
                {"kind": "scrollBy", "target": "0", "lines": -2},
                {"kind": "frame", "width": 10, "height": 3},
                {"kind": "scrollBy", "target": "0", "lines": -3},
                {"kind": "scrollBy", "target": "0", "lines": 10},
                {"kind": "frame", "width": 10, "height": 3},
                {"kind": "scrollTo", "target": "0", "top": 0, "disableFollow": true},
                {"kind": "frame", "width": 10, "height": 3},
                {"kind": "scrollToEnd", "target": "0"},
                {"kind": "frame", "width": 10, "height": 3},
                {"kind": "scrollToStart", "target": "0"},
                {"kind": "frame", "width": 10, "height": 3}
            ]
        },
        {
            "root": scroll_node(
                text_node("abcd界\nabcde2\nabcde3\nabcde4\nabcde5\nabcde6\nabcde7\nabcde8"),
                json!({"scrollbar": "auto", "scrollbarStyle": "red", "scrollbarHideDelayMs": 1000})
            ),
            "operations": [
                {"kind": "frame", "width": 6, "height": 4},
                {"kind": "scrollBy", "target": "0", "lines": 2},
                {"kind": "frame", "width": 6, "height": 4},
                {"kind": "scrollToEnd", "target": "0"},
                {"kind": "frame", "width": 6, "height": 4}
            ]
        },
        {
            "root": {
                "kind": "hstack",
                "align": "start",
                "entries": [
                    {
                        "component": scroll_node(
                            text_node("123456"),
                            json!({"scrollbar": "always"})
                        )
                    }
                ]
            },
            "operations": [
                {"kind": "frame", "width": 6, "height": 2},
                {"kind": "setScrollbar", "target": "0.0", "scrollbar": "hidden"},
                {"kind": "frame", "width": 6, "height": 2}
            ]
        },
        {
            "root": scroll_node(
                json!({
                    "kind": "vstack",
                    "entries": [
                        {
                            "component": scroll_node(text_node("1\n2\n3\n4\n5\n6"), json!({})),
                            "basis": 2
                        },
                        {"component": text_node("tail")}
                    ]
                }),
                json!({})
            ),
            "operations": [
                {"kind": "frame", "width": 10, "height": 2},
                {"kind": "scrollBy", "target": "0", "lines": 10},
                {"kind": "frame", "width": 10, "height": 2}
            ]
        },
        {
            "root": {
                "kind": "vstack",
                "entries": [
                    {"component": {"kind": "text", "text": "one", "paddingX": 0, "paddingY": 0, "background": false}}
                ]
            },
            "operations": [
                {"kind": "frame", "width": 10, "height": 4},
                {"kind": "setText", "target": "0.0", "text": "one\ntwo\nthree"},
                {"kind": "frame", "width": 10, "height": 4}
            ]
        },
        {
            "root": {
                "kind": "vstack",
                "entries": [
                    {"component": text_node("one"), "visible": {"minWidth": 8}},
                    {"component": text_node("two")}
                ]
            },
            "operations": [
                {"kind": "frame", "width": 10, "height": 3},
                {"kind": "frame", "width": 5, "height": 3}
            ]
        },
        {
            "root": {
                "kind": "hstack",
                "align": "center",
                "entries": [
                    {"component": text_node("a\nb"), "basis": 3, "shrink": 0},
                    {"component": text_node("x"), "basis": 3, "shrink": 0}
                ]
            },
            "operations": [{"kind": "frame", "width": 6, "height": 3}]
        },
        {
            "root": {
                "kind": "hstack",
                "align": "end",
                "entries": [
                    {"component": text_node("a\nb"), "basis": 3, "shrink": 0},
                    {"component": text_node("x"), "basis": 3, "shrink": 0}
                ]
            },
            "operations": [{"kind": "frame", "width": 6, "height": 3}]
        },
        {
            "root": {
                "kind": "vstack",
                "entries": [
                    {
                        "component": {
                            "kind": "lines",
                            "lines": ["one", "two", "IMAGE", "", ""],
                            "registerKitty": {"imageId": 124, "columns": 2, "rows": 3, "widthPx": 100, "heightPx": 100}
                        },
                        "basis": 0,
                        "grow": 1
                    },
                    {"component": text_node("dock")}
                ]
            },
            "operations": [{"kind": "frame", "width": 20, "height": 4}]
        }
    ]);
    // The kitty case uses the real encoded sequence so both sides crop the same line.
    let mut cases = cases;
    let kitty_line = pi_tui::terminal_image::encode_kitty(
        "AAAA",
        &pi_tui::terminal_image::KittyEncodeOptions {
            columns: Some(2),
            rows: Some(3),
            image_id: Some(124),
            move_cursor: Some(false),
        },
    );
    for case in cases.as_array_mut().unwrap() {
        let root = &mut case["root"];
        if root["entries"][0]["component"]["registerKitty"].is_object() {
            root["entries"][0]["component"]["lines"][2] = json!(kitty_line);
        }
    }
    let mut oracle = TsOracle::spawn("tui-components-oracle.ts", SOURCE_DIRECTORY);
    let response = oracle.request(&json!({"id": 1, "operation": "layout", "cases": cases}));
    assert_eq!(response["ok"], json!(true), "oracle failed: {response}");

    let expected: Vec<Value> = cases.as_array().unwrap().iter().map(execute_case).collect();
    assert_eq!(response["output"]["results"], json!(expected));
}
