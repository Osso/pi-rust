#[path = "support/oracle.rs"]
mod oracle;
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
