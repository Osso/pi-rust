use std::sync::Arc;

use pi_coding_agent::cli::session_picker::{SelectSessionOutcome, ValidateSelection};
use pi_js_compat::js_json::JsString;

#[test]
fn picker_selection_and_validation_retain_unpaired_path_units() {
    let path = JsString::from_utf16(vec![0x2f, 0xd800, 0x2f, 0xdc00]);
    let expected = path.clone();
    let validate: ValidateSelection = Arc::new(move |selected| {
        assert_eq!(selected.units(), expected.units());
        Err(selected.clone())
    });
    assert_eq!(validate(&path), Err(path.clone()));
    assert_eq!(
        SelectSessionOutcome::Selected(path.clone()),
        SelectSessionOutcome::Selected(path)
    );
}

#[test]
fn search_tokens_and_regex_keep_unpaired_units() {
    use pi_coding_agent::modes::interactive::components::session_selector_search::parse_search_query;
    let text = JsString::from_utf16(vec![0x22, 0xd800, 0x22, 0x20, 0xdc00]);
    let parsed = parse_search_query(&text);
    assert_eq!(parsed.tokens[0].value.units(), &[0xd800]);
    assert_eq!(parsed.tokens[1].value.units(), &[0xdc00]);
    let regex = parse_search_query(&(JsString::from("re:") + JsString::from_utf16(vec![0xd800])));
    assert!(regex.regex.unwrap().is_match(JsString::from_utf16(vec![0xd800])));
}

#[test]
fn session_selector_renders_raw_name_and_returns_original_path() {
    use pi_coding_agent::core::{keybindings::KeybindingsManager, session_manager::SessionInfo};
    use pi_coding_agent::modes::interactive::components::{
        session_selector::SessionList,
        session_selector_search::{NameFilter, SortMode},
    };
    use pi_tui::Component;
    let name = JsString::from_utf16(vec![0x6e, 0xd800, 0x6d, 0xdc00]);
    let path = JsString::from_utf16(vec![0x2f, 0xd801, 0x2e, 0x6a]);
    let info = SessionInfo {
        path: path.clone(),
        id: "raw-session".into(),
        cwd: "/work".into(),
        name: Some(name.clone()),
        is_archived: Some(false),
        parent_session_path: None,
        created: 0,
        modified: 0,
        message_count: 1,
        first_message: "body".into(),
    };
    let mut list = SessionList::new(
        vec![info],
        false,
        SortMode::Recent,
        NameFilter::All,
        KeybindingsManager::new(Default::default(), None),
        None,
    );
    assert_eq!(list.get_selected_session_path(), Some(&path));
    assert!(list.render(120).iter().any(|line| line.contains(&name)));
}

#[test]
fn tree_label_editor_keeps_units_through_input_and_render() {
    use pi_coding_agent::modes::interactive::components::tree_selector::LabelInput;
    use pi_tui::Component;
    let label = JsString::from_utf16(vec![0xd800, 0x61, 0xdc00]);
    let mut input = LabelInput::new(&"entry".into(), Some(&label));
    assert_eq!(input.input().get_value(), &label);
    assert!(input.render(80).iter().any(|line| line.contains(&label)));
}
