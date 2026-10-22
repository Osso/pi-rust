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
