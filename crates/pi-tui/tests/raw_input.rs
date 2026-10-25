use crate::shared_oracle as oracle;

use pi_js_compat::js_json::{JsString, parse_lossless};
use pi_tui::components::input::{Input, InputOptions};
use pi_tui::tui::Component;
use serde::Serialize;
use serde_json::{Value, json};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Serialize)]
struct State {
    value: JsString,
    cursor: usize,
    lines: Vec<JsString>,
}
#[derive(Serialize)]
struct Output {
    states: Vec<State>,
    submitted: Vec<JsString>,
}

#[test]
fn native_input_edits_paste_kill_yank_undo_and_submit_match_frozen_source() {
    let mut oracle = oracle::TsOracle::spawn("raw-tui-oracle.ts", "packages/tui/src");
    let scenarios = [
        json!([{ "key": [27,91,53,53,50,57,54,117] }, { "key": [27,91,53,54,51,50,48,117] }, { "key": [13] }]),
        json!([{ "key": [97] }, { "set": [55357,56832] }, { "key": [120] }, { "key": [13] }]),
        json!([{ "key": [97] }, { "set": [55357,56832] }, { "key": [127] }, { "key": [31] }]),
        json!([{ "key": [97] }, { "set": [55357,56832] }, { "key": [27,91,51,126] }, { "key": [31] }]),
        json!([{ "set": [55296,97,32,56320,55357,56832] }, { "key": [5] }, { "key": [23] }, { "key": [25] }, { "key": [1] }, { "key": [11] }, { "key": [25] }, { "key": [13] }]),
        json!([{ "key": [27,91,50,48,48,126,55296,13,10,9,56320] }, { "key": [55357,56832,27,91,50,48,49,126,120] }, { "key": [27,91,68] }, { "key": [127] }, { "key": [31] }]),
        json!([{ "key": [55296] }, { "key": [56320] }, { "key": [27,91,68] }, { "key": [120] }, { "key": [13] }]),
    ];
    for actions in scenarios {
        for mask in [false, true] {
            for width in 1..=9 {
                let response =
                    oracle.request(&json!({"operation":"input", "actions":actions, "width":width, "mask":mask}));
                assert_eq!(response["ok"], true, "{response}");
                let submitted = Rc::new(RefCell::new(Vec::new()));
                let mut input = Input::new(InputOptions { mask });
                let values = submitted.clone();
                input.on_submit = Some(Rc::new(move |value| values.borrow_mut().push(value.clone())));
                let states = actions
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|action| {
                        if let Some(units) = action.get("set") {
                            input.set_value(native(units));
                        }
                        if let Some(units) = action.get("key") {
                            input.handle_input(native(units));
                        }
                        State {
                            value: input.get_value().clone(),
                            cursor: input.cursor(),
                            lines: input.render(width),
                        }
                    })
                    .collect();
                let actual = Output {
                    states,
                    submitted: submitted.borrow().clone(),
                };
                let actual = parse_lossless(serde_json::to_string(&actual).unwrap()).unwrap();
                let expected = parse_lossless(response["json"].as_str().unwrap()).unwrap();
                assert_eq!(actual, expected, "mask={mask} width={width} actions={actions}");
            }
        }
    }
}

fn native(units: &Value) -> JsString {
    JsString::from_utf16(
        units
            .as_array()
            .unwrap()
            .iter()
            .map(|unit| unit.as_u64().unwrap() as u16)
            .collect(),
    )
}
