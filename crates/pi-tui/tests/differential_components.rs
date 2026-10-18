#[path = "support/oracle.rs"]
mod oracle;

use std::cell::RefCell;
use std::rc::Rc;

use oracle::TsOracle;
use pi_tui::components::StyleFn;
use pi_tui::components::loader::{Loader, LoaderIndicatorOptions};
use pi_tui::components::select_list::{
    SelectItem, SelectList, SelectListLayoutOptions, SelectListTheme, SelectListTruncatePrimaryContext,
};
use pi_tui::components::text::Text;
use pi_tui::tui::Component;
use serde_json::{Value, json};

const SOURCE_DIRECTORY: &str = "packages/tui/src";

fn run(oracle: &mut TsOracle, operation: &str, cases: Value) -> Value {
    let response = oracle.request(&json!({"id": 1, "operation": operation, "cases": cases}));
    assert_eq!(response["ok"], json!(true), "oracle failed: {response}");
    response["output"].clone()
}

fn identity() -> StyleFn {
    Rc::new(|text: &str| text.to_string())
}

#[test]
fn text_render_matches_typescript() {
    let cases: &[(&str, usize, usize, bool, usize)] = &[
        ("", 0, 0, false, 10),
        ("hello world", 1, 1, false, 12),
        ("alpha\tbeta gamma", 0, 0, false, 12),
        ("hi", 1, 1, true, 6),
        ("界界界", 0, 0, false, 3),
        ("line1\nline2", 2, 0, false, 10),
    ];
    let mut oracle = TsOracle::spawn("tui-components-oracle.ts", SOURCE_DIRECTORY);
    let output = run(
        &mut oracle,
        "text",
        Value::Array(
            cases
                .iter()
                .map(|(text, padding_x, padding_y, background, width)| {
                    json!({
                        "text": text,
                        "paddingX": padding_x,
                        "paddingY": padding_y,
                        "background": background,
                        "width": width,
                    })
                })
                .collect(),
        ),
    );

    let expected: Vec<Vec<String>> = cases
        .iter()
        .map(|(text, padding_x, padding_y, background, width)| {
            let mut component = if *background {
                Text::with_background(*text, *padding_x, *padding_y, Rc::new(|text: &str| format!("[{text}]")))
            } else {
                Text::new(*text, *padding_x, *padding_y)
            };
            component.render(*width)
        })
        .collect();
    assert_eq!(output["results"], json!(expected));
}

