use std::cell::RefCell;
use std::rc::Rc;

use pi_tui::components::settings_list::{SettingItem, SettingsList, SettingsListOptions, SettingsListTheme};
use pi_tui::tui::Component;

fn items() -> Vec<SettingItem> {
    vec![SettingItem {
        id: "tui-mode".to_string(),
        label: "TUI mode".to_string(),
        description: None,
        current_value: "regular".to_string(),
        values: Some(vec!["regular".to_string(), "fullscreen".to_string()]),
        submenu: None,
    }]
}

fn list(changes: Rc<RefCell<Vec<(String, String)>>>, enable_search: bool) -> SettingsList {
    SettingsList::new(
        items(),
        10,
        SettingsListTheme::plain("> "),
        Rc::new(move |id: &str, value: &str| {
            changes.borrow_mut().push((id.to_string(), value.to_string()));
        }),
        Rc::new(|| {}),
        SettingsListOptions { enable_search },
    )
}

#[test]
fn space_activates_during_search_and_multi_character_input_strips_spaces() {
    let changes = Rc::new(RefCell::new(Vec::new()));
    let mut list = list(Rc::clone(&changes), true);
    list.handle_input("TUI");
    list.handle_input(" ");
    assert_eq!(
        *changes.borrow(),
        vec![("tui-mode".to_string(), "fullscreen".to_string())]
    );
    list.handle_input(" mo de");
    assert!(list.render(80)[0].contains("TUImode"));
    list.handle_input("\r");
    assert_eq!(
        *changes.borrow(),
        vec![
            ("tui-mode".to_string(), "fullscreen".to_string()),
            ("tui-mode".to_string(), "regular".to_string())
        ]
    );
}

#[test]
fn keeps_space_as_a_change_shortcut_before_a_search_query_is_entered() {
    let changes = Rc::new(RefCell::new(Vec::new()));
    let mut list = list(Rc::clone(&changes), true);

    list.handle_input(" ");

    assert_eq!(
        *changes.borrow(),
        vec![("tui-mode".to_string(), "fullscreen".to_string())]
    );
}
