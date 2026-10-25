use crate::shared_oracle as oracle;

use pi_js_compat::js_json::JsString;
use std::cell::RefCell;
use std::rc::Rc;

use oracle::TsOracle;
use pi_tui::components::settings_list::{
    SettingItem, SettingsList, SettingsListOptions, SettingsListTheme, SubmenuDone,
};
use pi_tui::tui::{Component, component};
use serde_json::{Value, json};

const SOURCE_DIRECTORY: &str = "packages/tui/src";

fn theme(markers: bool) -> SettingsListTheme {
    if !markers {
        return SettingsListTheme::plain("> ");
    }
    SettingsListTheme {
        label: Rc::new(|text: &JsString, selected: bool| JsString::from(if selected { "L+" } else { "L-" }) + text),
        value: Rc::new(|text: &JsString, selected: bool| JsString::from(if selected { "V+" } else { "V-" }) + text),
        description: Rc::new(|text: &JsString| JsString::from("«") + text + "»"),
        cursor: "> ".into(),
        hint: Rc::new(|text: &JsString| JsString::from("«") + text + "»"),
    }
}

fn build_item(value: &Value) -> SettingItem {
    let id = JsString::from(value["id"].as_str().unwrap());
    let submenu = value.get("submenu").map(|submenu| {
        let option = JsString::from(submenu["options"][0].as_str().unwrap());
        let handle: Rc<pi_tui::components::settings_list::SubmenuFactory> =
            Rc::new(move |current_value: &JsString, done: SubmenuDone| {
                component(SubmenuWithDone {
                    label: current_value.clone(),
                    option: option.clone(),
                    done: Rc::clone(&done),
                })
            });
        handle
    });
    SettingItem {
        id,
        label: value["label"].as_str().unwrap().into(),
        description: value["description"].as_str().map(JsString::from),
        current_value: value["currentValue"].as_str().unwrap().into(),
        values: value["values"]
            .as_array()
            .map(|values| values.iter().map(|value| value.as_str().unwrap().into()).collect()),
        submenu,
    }
}

struct SubmenuWithDone {
    label: JsString,
    option: JsString,
    done: SubmenuDone,
}

impl Component for SubmenuWithDone {
    fn render(&mut self, _width: usize) -> Vec<JsString> {
        vec![JsString::from("submenu:") + &self.label]
    }

    fn handle_input(&mut self, data: &JsString) {
        if data == "\r" {
            let done = Rc::clone(&self.done);
            let option = self.option.clone();
            done(Some(option));
        } else if data == "\x1b" {
            let done = Rc::clone(&self.done);
            done(None);
        }
    }
}

fn execute_case(case: &Value) -> Value {
    let items: Vec<SettingItem> = case["items"].as_array().unwrap().iter().map(build_item).collect();
    let changes = Rc::new(RefCell::new(Vec::new()));
    let cancels = Rc::new(RefCell::new(0usize));
    let change_sink = Rc::clone(&changes);
    let cancel_sink = Rc::clone(&cancels);
    let mut list = SettingsList::new(
        items,
        case["maxVisible"].as_u64().unwrap() as usize,
        theme(case["markers"].as_bool().unwrap()),
        Rc::new(move |id: &JsString, value: &JsString| {
            change_sink.borrow_mut().push(id.clone() + "=" + value);
        }),
        Rc::new(move || *cancel_sink.borrow_mut() += 1),
        SettingsListOptions {
            enable_search: case["enableSearch"].as_bool().unwrap(),
        },
    );

    let mut snapshots = Vec::new();
    for step in case["steps"].as_array().unwrap() {
        match step["kind"].as_str().unwrap() {
            "key" => list.handle_input(&pi_js_compat::js_json::JsString::from(step["data"].as_str().unwrap())),
            "render" => snapshots.push(list.render(step["width"].as_u64().unwrap() as usize)),
            "updateValue" => list.update_value(step["id"].as_str().unwrap(), step["value"].as_str().unwrap()),
            other => panic!("unsupported settings step: {other}"),
        }
    }
    json!({
        "snapshots": snapshots,
        "changes": changes.borrow().clone(),
        "cancels": *cancels.borrow(),
    })
}

