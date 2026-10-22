use std::{cell::Cell, rc::Rc};

use pi_agent_core::ThinkingLevel;
use pi_coding_agent::modes::interactive::components::{
    show_images_selector::ShowImagesSelectorComponent, thinking_selector::ThinkingSelectorComponent,
};
use pi_js_compat::js_json::JsString;
use pi_tui::Component;

fn init_theme() {
    pi_coding_agent::modes::interactive::theme::init_theme(Some(&"dark".into()), false).unwrap();
}

#[test]
fn thinking_selector_retains_selection_after_unmatched_input() {
    init_theme();
    let selected = Rc::new(Cell::new(None));
    let callback = selected.clone();
    let mut selector = ThinkingSelectorComponent::new(
        ThinkingLevel::High,
        &[ThinkingLevel::Low, ThinkingLevel::High],
        Rc::new(move |level| callback.set(Some(level))),
        Rc::new(|| {}),
    );
    let list = selector.get_select_list();
    list.borrow_mut().handle_input(&JsString::from_utf16(vec![0xd800]));
    assert!(JsString::join(selector.render(80), "\n").contains("Deep reasoning"));
    list.borrow_mut().handle_input(&"\r".into());
    assert_eq!(selected.get(), Some(ThinkingLevel::High));
}

#[test]
fn image_selector_retains_selection_after_unmatched_input() {
    init_theme();
    let selected = Rc::new(Cell::new(None));
    let callback = selected.clone();
    let mut selector =
        ShowImagesSelectorComponent::new(false, Rc::new(move |show| callback.set(Some(show))), Rc::new(|| {}));
    let list = selector.get_select_list();
    list.borrow_mut().handle_input(&JsString::from_utf16(vec![0xdc00]));
    assert!(JsString::join(selector.render(80), "\n").contains("Show text placeholder instead"));
    list.borrow_mut().handle_input(&"\r".into());
    assert_eq!(selected.get(), Some(false));
}

#[test]
fn user_message_keeps_raw_units_across_padding_and_invalidation() {
    use pi_coding_agent::modes::interactive::components::user_message::UserMessageComponent;
    init_theme();
    let text = JsString::from("raw ") + JsString::from_utf16(vec![0xd800, 0x20, 0xdc00]);
    let mut message = UserMessageComponent::new(text.clone(), None, 1, vec![]);
    for padding in [1, 2] {
        message.set_output_pad(padding);
        message.invalidate();
        let output = JsString::join(message.render(80), "\n");
        assert!(output.contains(&text));
        assert!(output.contains("\u{1b}]133;A\u{7}"));
        assert!(output.contains("\u{1b}]133;B\u{7}\u{1b}]133;C\u{7}"));
    }
}
