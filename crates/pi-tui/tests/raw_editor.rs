use crate::shared_oracle as oracle;
use pi_js_compat::js_json::{JsString, parse_lossless};
use pi_tui::components::editor::{Editor, EditorOptions, EditorTheme};
use pi_tui::terminal::ProcessTerminal;
use pi_tui::tui::{Component, TuiRenderer};
use serde_json::json;
use std::{cell::RefCell, rc::Rc};

#[test]
fn editor_cursor_and_render_keep_native_utf16_after_astral_set_text() {
    let mut oracle = oracle::TsOracle::spawn("raw-tui-oracle.ts", "packages/tui/src");
    let expected = oracle.request(&json!({"operation":"editor_set","units":JsString::from("😀").units(),"width":80}));
    assert_eq!(expected["ok"], true, "{expected}");
    let terminal = Rc::new(RefCell::new(ProcessTerminal::with_output(Box::new(std::io::sink()))));
    let tui = TuiRenderer::new(terminal);
    let mut editor = Editor::new(Rc::new(tui), EditorTheme::plain(), EditorOptions::default());
    editor.set_text("😀");
    let (line, col) = editor.get_cursor();
    let actual = serde_json::to_string(&(
        editor.get_text(),
        editor.get_expanded_text(),
        line,
        col,
        editor.render(80),
    ))
    .unwrap();
    assert_eq!(
        parse_lossless(actual).unwrap(),
        parse_lossless(expected["json"].as_str().unwrap()).unwrap()
    );
}

#[derive(serde::Serialize)]
struct Snapshot {
    text: JsString,
    expanded: JsString,
    cursor: (usize, usize),
    lines: Vec<JsString>,
    history: Vec<JsString>,
}

#[test]
fn editor_raw_histories_callbacks_paste_inside_pair_edits_and_narrow_scroll_match_source() {
    let mut oracle = oracle::TsOracle::spawn("raw-tui-oracle.ts", "packages/tui/src");
    for raw in [
        JsString::from_utf16(vec![0xd800]),
        JsString::from_utf16(vec![0xdc00]),
        JsString::from("😀"),
        JsString::from_utf16(vec![97, 0xd800, 0x301, 98, 0xdc00]),
    ] {
        let key = |value: JsString| json!({"key":value.units()});
        let set = |value: JsString| json!({"set":value.units()});
        let paste = JsString::from("\x1b[200~") + &raw + "\r\n\t" + &raw + "\x1b[106;5u" + &raw + "\x1b[201~";
        let large = JsString::from("\x1b[200~") + JsString::from_utf16(raw.units().repeat(1001)) + "\x1b[201~";
        let multi = JsString::join((0..10).map(|index| raw.clone() + "line" + index.to_string()), "\n");
        let histories = [
            vec![
                set(raw.clone() + "a\nb" + &raw),
                key("\x1b[D".into()),
                key("x".into()),
                key(raw.clone()),
                key("\x7f".into()),
                key("\x1b[3~".into()),
                key("\x1f".into()),
                key("\x0b".into()),
                key("\x19".into()),
                key("\r".into()),
            ],
            vec![
                set("a😀b".into()),
                key("\x01".into()),
                key("\x1d".into()),
                key(JsString::from_utf16(vec![0xde00])),
                key("x".into()),
                key("\x7f".into()),
                key("\x1f".into()),
                key("\x04".into()),
                json!({"insert":raw.units()}),
                key("\r".into()),
            ],
            vec![set("a".into()), key(paste), key("\x1f".into()), key("\r".into())],
            vec![
                json!({"history":[(raw.clone()+"old").units(),(JsString::from("İ ")+&raw+"old").units()]}),
                set("draft".into()),
                key("\x12".into()),
                key("i\u{307}".into()),
                key("\r".into()),
                key("\r".into()),
            ],
            vec![
                set(multi),
                key("\x01".into()),
                key("\x1b[A".into()),
                key("\x1b[5~".into()),
            ],
            vec![key(large), key("\r".into())],
        ];
        for width in [1, 2, 4, 9] {
            for (history_index, actions) in histories.iter().enumerate() {
                let expected = oracle.request(&json!({"operation":"editor_actions","actions":actions,"width":width}));
                assert_eq!(expected["ok"], true, "{expected}");
                let terminal = Rc::new(RefCell::new(ProcessTerminal::with_output(Box::new(std::io::sink()))));
                let mut editor = Editor::new(
                    Rc::new(TuiRenderer::new(terminal)),
                    EditorTheme::plain(),
                    EditorOptions::default(),
                );
                let changes = Rc::new(RefCell::new(Vec::new()));
                let captured = changes.clone();
                editor.on_change = Some(Rc::new(move |value| captured.borrow_mut().push(value.clone())));
                let submissions = Rc::new(RefCell::new(Vec::new()));
                let captured = submissions.clone();
                editor.on_submit = Some(Rc::new(move |value| captured.borrow_mut().push(value.clone())));
                let expected = parse_lossless(expected["json"].as_str().unwrap()).unwrap();
                let expected_states = expected.get("states").unwrap().as_array().unwrap();
                for (index, action) in actions.iter().enumerate() {
                    if let Some(value) = action.get("set") {
                        editor.set_text(native(value));
                    }
                    if let Some(value) = action.get("key") {
                        editor.handle_input(&native(value));
                    }
                    if let Some(value) = action.get("insert") {
                        editor.insert_text_at_cursor(native(value));
                    }
                    if let Some(values) = action.get("history") {
                        editor.set_history(values.as_array().unwrap().iter().map(native).collect());
                    }
                    let lines = editor.render(width);
                    let actual = Snapshot {
                        text: editor.get_text(),
                        expanded: editor.get_expanded_text(),
                        cursor: editor.get_cursor(),
                        lines,
                        history: editor.get_history(),
                    };
                    let actual = parse_lossless(serde_json::to_string(&actual).unwrap()).unwrap();
                    assert_eq!(
                        &actual, &expected_states[index],
                        "raw={raw:?} width={width} history={history_index} step={index}"
                    );
                }
                let actual = parse_lossless(serde_json::to_string(&changes.borrow().clone()).unwrap()).unwrap();
                assert_eq!(
                    &actual,
                    expected.get("changes").unwrap(),
                    "changes raw={raw:?} width={width} history={history_index}"
                );
                let actual = parse_lossless(serde_json::to_string(&submissions.borrow().clone()).unwrap()).unwrap();
                assert_eq!(
                    &actual,
                    expected.get("submissions").unwrap(),
                    "submit raw={raw:?} width={width} history={history_index}"
                );
            }
        }
    }
}

fn native(units: &serde_json::Value) -> JsString {
    JsString::from_utf16(
        units
            .as_array()
            .unwrap()
            .iter()
            .map(|unit| unit.as_u64().unwrap() as u16)
            .collect(),
    )
}
