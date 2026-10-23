#[path = "support/components.rs"]
mod components;
#[path = "support/oracle.rs"]
mod oracle;

use pi_js_compat::js_json::JsString;
use std::cell::RefCell;
use std::rc::Rc;

use components::test_component;
use oracle::TsOracle;
use pi_tui::components::StyleFn;
use pi_tui::components::r#box::BoxComponent;
use pi_tui::components::cancellable_loader::CancellableLoader;
use pi_tui::components::dynamic_border::DynamicBorder;
use pi_tui::components::image::{Image, ImageOptions, ImageTheme};
use pi_tui::components::loader::{Loader, LoaderIndicatorOptions};
use pi_tui::components::select_list::{
    SelectItem, SelectList, SelectListLayoutOptions, SelectListTheme, SelectListTruncatePrimaryContext,
};
use pi_tui::components::spacer::Spacer;
use pi_tui::components::text::Text;
use pi_tui::components::truncated_text::TruncatedText;
use pi_tui::terminal_image::ImageDimensions;
use pi_tui::tui::Component;
use serde_json::{Value, json};

const SOURCE_DIRECTORY: &str = "packages/tui/src";

fn run(oracle: &mut TsOracle, operation: &str, cases: Value) -> Value {
    let response = oracle.request(&json!({"id": 1, "operation": operation, "cases": cases}));
    assert_eq!(response["ok"], json!(true), "oracle failed: {response}");
    response["output"].clone()
}

fn identity() -> StyleFn {
    Rc::new(|text: &JsString| text.clone())
}

