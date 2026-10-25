use crate::shared_oracle as oracle;

use pi_js_compat::js_json::JsString;
use std::cell::RefCell;
use std::rc::Rc;

use oracle::TsOracle;
use pi_tui::components::input::{Input, InputOptions};
use pi_tui::tui::{Component, Focusable};
use serde_json::{Value, json};

const SOURCE_DIRECTORY: &str = "packages/tui/src";

#[derive(Clone, Copy)]
enum Step {
    SetValue(&'static str),
    Key(&'static str),
    Render(usize),
    Value,
}

struct Case {
    mask: bool,
    focused: bool,
    steps: &'static [Step],
}

const CASES: &[Case] = &[
    Case {
        mask: false,
        focused: false,
        steps: &[
            Step::Key("h"),
            Step::Key("e"),
            Step::Key("l"),
            Step::Key("l"),
            Step::Key("o"),
            Step::Key("\\"),
            Step::Key("\r"),
            Step::Value,
            Step::Key("x"),
            Step::Value,
        ],
    },
    Case {
        mask: false,
        focused: true,
        steps: &[
            Step::SetValue("가나다라마바사아자차카타파하"),
            Step::Render(20),
            Step::Key("\x1b[C"),
            Step::Key("\x1b[C"),
            Step::Key("\x1b[C"),
            Step::Render(20),
            Step::Key("\x05"),
            Step::Render(20),
            Step::Key("\x1b[D"),
            Step::Render(12),
        ],
    },
    Case {
        mask: true,
        focused: true,
        steps: &[
            Step::SetValue("s3cr3t"),
            Step::Render(20),
            Step::Key("\x1b[C"),
            Step::Key("x"),
            Step::Render(12),
            Step::Value,
        ],
    },
    Case {
        mask: false,
        focused: false,
        steps: &[
            Step::SetValue("foo bar baz"),
            Step::Key("\x05"),
            Step::Key("\x17"),
            Step::Value,
            Step::Key("\x01"),
            Step::Key("\x19"),
            Step::Value,
            Step::Key("\x1by"),
            Step::Value,
        ],
    },
    Case {
        mask: false,
        focused: false,
        steps: &[
            Step::SetValue("你好世界。你好，世界"),
            Step::Key("\x05"),
            Step::Key("\x17"),
            Step::Value,
            Step::Key("\x17"),
            Step::Value,
            Step::Key("\x17"),
            Step::Value,
            Step::Key("\x01"),
            Step::Key("\x1bd"),
            Step::Value,
            Step::Key("\x1bd"),
            Step::Value,
        ],
    },
    Case {
        mask: false,
        focused: false,
        steps: &[
            Step::SetValue("first"),
            Step::Key("\x05"),
            Step::Key("\x17"),
            Step::SetValue("second"),
            Step::Key("\x05"),
            Step::Key("\x17"),
            Step::SetValue(""),
            Step::Key("\x19"),
            Step::Value,
            Step::Key("\x1by"),
            Step::Value,
            Step::Key("\x1by"),
            Step::Value,
        ],
    },
    Case {
        mask: false,
        focused: false,
        steps: &[
            Step::SetValue("hello world"),
            Step::Key("\x01"),
            Step::Key("\x1b[C"),
            Step::Key("\x1b[C"),
            Step::Key("\x1b[C"),
            Step::Key("\x1b[C"),
            Step::Key("\x1b[C"),
            Step::Key("\x1b[C"),
            Step::Key("\x0b"),
            Step::Value,
            Step::Key("\x19"),
            Step::Value,
            Step::Key("\x1b[45;5u"),
            Step::Value,
        ],
    },
    Case {
        mask: false,
        focused: false,
        steps: &[
            Step::Key("h"),
            Step::Key("e"),
            Step::Key("l"),
            Step::Key("l"),
            Step::Key("o"),
            Step::Key(" "),
            Step::Key("w"),
            Step::Key("o"),
            Step::Key("r"),
            Step::Key("l"),
            Step::Key("d"),
            Step::Key("\x1b[45;5u"),
            Step::Value,
            Step::Key("\x1b[45;5u"),
            Step::Value,
            Step::Key("\x1b[45;5u"),
            Step::Value,
        ],
    },
    Case {
        mask: false,
        focused: false,
        steps: &[
            Step::SetValue("hello world"),
            Step::Key("\x01"),
            Step::Key("\x1b[C"),
            Step::Key("\x1b[C"),
            Step::Key("\x1b[C"),
            Step::Key("\x1b[C"),
            Step::Key("\x1b[C"),
            Step::Key("\x1b[200~beep boop\x1b[201~"),
            Step::Value,
            Step::Render(24),
            Step::Key("\x1b[45;5u"),
            Step::Value,
        ],
    },
    Case {
        mask: false,
        focused: false,
        steps: &[
            Step::Key("\x1b[97u"),
            Step::Key("\x1b[98;5u"),
            Step::Key("\u{1}"),
            Step::Key("\u{7f}"),
            Step::Value,
        ],
    },
    Case {
        mask: false,
        focused: false,
        steps: &[
            Step::SetValue("hello world test"),
            Step::Key("\x01"),
            Step::Key("\x1bd"),
            Step::Value,
            Step::Key("\x1bd"),
            Step::Value,
            Step::Key("\x19"),
            Step::Value,
        ],
    },
    Case {
        mask: false,
        focused: false,
        steps: &[
            Step::SetValue("prefix|suffix"),
            Step::Key("\x01"),
            Step::Key("\x1b[C"),
            Step::Key("\x1b[C"),
            Step::Key("\x1b[C"),
            Step::Key("\x1b[C"),
            Step::Key("\x1b[C"),
            Step::Key("\x1b[C"),
            Step::Key("\x15"),
            Step::Value,
            Step::Key("\x19"),
            Step::Value,
            Step::Key("\x1b[45;5u"),
            Step::Value,
        ],
    },
    Case {
        mask: false,
        focused: false,
        steps: &[
            Step::Key("a"),
            Step::Key("b"),
            Step::Key("c"),
            Step::Key("\x01"),
            Step::Key("\x05"),
            Step::Key("d"),
            Step::Key("e"),
            Step::Value,
            Step::Key("\x1b[45;5u"),
            Step::Value,
            Step::Key("\x1b[45;5u"),
            Step::Value,
        ],
    },
];

fn step_json(step: &Step) -> Value {
    match step {
        Step::SetValue(value) => json!({"kind": "setValue", "value": value}),
        Step::Key(data) => json!({"kind": "key", "data": data}),
        Step::Render(width) => json!({"kind": "render", "width": width}),
        Step::Value => json!({"kind": "value"}),
    }
}

#[test]
fn input_editing_and_rendering_match_typescript() {
    let cases = Value::Array(
        CASES
            .iter()
            .map(|case| {
                json!({
                    "mask": case.mask,
                    "focused": case.focused,
                    "steps": case.steps.iter().map(step_json).collect::<Vec<_>>(),
                })
            })
            .collect(),
    );
    let mut oracle = TsOracle::spawn("tui-components-oracle.ts", SOURCE_DIRECTORY);
    let response = oracle.request(&json!({"id": 1, "operation": "input", "cases": cases}));
    assert_eq!(response["ok"], json!(true), "oracle failed: {response}");

    let mut expected = Vec::new();
    for case in CASES {
        let mut input = Input::new(InputOptions { mask: case.mask });
        input.set_focused(case.focused);
        let submitted = Rc::new(RefCell::new(Vec::new()));
        let sink = Rc::clone(&submitted);
        input.on_submit = Some(Rc::new(move |value: &JsString| {
            sink.borrow_mut().push(value.clone());
        }));
        let escapes = Rc::new(RefCell::new(0usize));
        let escape_sink = Rc::clone(&escapes);
        input.on_escape = Some(Rc::new(move || {
            *escape_sink.borrow_mut() += 1;
        }));

        let mut lines = Vec::new();
        let mut values = Vec::new();
        for step in case.steps {
            match step {
                Step::SetValue(value) => input.set_value(*value),
                Step::Key(data) => input.handle_input(pi_js_compat::js_json::JsString::from(*data)),
                Step::Render(width) => lines.push(input.render(*width)),
                Step::Value => values.push(input.get_value().clone()),
            }
        }
        expected.push(json!({
            "lines": lines,
            "values": values,
            "submitted": submitted.borrow().clone(),
            "escapes": *escapes.borrow(),
        }));
    }

    assert_eq!(response["output"]["results"], json!(expected));
}