#[test]
fn loader_render_matches_typescript() {
    let cases = json!([
        {
            "message": "Loading...",
            "frames": null,
            "intervalMs": null,
            "styled": false,
            "steps": [
                {"kind": "render", "width": 20},
                {"kind": "setMessage", "message": "Updated"},
                {"kind": "render", "width": 12},
                {"kind": "stop"}
            ]
        },
        {
            "message": "Work",
            "frames": ["a", "b"],
            "intervalMs": 10,
            "styled": true,
            "steps": [
                {"kind": "render", "width": 16},
                {"kind": "setIndicator", "frames": ["x"], "intervalMs": 10},
                {"kind": "render", "width": 16},
                {"kind": "stop"}
            ]
        },
        {
            "message": "Hidden",
            "frames": [],
            "intervalMs": null,
            "styled": true,
            "steps": [
                {"kind": "render", "width": 16},
                {"kind": "setMessage", "message": "Still hidden"},
                {"kind": "render", "width": 16}
            ]
        }
    ]);
    let mut oracle = TsOracle::spawn("tui-components-oracle.ts", SOURCE_DIRECTORY);
    let output = run(&mut oracle, "loader", cases.clone());

    let mut expected = Vec::new();
    for case in cases.as_array().unwrap() {
        let renders = Rc::new(RefCell::new(0usize));
        let counter = Rc::clone(&renders);
        let styled = case["styled"].as_bool().unwrap();
        let spinner: StyleFn = if styled {
            Rc::new(|text: &str| format!("<{text}>"))
        } else {
            identity()
        };
        let message_style: StyleFn = if styled {
            Rc::new(|text: &str| format!("{{{text}}}"))
        } else {
            identity()
        };
        let indicator = if case["frames"].is_null() && case["intervalMs"].is_null() {
            None
        } else {
            Some(LoaderIndicatorOptions {
                frames: case["frames"]
                    .as_array()
                    .map(|frames| frames.iter().map(|frame| frame.as_str().unwrap().to_string()).collect()),
                interval_ms: case["intervalMs"].as_u64(),
            })
        };
        let mut loader = Loader::new(
            Rc::new(move || *counter.borrow_mut() += 1),
            spinner,
            message_style,
            case["message"].as_str().unwrap(),
            indicator,
        );
        let mut snapshots = Vec::new();
        for step in case["steps"].as_array().unwrap() {
            match step["kind"].as_str().unwrap() {
                "render" => snapshots.push(loader.render(step["width"].as_u64().unwrap() as usize)),
                "setMessage" => loader.set_message(step["message"].as_str().unwrap()),
                "setIndicator" => loader.set_indicator(Some(LoaderIndicatorOptions {
                    frames: step["frames"]
                        .as_array()
                        .map(|frames| frames.iter().map(|frame| frame.as_str().unwrap().to_string()).collect()),
                    interval_ms: step["intervalMs"].as_u64(),
                })),
                "stop" => loader.stop(),
                other => panic!("unsupported loader step: {other}"),
            }
        }
        loader.stop();
        expected.push(json!({
            "snapshots": snapshots,
            "renderCount": *renders.borrow(),
        }));
    }
    assert_eq!(output["results"], json!(expected));
}

fn select_theme(markers: bool) -> SelectListTheme {
    if !markers {
        return SelectListTheme::plain();
    }
    let style: StyleFn = Rc::new(|text: &str| format!("«{text}»"));
    SelectListTheme {
        selected_prefix: Rc::clone(&style),
        selected_text: Rc::clone(&style),
        description: Rc::clone(&style),
        scroll_info: Rc::clone(&style),
        no_match: style,
    }
}

