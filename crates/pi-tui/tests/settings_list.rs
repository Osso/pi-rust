use pi_js_compat::js_json::JsString;
use std::cell::RefCell;
use std::rc::Rc;

use pi_tui::components::settings_list::{SettingItem, SettingsList, SettingsListOptions, SettingsListTheme};
use pi_tui::tui::Component;

fn items() -> Vec<SettingItem> {
    vec![SettingItem {
        id: "tui-mode".into(),
        label: "TUI mode".into(),
        description: None,
        current_value: "regular".into(),
        values: Some(vec!["regular".into(), "fullscreen".into()]),
        submenu: None,
    }]
}

fn list(changes: Rc<RefCell<Vec<(JsString, JsString)>>>, enable_search: bool) -> SettingsList {
    SettingsList::new(
        items(),
        10,
        SettingsListTheme::plain("> "),
        Rc::new(move |id: &JsString, value: &JsString| {
            changes.borrow_mut().push((id.into(), value.into()));
        }),
        Rc::new(|| {}),
        SettingsListOptions { enable_search },
    )
}

#[test]
fn space_activates_during_search_and_multi_character_input_strips_spaces() {
    let changes = Rc::new(RefCell::new(Vec::new()));
    let mut list = list(Rc::clone(&changes), true);
    list.handle_input(&pi_js_compat::js_json::JsString::from("TUI"));
    list.handle_input(&pi_js_compat::js_json::JsString::from(" "));
    assert_eq!(*changes.borrow(), vec![("tui-mode".into(), "fullscreen".into())]);
    list.handle_input(&pi_js_compat::js_json::JsString::from(" mo de"));
    assert!(list.render(80)[0].contains("TUImode"));
    list.handle_input(&pi_js_compat::js_json::JsString::from("\r"));
    assert_eq!(
        *changes.borrow(),
        vec![
            ("tui-mode".into(), "fullscreen".into()),
            ("tui-mode".into(), "regular".into())
        ]
    );
}

#[test]
fn keeps_space_as_a_change_shortcut_before_a_search_query_is_entered() {
    let changes = Rc::new(RefCell::new(Vec::new()));
    let mut list = list(Rc::clone(&changes), true);

    list.handle_input(&pi_js_compat::js_json::JsString::from(" "));

    assert_eq!(*changes.borrow(), vec![("tui-mode".into(), "fullscreen".into())]);
}