#[test]
fn spacer_default_and_explicit_zero_match_typescript() {
    let mut oracle = TsOracle::spawn("tui-components-oracle.ts", SOURCE_DIRECTORY);
    let output = run(
        &mut oracle,
        "spacer",
        json!([
            {"width": 8, "setLines": null}, {"width": 8, "lines": 0, "setLines": null}
        ]),
    );
    assert_eq!(
        output["results"],
        json!([
            {"snapshots": [Spacer::default().render(8)]}, {"snapshots": [Spacer::new(0).render(8)]}
        ])
    );
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

    let expected: Vec<Vec<JsString>> = cases
        .iter()
        .map(|(text, padding_x, padding_y, background, width)| {
            let mut component = if *background {
                Text::with_background(
                    *text,
                    *padding_x,
                    *padding_y,
                    Rc::new(|text: &JsString| JsString::from("[") + text + "]"),
                )
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
            Rc::new(|text: &JsString| JsString::from("<") + text + ">")
        } else {
            identity()
        };
        let message_style: StyleFn = if styled {
            Rc::new(|text: &JsString| JsString::from("{") + text + "}")
        } else {
            identity()
        };
        let indicator = if case["frames"].is_null() && case["intervalMs"].is_null() {
            None
        } else {
            Some(LoaderIndicatorOptions {
                frames: case["frames"].as_array().map(|frames| {
                    frames
                        .iter()
                        .map(|frame| JsString::from(frame.as_str().unwrap()))
                        .collect()
                }),
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
                    frames: step["frames"].as_array().map(|frames| {
                        frames
                            .iter()
                            .map(|frame| JsString::from(frame.as_str().unwrap()))
                            .collect()
                    }),
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
    let style: StyleFn = Rc::new(|text: &JsString| JsString::from("«") + text + "»");
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
                    if (context.text.len() as i64) <= context.max_width {
                        return context.text.clone();
                    }
                    let keep = context.max_width.saturating_sub(1).max(0) as usize;
                    context.text.slice(0, keep.min(context.text.len())) + "…"
                }) as Rc<dyn Fn(&SelectListTruncatePrimaryContext<'_>) -> JsString>
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
            select_log.borrow_mut().push(JsString::from("select:") + &item.value);
        }));
        let cancel_log = Rc::clone(&log);
        list.on_cancel = Some(Rc::new(move || cancel_log.borrow_mut().push(JsString::from("cancel"))));
        let change_log = Rc::clone(&log);
        list.on_selection_change = Some(Rc::new(move |item: &SelectItem| {
            change_log.borrow_mut().push(JsString::from("change:") + &item.value);
        }));

        let mut snapshots = Vec::new();
        for step in case["steps"].as_array().unwrap() {
            match step["kind"].as_str().unwrap() {
                "filter" => list.set_filter(step["filter"].as_str().unwrap()),
                "index" => list.set_selected_index(step["index"].as_i64().unwrap() as isize),
                "key" => list.handle_input(&pi_js_compat::js_json::JsString::from(step["data"].as_str().unwrap())),
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

#[test]
fn box_render_matches_typescript() {
    let cases = json!([
        {"paddingX": 1, "paddingY": 1, "background": false, "childLines": ["a", "bb"], "width": 8, "setBackgroundAfterFirstRender": false},
        {"paddingX": 0, "paddingY": 2, "background": true, "childLines": ["alpha"], "width": 6, "setBackgroundAfterFirstRender": false},
        {"paddingX": 1, "paddingY": 0, "background": true, "childLines": ["x"], "width": 5, "setBackgroundAfterFirstRender": true},
        {"paddingX": 1, "paddingY": 1, "background": false, "childLines": [], "width": 5, "setBackgroundAfterFirstRender": false}
    ]);
    let mut oracle = TsOracle::spawn("tui-components-oracle.ts", SOURCE_DIRECTORY);
    let output = run(&mut oracle, "box", cases.clone());

    let mut expected = Vec::new();
    for case in cases.as_array().unwrap() {
        let background: Option<StyleFn> = case["background"]
            .as_bool()
            .unwrap()
            .then(|| Rc::new(|text: &JsString| JsString::from("[") + text + "]") as StyleFn);
        let mut box_component = BoxComponent::new(
            case["paddingX"].as_u64().unwrap() as usize,
            case["paddingY"].as_u64().unwrap() as usize,
            background,
        );
        let lines: Vec<&str> = case["childLines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|line| line.as_str().unwrap())
            .collect();
        let (child, _) = test_component(&lines);
        box_component.add_child(child);
        let width = case["width"].as_u64().unwrap() as usize;
        let mut snapshots = vec![box_component.render(width)];
        if case["setBackgroundAfterFirstRender"].as_bool().unwrap() {
            box_component.set_bg_fn(Some(Rc::new(|text: &JsString| JsString::from("<") + text + ">")));
            snapshots.push(box_component.render(width));
        }
        expected.push(json!({"snapshots": snapshots}));
    }
    assert_eq!(output["results"], json!(expected));
}

#[test]
fn spacer_dynamic_border_and_truncated_text_match_typescript() {
    let mut oracle = TsOracle::spawn("tui-components-oracle.ts", SOURCE_DIRECTORY);

    let spacer_cases = json!([
        {"lines": 2, "width": 4, "setLines": null},
        {"lines": 0, "width": 4, "setLines": 1}
    ]);
    let spacer_output = run(&mut oracle, "spacer", spacer_cases.clone());
    let mut spacer_expected = Vec::new();
    for case in spacer_cases.as_array().unwrap() {
        let mut spacer = Spacer::new(case["lines"].as_u64().unwrap() as usize);
        let width = case["width"].as_u64().unwrap() as usize;
        let mut snapshots = vec![spacer.render(width)];
        if let Some(lines) = case["setLines"].as_u64() {
            spacer.set_lines(lines as usize);
            snapshots.push(spacer.render(width));
        }
        spacer_expected.push(json!({"snapshots": snapshots}));
    }
    assert_eq!(spacer_output["results"], json!(spacer_expected));

    let border_cases = json!([
        {"width": 4, "markers": false},
        {"width": 3, "markers": true},
        {"width": 0, "markers": false}
    ]);
    let border_output = run(&mut oracle, "dynamic_border", border_cases.clone());
    let border_expected: Vec<Vec<JsString>> = border_cases
        .as_array()
        .unwrap()
        .iter()
        .map(|case| {
            let mut border = if case["markers"].as_bool().unwrap() {
                DynamicBorder::new(Rc::new(|line: &JsString| JsString::from("[") + line + "]"))
            } else {
                DynamicBorder::default()
            };
            border.render(case["width"].as_u64().unwrap() as usize)
        })
        .collect();
    assert_eq!(border_output["results"], json!(border_expected));

    let truncated_cases = json!([
        {"text": "Hello world", "paddingX": 1, "paddingY": 0, "width": 50},
        {"text": "Hello", "paddingX": 0, "paddingY": 2, "width": 40},
        {"text": "This is a very long piece of text that will definitely exceed the available width", "paddingX": 1, "paddingY": 0, "width": 30},
        {"text": "First line\nSecond line\nThird line", "paddingX": 1, "paddingY": 0, "width": 40},
        {"text": "", "paddingX": 1, "paddingY": 0, "width": 30}
    ]);
    let truncated_output = run(&mut oracle, "truncated_text", truncated_cases.clone());
    let truncated_expected: Vec<Vec<JsString>> = truncated_cases
        .as_array()
        .unwrap()
        .iter()
        .map(|case| {
            let mut text = TruncatedText::new(
                case["text"].as_str().unwrap(),
                case["paddingX"].as_u64().unwrap() as usize,
                case["paddingY"].as_u64().unwrap() as usize,
            );
            text.render(case["width"].as_u64().unwrap() as usize)
        })
        .collect();
    assert_eq!(truncated_output["results"], json!(truncated_expected));
}

#[test]
fn cancellable_loader_matches_typescript() {
    let cases = json!([
        {
            "message": "Working",
            "frames": null,
            "steps": [
                {"kind": "render", "width": 20},
                {"kind": "key", "data": "\u{001b}"},
                {"kind": "key", "data": "\u{0003}"},
                {"kind": "setMessage", "message": "Still going"},
                {"kind": "render", "width": 20}
            ]
        },
        {
            "message": "No indicator",
            "frames": [],
            "steps": [{"kind": "render", "width": 12}]
        }
    ]);
    let mut oracle = TsOracle::spawn("tui-components-oracle.ts", SOURCE_DIRECTORY);
    let output = run(&mut oracle, "cancellable_loader", cases.clone());

    let mut expected = Vec::new();
    for case in cases.as_array().unwrap() {
        let renders = Rc::new(RefCell::new(0usize));
        let counter = Rc::clone(&renders);
        let indicator = case["frames"].as_array().map(|frames| LoaderIndicatorOptions {
            frames: Some(
                frames
                    .iter()
                    .map(|frame| JsString::from(frame.as_str().unwrap()))
                    .collect(),
            ),
            interval_ms: Some(250),
        });
        let mut loader = CancellableLoader::new(
            Rc::new(move || *counter.borrow_mut() += 1),
            identity(),
            identity(),
            case["message"].as_str().unwrap(),
            indicator,
        );
        let log = Rc::new(RefCell::new(Vec::new()));
        let abort_log = Rc::clone(&log);
        loader.on_abort = Some(Rc::new(move || abort_log.borrow_mut().push("abort".to_string())));
        let mut snapshots = Vec::new();
        for step in case["steps"].as_array().unwrap() {
            match step["kind"].as_str().unwrap() {
                "key" => loader.handle_input(&pi_js_compat::js_json::JsString::from(step["data"].as_str().unwrap())),
                "render" => snapshots.push(loader.render(step["width"].as_u64().unwrap() as usize)),
                "setMessage" => loader.set_message(step["message"].as_str().unwrap()),
                other => panic!("unsupported cancellable-loader step: {other}"),
            }
        }
        let aborted = loader.is_aborted();
        loader.stop();
        expected.push(json!({
            "snapshots": snapshots,
            "log": log.borrow().clone(),
            "aborted": aborted,
            "renderCount": *renders.borrow(),
        }));
    }
    assert_eq!(output["results"], json!(expected));
}

static CAPS_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn image_render_matches_typescript() {
    let _guard = CAPS_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let cases = json!([
        {
            "images": "kitty",
            "cellWidthPx": 10,
            "cellHeightPx": 20,
            "base64Data": "AAAA",
            "mimeType": "image/png",
            "dimensions": {"widthPx": 100, "heightPx": 100},
            "options": {"maxWidthCells": 20, "maxHeightCells": null, "filename": "cat.png", "imageId": 7},
            "width": 30
        },
        {
            "images": "iterm2",
            "cellWidthPx": 10,
            "cellHeightPx": 20,
            "base64Data": "AAAA",
            "mimeType": "image/png",
            "dimensions": {"widthPx": 40, "heightPx": 80},
            "options": {"maxWidthCells": 10, "maxHeightCells": 3, "filename": null, "imageId": null},
            "width": 20
        },
        {
            "images": null,
            "cellWidthPx": 10,
            "cellHeightPx": 20,
            "base64Data": "AAAA",
            "mimeType": "image/png",
            "dimensions": {"widthPx": 800, "heightPx": 600},
            "options": {"maxWidthCells": null, "maxHeightCells": null, "filename": "photo.png", "imageId": null},
            "width": 40
        },
        {
            "images": null,
            "cellWidthPx": 10,
            "cellHeightPx": 20,
            "base64Data": "AAAA",
            "mimeType": "image/webp",
            "dimensions": {"widthPx": 800, "heightPx": 600},
            "options": {"maxWidthCells": null, "maxHeightCells": null, "filename": null, "imageId": null},
            "width": 60
        }
    ]);
    let mut oracle = TsOracle::spawn("tui-components-oracle.ts", SOURCE_DIRECTORY);
    let output = run(&mut oracle, "image", cases.clone());

    let mut expected = Vec::new();
    for case in cases.as_array().unwrap() {
        let images = match case["images"].as_str() {
            Some("kitty") => Some(pi_tui::terminal_image::ImageProtocol::Kitty),
            Some("iterm2") => Some(pi_tui::terminal_image::ImageProtocol::Iterm2),
            _ => None,
        };
        pi_tui::terminal_image::set_capabilities(pi_tui::terminal_image::TerminalCapabilities {
            images,
            true_color: true,
            hyperlinks: true,
        });
        pi_tui::terminal_image::set_cell_dimensions(pi_tui::terminal_image::CellDimensions {
            width_px: case["cellWidthPx"].as_u64().unwrap() as u32,
            height_px: case["cellHeightPx"].as_u64().unwrap() as u32,
        });
        let mut image = Image::new(
            case["base64Data"].as_str().unwrap(),
            case["mimeType"].as_str().unwrap(),
            Rc::new(ImageTheme {
                fallback_color: Rc::new(|text: &JsString| JsString::from("<") + text + ">"),
            }),
            ImageOptions {
                max_width_cells: case["options"]["maxWidthCells"].as_f64(),
                max_height_cells: case["options"]["maxHeightCells"].as_f64(),
                filename: case["options"]["filename"].as_str().map(JsString::from),
                image_id: case["options"]["imageId"].as_u64().map(|value| value as u32),
            },
            Some(ImageDimensions {
                width_px: case["dimensions"]["widthPx"].as_u64().unwrap() as u32,
                height_px: case["dimensions"]["heightPx"].as_u64().unwrap() as u32,
            }),
        );
        let lines = image.render(case["width"].as_u64().unwrap() as usize);
        expected.push(json!({"lines": lines, "imageId": image.image_id()}));
        pi_tui::terminal_image::reset_capabilities_cache();
    }
    assert_eq!(output["results"], json!(expected));
}
