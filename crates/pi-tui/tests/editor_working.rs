//! Ported from `packages/tui/test/editor.test.ts` (`Working prompt animation`).

use crate::shared_editor_host as editor_host;

use std::rc::Rc;
use std::time::Duration;

use editor_host::{fake_host, plain_theme};
use pi_js_compat::js_json::JsString;
use pi_tui::components::editor::{Editor, EditorOptions, EditorTheme};
use pi_tui::components::loader::LoaderIndicatorOptions;
use pi_tui::tui::Component;

fn working_editor() -> (Editor, std::rc::Rc<std::cell::RefCell<editor_host::FakeHostState>>) {
    let (host, state) = fake_host(24);
    let mut theme: EditorTheme = plain_theme();
    theme.prompt_prefix = Some(Rc::new(|text| text.clone()));
    (Editor::new(host, theme, EditorOptions::default()), state)
}

#[test]
fn animates_the_prompt_through_fixed_cell_writes_without_global_renders() {
    let (mut editor, state) = working_editor();
    editor.render(40);
    editor.set_screen_origin(10, 5);
    editor.set_working(true);
    assert_eq!(editor.get_prompt_position(), Some((1, 0)));

    let writes = state.borrow().writes.clone();
    assert!(!writes.is_empty(), "working state should render a prompt frame");
    assert_eq!((writes[0].row, writes[0].col), (11, 5));
    assert_ne!(writes[0].value, "›");

    let renders_before = state.borrow().render_count;
    let writes_before = writes.len();
    let deadline = editor.working_next_deadline().unwrap();
    editor.poll_working(deadline + Duration::from_millis(250));

    let state = state.borrow();
    assert!(state.writes.len() > writes_before);
    assert_ne!(
        state.writes.last().unwrap().value,
        state.writes[writes_before - 1].value
    );
    assert_eq!(state.render_count, renders_before);
}

#[test]
fn supports_static_hidden_and_custom_indicators_and_restores_the_idle_prompt() {
    let (mut editor, state) = working_editor();
    editor.render(40);
    editor.set_screen_origin(0, 0);
    editor
        .set_working_indicator(Some(LoaderIndicatorOptions {
            frames: Some(vec![JsString::from("●")]),
            interval_ms: None,
        }))
        .unwrap();
    editor.set_working(true);
    assert_eq!(state.borrow().writes.last().unwrap().value, "●");

    let static_writes = state.borrow().writes.len();
    editor.poll_working(std::time::Instant::now() + Duration::from_secs(1));
    assert_eq!(state.borrow().writes.len(), static_writes);

    editor
        .set_working_indicator(Some(LoaderIndicatorOptions {
            frames: Some(vec![JsString::from("a"), JsString::from("b")]),
            interval_ms: Some(100),
        }))
        .unwrap();
    let custom_writes = state.borrow().writes.len();
    let deadline = editor.working_next_deadline().unwrap();
    editor.poll_working(deadline);
    assert!(state.borrow().writes.len() > custom_writes);
    assert_eq!(state.borrow().writes.last().unwrap().value, "b");

    editor
        .set_working_indicator(Some(LoaderIndicatorOptions {
            frames: Some(Vec::new()),
            interval_ms: None,
        }))
        .unwrap();
    assert_eq!(state.borrow().writes.last().unwrap().value, " ");
    editor.set_working(false);
    assert_eq!(state.borrow().writes.last().unwrap().value, "›");
    assert_eq!(state.borrow().render_count, 0);
}

#[test]
fn rejects_working_frames_wider_than_one_terminal_cell() {
    let (mut editor, _) = working_editor();
    let error = editor
        .set_working_indicator(Some(LoaderIndicatorOptions {
            frames: Some(vec![JsString::from("XX")]),
            interval_ms: None,
        }))
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "Editor working indicator frames must occupy exactly one terminal cell"
    );
}
