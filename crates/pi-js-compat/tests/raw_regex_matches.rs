use crate::shared_oracle_1 as oracle;

use pi_js_compat::js_json::{JsString, parse_lossless};
use pi_js_compat::js_regex::JsRegex;
use serde_json::json;

#[test]
fn native_capture_ranges_groups_and_replacements_match_node_regexp() {
    let mut oracle = oracle::TsOracle::spawn("raw-regexp-oracle.mjs", "packages/tui/src");
    let text = JsString::from_utf16(vec![0xd800, 0x61, 0xd83d, 0xde00, 0x62, 0xdc00]);
    for (pattern, flags) in [
        ("(.)", ""),
        ("(.)", "u"),
        ("(?<letter>a)(b)?", ""),
        ("(?:)", ""),
        ("(?:)", "u"),
        ("(?<=a)(.)", ""),
        ("(a)|(b)", ""),
    ] {
        let regex = JsRegex::with_flags(pattern, flags).unwrap();
        for start in [0, 1, 2, 3, 4, 6, 7] {
            let replacement = JsString::from_utf16(vec![0xdc00]) + "$$:$&:$`:$':$1:$2:$01:$99:$<letter>";
            let expected = oracle.request(&json!({"pattern":pattern,"flags":flags,"units":text.units(),"start":start,"replacement":replacement.units()}));
            assert_eq!(expected["ok"], true, "{expected}");
            let captures: Vec<_> = regex
                .captures_from(&text, start)
                .into_iter()
                .map(|capture| {
                    let groups: Vec<_> = (0..capture.len())
                        .map(|index| {
                            capture
                                .get(index)
                                .map(|group| (group.start(), group.end(), group.text()))
                        })
                        .collect();
                    groups
                })
                .collect();
            let replacement = regex.replace_all(&text, &replacement);
            let callback = regex.replace_all_with(&text, |capture| {
                JsString::from_utf16(vec![0xd800]) + capture.get(0).unwrap().text()
            });
            let actual = serde_json::to_string(&(captures, replacement, callback)).unwrap();
            assert_eq!(
                parse_lossless(actual).unwrap(),
                parse_lossless(expected["json"].as_str().unwrap()).unwrap(),
                "pattern={pattern} flags={flags} start={start}"
            );
        }
    }
}