#[test]
fn empty_description_skips_block_but_whitespace_does_not() {
    let cases = Value::Array(
        [None, Some(""), Some(" "), Some("description")]
            .into_iter()
            .map(|description| {
                json!({
                    "items": [{"id": "x", "label": "X", "currentValue": "on", "description": description}],
                    "maxVisible": 5, "enableSearch": false, "markers": true,
                    "steps": [{"kind": "render", "width": 80}]
                })
            })
            .collect(),
    );
    let mut oracle = TsOracle::spawn("tui-components-oracle.ts", SOURCE_DIRECTORY);
    let response = oracle.request(&json!({"id": 1, "operation": "settings_list", "cases": cases}));
    assert_eq!(response["ok"], true, "{response}");
    let actual: Vec<Value> = cases.as_array().unwrap().iter().map(execute_case).collect();
    assert_eq!(response["output"]["results"], json!(actual));
}

#[test]
fn settings_list_rendering_and_callbacks_match_typescript() {
    let cases = json!([
        {
            "items": [
                {"id": "tui-mode", "label": "TUI mode", "currentValue": "regular", "values": ["regular", "fullscreen"]}
            ],
            "maxVisible": 10,
            "enableSearch": true,
            "markers": false,
            "steps": [
                {"kind": "render", "width": 80},
                {"kind": "key", "data": "T"},
                {"kind": "key", "data": "U"},
                {"kind": "key", "data": "I"},
                {"kind": "key", "data": " "},
                {"kind": "render", "width": 80},
                {"kind": "key", "data": "\r"},
                {"kind": "render", "width": 80}
            ]
        },
        {
            "items": [
                {"id": "tui-mode", "label": "TUI mode", "currentValue": "regular", "values": ["regular", "fullscreen"]}
            ],
            "maxVisible": 10,
            "enableSearch": true,
            "markers": true,
            "steps": [
                {"kind": "key", "data": " "},
                {"kind": "render", "width": 60}
            ]
        },
        {
            "items": [
                {"id": "alpha", "label": "Alpha", "description": "First setting with a longer description that wraps", "currentValue": "on", "values": ["on", "off"]},
                {"id": "beta", "label": "Beta", "currentValue": "blue", "values": ["blue", "green"]},
                {"id": "gamma", "label": "Gamma", "currentValue": "1"}
            ],
            "maxVisible": 2,
            "enableSearch": false,
            "markers": true,
            "steps": [
                {"kind": "render", "width": 40},
                {"kind": "key", "data": "\u{1b}[B"},
                {"kind": "key", "data": "\u{1b}[B"},
                {"kind": "render", "width": 40},
                {"kind": "key", "data": "\u{1b}[A"},
                {"kind": "key", "data": "\u{1b}[A"},
                {"kind": "key", "data": "\r"},
                {"kind": "render", "width": 40},
                {"kind": "updateValue", "id": "alpha", "value": "off"},
                {"kind": "render", "width": 40}
            ]
        },
        {
            "items": [
                {"id": "theme", "label": "Theme", "currentValue": "dark", "submenu": {"options": ["light"]}},
                {"id": "size", "label": "Size", "currentValue": "12"}
            ],
            "maxVisible": 5,
            "enableSearch": false,
            "markers": false,
            "steps": [
                {"kind": "render", "width": 40},
                {"kind": "key", "data": "\r"},
                {"kind": "render", "width": 40},
                {"kind": "key", "data": "\r"},
                {"kind": "render", "width": 40},
                {"kind": "key", "data": "\u{1b}"},
                {"kind": "key", "data": "\u{1b}"},
                {"kind": "render", "width": 40}
            ]
        },
        {
            "items": [
                {"id": "alpha", "label": "Alpha", "currentValue": "on", "values": ["on", "off"]}
            ],
            "maxVisible": 5,
            "enableSearch": true,
            "markers": false,
            "steps": [
                {"kind": "key", "data": "z"},
                {"kind": "key", "data": "z"},
                {"kind": "render", "width": 40},
                {"kind": "key", "data": "\u{1b}"}
            ]
        },
        {
            "items": [],
            "maxVisible": 5,
            "enableSearch": true,
            "markers": false,
            "steps": [{"kind": "render", "width": 40}]
        }
    ]);
    let mut oracle = TsOracle::spawn("tui-components-oracle.ts", SOURCE_DIRECTORY);
    let response = oracle.request(&json!({"id": 1, "operation": "settings_list", "cases": cases}));
    assert_eq!(response["ok"], json!(true), "oracle failed: {response}");

    let expected: Vec<Value> = cases.as_array().unwrap().iter().map(execute_case).collect();
    assert_eq!(response["output"]["results"], json!(expected));
}
