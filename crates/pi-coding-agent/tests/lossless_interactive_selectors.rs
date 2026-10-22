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
