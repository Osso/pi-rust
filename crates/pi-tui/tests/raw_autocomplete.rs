#[path = "support/oracle.rs"]
mod oracle;
use pi_js_compat::js_json::{JsString, parse_lossless};
use pi_tui::autocomplete::{AutocompleteItem, CombinedAutocompleteProvider};
use serde_json::json;

#[test]
fn completion_applies_at_a_native_cursor_inside_a_surrogate_pair() {
    let mut oracle = oracle::TsOracle::spawn("raw-tui-oracle.ts", "packages/tui/src");
    let line = "😀tail";
    let expected=oracle.request(&json!({"operation":"autocomplete_apply","units":JsString::from(line).units(),"cursor":1,"item":JsString::from("X").units(),"prefix":[]}));
    assert_eq!(expected["ok"], true, "{expected}");
    let provider = CombinedAutocompleteProvider::new(vec![], ".", None);
    let result = provider.apply_completion(
        &[JsString::from(line)],
        0,
        1,
        &AutocompleteItem::new("X", "X"),
        &JsString::default(),
    );
    let actual = serde_json::to_string(&(result.lines, result.cursor_line, result.cursor_col)).unwrap();
    assert_eq!(
        parse_lossless(actual).unwrap(),
        parse_lossless(expected["json"].as_str().unwrap()).unwrap()
    );
}
