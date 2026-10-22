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
