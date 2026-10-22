use pi_js_compat::js_json::JsString;
use pi_tui::keybindings::{KeybindingConflict, KeybindingDefinition, KeybindingsManager, tui_keybindings};

fn manager(config: Vec<(&str, Vec<&str>)>) -> KeybindingsManager {
    let user = config
        .into_iter()
        .map(|(id, keys)| (JsString::from(id), keys.into_iter().map(JsString::from).collect()))
        .collect();
    KeybindingsManager::new(tui_keybindings(), user)
}

#[test]
fn binds_ctrl_j_as_default_newline_alias() {
    let keybindings = manager(Vec::new());

    assert_eq!(keybindings.get_keys("tui.input.newLine"), vec!["shift+enter", "ctrl+j"]);
    assert!(keybindings.matches("\n", "tui.input.newLine"));
    assert!(keybindings.matches("\x1b[106;5u", "tui.input.newLine"));
}

#[test]
fn binds_ctrl_r_as_reverse_history_search() {
    let keybindings = manager(Vec::new());

    assert_eq!(keybindings.get_keys("tui.editor.historySearch"), vec!["ctrl+r"]);
    assert!(keybindings.matches("\x12", "tui.editor.historySearch"));
}

#[test]
fn binds_frozen_editor_viewport_navigation() {
    let keybindings = manager(Vec::new());

    assert_eq!(
        keybindings.get_keys("tui.editor.cursorLineStart"),
        vec!["home", "ctrl+a"]
    );
    assert_eq!(keybindings.get_keys("tui.editor.cursorLineEnd"), vec!["end", "ctrl+e"]);
    assert_eq!(keybindings.get_keys("tui.editor.pageUp"), vec!["pageUp"]);
    assert_eq!(keybindings.get_keys("tui.editor.pageDown"), vec!["pageDown"]);
}

#[test]
fn leaves_dedicated_prompt_history_navigation_unbound_by_default() {
    let keybindings = manager(Vec::new());

    assert!(keybindings.get_keys("tui.editor.historyPrevious").is_empty());
    assert!(keybindings.get_keys("tui.editor.historyNext").is_empty());
}

#[test]
fn frozen_defaults_exclude_later_alternate_screen_bindings() {
    let keybindings = manager(Vec::new());
    assert!(keybindings.get_definition("tui.altScreen.pageUp").is_none());
    assert!(keybindings.get_definition("tui.editor.historyPrevious").is_none());
    assert!(!keybindings.matches("\x1b[5~", "tui.altScreen.pageUp"));
}

#[test]
fn does_not_evict_selector_confirm_when_input_submit_is_rebound() {
    let keybindings = manager(vec![("tui.input.submit", vec!["enter", "ctrl+enter"])]);

    assert_eq!(keybindings.get_keys("tui.input.submit"), vec!["enter", "ctrl+enter"]);
    assert_eq!(keybindings.get_keys("tui.select.confirm"), vec!["enter"]);
}

#[test]
fn does_not_evict_cursor_bindings_when_another_action_reuses_the_same_key() {
    let keybindings = manager(vec![("tui.select.up", vec!["up", "ctrl+p"])]);

    assert_eq!(keybindings.get_keys("tui.select.up"), vec!["up", "ctrl+p"]);
    assert_eq!(keybindings.get_keys("tui.editor.cursorUp"), vec!["up"]);
}

#[test]
fn reports_direct_user_binding_conflicts_without_evicting_defaults() {
    let keybindings = manager(vec![
        ("tui.input.submit", vec!["ctrl+x"]),
        ("tui.select.confirm", vec!["ctrl+x"]),
    ]);

    assert_eq!(
        keybindings.get_conflicts(),
        vec![KeybindingConflict {
            key: "ctrl+x".into(),
            keybindings: vec!["tui.input.submit".into(), "tui.select.confirm".into()],
        }]
    );
    assert_eq!(keybindings.get_keys("tui.editor.cursorLeft"), vec!["left", "ctrl+b"]);
}

#[test]
fn user_bindings_replace_defaults_and_deduplicate_keys() {
    let keybindings = manager(vec![("tui.select.up", vec!["ctrl+p", "ctrl+p", "up"])]);

    assert_eq!(keybindings.get_keys("tui.select.up"), vec!["ctrl+p", "up"]);
    assert!(keybindings.get_conflicts().is_empty());
}

#[test]
fn ignores_user_bindings_for_unknown_ids_and_resolves_definitions() {
    let keybindings = manager(vec![("not.a.binding", vec!["ctrl+x"])]);

    assert_eq!(
        keybindings.get_definition("tui.editor.cursorUp"),
        Some(&KeybindingDefinition {
            default_keys: vec!["up".into()],
            description: Some("Move cursor up".into()),
        })
    );
    assert_eq!(keybindings.get_definition("not.a.binding"), None);
    assert!(keybindings.get_conflicts().is_empty());
    assert_eq!(
        keybindings.get_user_bindings(),
        vec![("not.a.binding".into(), vec!["ctrl+x".into()])]
    );
}

#[test]
fn resolved_bindings_include_every_definition_in_order() {
    let keybindings = manager(Vec::new());
    let resolved = keybindings.get_resolved_bindings();
    let ids: Vec<JsString> = resolved.iter().map(|(id, _)| id.clone()).collect();

    assert_eq!(ids.first().copied(), Some("tui.editor.cursorUp"));
    assert_eq!(ids.last().copied(), Some("tui.select.cancel"));
    assert_eq!(
        resolved
            .iter()
            .find(|(id, _)| id == "tui.editor.cursorUp")
            .map(|(_, keys)| keys.clone()),
        Some(vec!["up".into()])
    );
    assert_eq!(
        resolved
            .iter()
            .find(|(id, _)| id == "tui.editor.cursorLeft")
            .map(|(_, keys)| keys.clone()),
        Some(vec!["left".into(), "ctrl+b".into()])
    );
}

#[test]
fn set_user_bindings_rebuilds_keys_and_conflicts() {
    let mut keybindings = manager(Vec::new());
    keybindings.set_user_bindings(vec![
        ("tui.select.up".into(), vec!["ctrl+p".into()]),
        ("tui.editor.cursorUp".into(), vec!["ctrl+p".into()]),
    ]);

    assert_eq!(keybindings.get_keys("tui.select.up"), vec!["ctrl+p"]);
    assert_eq!(
        keybindings.get_conflicts(),
        vec![KeybindingConflict {
            key: "ctrl+p".into(),
            keybindings: vec!["tui.select.up".into(), "tui.editor.cursorUp".into()],
        }]
    );
}
