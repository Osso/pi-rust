use crate::shared_oracle_2 as oracle;
use pi_js_compat::js_json::JsString;
use pi_js_compat::string::{replace, replace_global};

#[test]
fn matches_node_replace_and_replace_all_on_raw_units() {
    let texts = [
        JsString::from_utf16(vec![0xd800, 88, 0xdc00]),
        JsString::from("😀😀"),
        JsString::from("abc"),
        JsString::from_utf16(vec![0xd800, 0xd800]),
    ];
    let searches = [
        JsString::from("X"),
        JsString::from_utf16(vec![0xd800]),
        JsString::from_utf16(vec![0xd83d]),
        JsString::from(""),
        JsString::from("z"),
    ];
    let replacements = [
        JsString::from("$`|$&|$'|$$|$1"),
        JsString::from_utf16(vec![0xdc01]),
        JsString::from(""),
        JsString::from("$"),
        JsString::from("😀"),
    ];
    let mut cases = Vec::new();
    let mut actual = Vec::new();
    for input in &texts {
        for search in &searches {
            for replacement in &replacements {
                for global in [false, true] {
                    if global && search.is_empty() {
                        continue;
                    }
                    cases.push(serde_json::json!({"input":input.units(),"search":search.units(),"replacement":replacement.units(),"global":global}));
                    let output = if global {
                        replace_global(input, search, replacement)
                    } else {
                        replace(input, search, replacement)
                    };
                    actual.push(output.units().to_vec());
                }
            }
        }
    }
    let mut oracle = oracle::TsOracle::spawn("js-compat-oracle.ts", "packages/coding-agent/src/index.ts");
    let expected = oracle.request(&serde_json::json!({"id":"raw-replace","operation":"replace_units","cases":cases}));
    assert_eq!(expected["ok"], true);
    assert_eq!(serde_json::json!(actual), expected["output"]["results"]);
}

#[test]
fn first_replacement_can_match_one_half_of_a_surrogate_pair() {
    let input = JsString::from("😀😀");
    let high = JsString::from_utf16(vec![0xd83d]);
    let low = JsString::from_utf16(vec![0xdfff]);
    assert_eq!(replace(&input, &high, &low).units(), &[0xdfff, 0xde00, 0xd83d, 0xde00]);
    assert_eq!(input.units(), &[0xd83d, 0xde00, 0xd83d, 0xde00]);
}

#[test]
fn replacement_expansion_keeps_raw_original_prefix_and_suffix() {
    let high = JsString::from_utf16(vec![0xd800]);
    let low = JsString::from_utf16(vec![0xdc00]);
    let input = high.clone() + "X" + &low;
    let expected = high.clone() + &high + "|X|" + &low + "|$|$1" + low;
    assert_eq!(replace(&input, "X", "$`|$&|$'|$$|$1"), expected);
}

#[test]
fn global_replacement_preserves_raw_matches_and_replacement_units() {
    let high = JsString::from_utf16(vec![0xd800]);
    let input = high.clone() + "a" + &high;
    let replacement = JsString::from("$&") + JsString::from_utf16(vec![0xdc01]);
    let expected = high.clone() + JsString::from_utf16(vec![0xdc01]) + "a" + high + JsString::from_utf16(vec![0xdc01]);
    assert_eq!(
        replace_global(&input, JsString::from_utf16(vec![0xd800]), replacement),
        expected
    );
}