#[test]
fn select_list_render_and_callbacks_match_typescript() {
    let cases = json!([
        {
            "items": [
                {"value": "test", "label": "test", "description": "Line one\nLine two\nLine three"}
            ],
            "maxVisible": 5,
            "markers": false,
            "layout": {"minPrimaryColumnWidth": null, "maxPrimaryColumnWidth": null, "truncatePrimary": false},
            "steps": [
                {"kind": "render", "width": 100},
                {"kind": "key", "data": "\u{001b}[A"},
                {"kind": "key", "data": "\u{001b}[B"},
                {"kind": "key", "data": "\r"},
                {"kind": "key", "data": "\u{001b}"}
            ]
        },
        {
            "items": [
                {"value": "short", "label": "short", "description": "short description"},
                {"value": "very-long-command-name-that-needs-truncation", "label": "very-long-command-name-that-needs-truncation", "description": "long description"}
            ],
            "maxVisible": 5,
            "markers": true,
            "layout": {"minPrimaryColumnWidth": null, "maxPrimaryColumnWidth": null, "truncatePrimary": false},
            "steps": [
                {"kind": "render", "width": 80},
                {"kind": "index", "index": 1},
                {"kind": "render", "width": 41}
            ]
        },
        {
            "items": [
                {"value": "a", "label": "a", "description": "first"},
                {"value": "bb", "label": "bb", "description": "second"}
            ],
            "maxVisible": 5,
            "markers": false,
            "layout": {"minPrimaryColumnWidth": 12, "maxPrimaryColumnWidth": 20, "truncatePrimary": false},
            "steps": [{"kind": "render", "width": 80}]
        },
        {
            "items": [
                {"value": "very-long-command-name-that-needs-truncation", "label": "very-long-command-name-that-needs-truncation", "description": "first"},
                {"value": "short", "label": "short", "description": "second"}
            ],
            "maxVisible": 5,
            "markers": false,
            "layout": {"minPrimaryColumnWidth": 12, "maxPrimaryColumnWidth": 12, "truncatePrimary": true},
            "steps": [{"kind": "render", "width": 80}]
        },
        {
            "items": [
                {"value": "alpha", "label": "alpha"},
                {"value": "beta", "label": "beta"},
                {"value": "alphabet", "label": "alphabet"}
            ],
            "maxVisible": 2,
            "markers": false,
            "layout": {"minPrimaryColumnWidth": null, "maxPrimaryColumnWidth": null, "truncatePrimary": false},
            "steps": [
                {"kind": "filter", "filter": "ALPHA"},
                {"kind": "render", "width": 20},
                {"kind": "key", "data": "\u{001b}[B"},
                {"kind": "key", "data": "\r"},
                {"kind": "filter", "filter": "zzz"},
                {"kind": "render", "width": 20},
                {"kind": "key", "data": "\u{0003}"}
            ]
        }
    ]);
    let mut oracle = TsOracle::spawn("tui-components-oracle.ts", SOURCE_DIRECTORY);
    let output = run(&mut oracle, "select_list", cases.clone());

    let mut expected = Vec::new();
    for case in cases.as_array().unwrap() {
        let items: Vec<SelectItem> = case["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| {
                let mut select_item = SelectItem::new(item["value"].as_str().unwrap(), item["label"].as_str().unwrap());
                if let Some(description) = item["description"].as_str() {
                    select_item = select_item.with_description(description);
                }
                select_item
            })
            .collect();
        let layout = SelectListLayoutOptions {
            min_primary_column_width: case["layout"]["minPrimaryColumnWidth"]
                .as_u64()
                .map(|value| value as usize),
            max_primary_column_width: case["layout"]["maxPrimaryColumnWidth"]
                .as_u64()
                .map(|value| value as usize),
            truncate_primary: case["layout"]["truncatePrimary"].as_bool().unwrap().then(|| {
                Rc::new(|context: &SelectListTruncatePrimaryContext<'_>| {
                    if context.text.len() <= context.max_width {
                        return context.text.to_string();
                    }
                    let keep = context.max_width.saturating_sub(1);
                    format!("{}…", &context.text[..keep.min(context.text.len())])
                }) as Rc<dyn Fn(&SelectListTruncatePrimaryContext<'_>) -> String>
            }),
        };
        let mut list = SelectList::new(
            items,
            case["maxVisible"].as_u64().unwrap() as usize,
            select_theme(case["markers"].as_bool().unwrap()),
            layout,
        );
        let log = Rc::new(RefCell::new(Vec::new()));
        let select_log = Rc::clone(&log);
        list.on_select = Some(Rc::new(move |item: &SelectItem| {
            select_log.borrow_mut().push(format!("select:{}", item.value));
        }));
        let cancel_log = Rc::clone(&log);
        list.on_cancel = Some(Rc::new(move || cancel_log.borrow_mut().push("cancel".to_string())));
        let change_log = Rc::clone(&log);
        list.on_selection_change = Some(Rc::new(move |item: &SelectItem| {
            change_log.borrow_mut().push(format!("change:{}", item.value));
        }));

        let mut snapshots = Vec::new();
        for step in case["steps"].as_array().unwrap() {
            match step["kind"].as_str().unwrap() {
                "filter" => list.set_filter(step["filter"].as_str().unwrap()),
                "index" => list.set_selected_index(step["index"].as_i64().unwrap() as isize),
                "key" => list.handle_input(step["data"].as_str().unwrap()),
                "render" => snapshots.push(list.render(step["width"].as_u64().unwrap() as usize)),
                other => panic!("unsupported select-list step: {other}"),
            }
        }
        expected.push(json!({
            "snapshots": snapshots,
            "log": log.borrow().clone(),
            "selected": list.get_selected_item().map(|item| item.value.clone()),
        }));
    }
    assert_eq!(output["results"], json!(expected));
}
