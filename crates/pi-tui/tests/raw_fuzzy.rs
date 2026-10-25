use crate::shared_oracle as oracle;
use pi_js_compat::js_json::{JsString, parse_lossless};
use pi_tui::fuzzy::{fuzzy_filter, fuzzy_match};
use serde_json::json;

#[test]
fn raw_fuzzy_matching_lowercase_tokens_and_filter_preserve_native_identity() {
    let mut oracle = oracle::TsOracle::spawn("raw-tui-oracle.ts", "packages/tui/src");
    let candidates = vec![
        JsString::from_utf16(vec![0xd800, 97]),
        JsString::from_utf16(vec![0xdc00, 98]),
        JsString::from("😀/a"),
        JsString::from("Σ\u{301} x"),
        JsString::from("abc12"),
        JsString::from("12abc"),
    ];
    for query in [
        JsString::from_utf16(vec![0xd800]),
        JsString::from_utf16(vec![0xdc00]),
        JsString::from_utf16(vec![0xd83d]),
        JsString::from("😀/a"),
        JsString::from("abc12"),
        JsString::from("12abc"),
        JsString::from("σ x"),
    ] {
        let response = oracle.request(&json!({"operation":"fuzzy","query":query.units(),"items":candidates.iter().map(|value|value.units()).collect::<Vec<_>>()}));
        assert_eq!(response["ok"], true, "{response}");
        let scores: Vec<_> = candidates
            .iter()
            .map(|candidate| {
                let value = fuzzy_match(&query, candidate);
                (value.matches, value.score)
            })
            .collect();
        let filtered = fuzzy_filter(&candidates, &query, |value| vec![value.clone()]);
        let actual = serde_json::to_string(&(scores, filtered)).unwrap();
        assert_eq!(
            parse_lossless(actual).unwrap(),
            parse_lossless(response["json"].as_str().unwrap()).unwrap(),
            "query={query:?}"
        );
    }
}
