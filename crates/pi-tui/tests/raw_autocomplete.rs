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

#[test]
fn raw_completion_slices_quotes_directories_and_negative_internal_prefix_offsets_match_source() {
    let mut oracle = oracle::TsOracle::spawn("raw-tui-oracle.ts", "packages/tui/src");
    let provider = CombinedAutocompleteProvider::new(vec![], ".", None);
    for raw in [
        JsString::from_utf16(vec![0xd800]),
        JsString::from_utf16(vec![0xdc00]),
        JsString::from("😀"),
        JsString::from_utf16(vec![97, 0xd800, 98, 0xdc00]),
    ] {
        let line = raw.clone() + "tail";
        for cursor in 0..=line.len() {
            for prefix in [JsString::default(), JsString::from("zz"), JsString::from("@")] {
                for directory in [false, true] {
                    let item = raw.clone() + if directory { "/\"" } else { "x" };
                    let label = raw.clone() + if directory { "/" } else { "x" };
                    let expected = oracle.request(&json!({"operation":"autocomplete_apply","units":line.units(),"cursor":cursor,"item":item.units(),"itemLabel":label.units(),"prefix":prefix.units()}));
                    assert_eq!(expected["ok"], true, "{expected}");
                    let result = provider.apply_completion(
                        &[line.clone()],
                        0,
                        cursor,
                        &AutocompleteItem::new(item, label),
                        &prefix,
                    );
                    let actual = serde_json::to_string(&(result.lines, result.cursor_line, result.cursor_col)).unwrap();
                    assert_eq!(
                        parse_lossless(actual).unwrap(),
                        parse_lossless(expected["json"].as_str().unwrap()).unwrap(),
                        "raw={raw:?} cursor={cursor} prefix={prefix:?} directory={directory}"
                    );
                }
            }
        }
    }
}
