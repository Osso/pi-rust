use crate::shared_oracle_1 as oracle;
use pi_js_compat::{js_json::JsString, js_regex::JsRegex};
use serde_json::{Value, json};

struct RegexCase {
    pattern: String,
    flags: String,
    approved_limit_gap: bool,
}

fn cases() -> Vec<RegexCase> {
    let mut cases: Vec<_> = [
        ("(", "i"),
        (" )", "i"),
        ("(()))(", "i"),
        ("[", "i"),
        ("[\\", "i"),
        ("\\", "i"),
        ("[z-a]", "i"),
        ("*a", "i"),
        ("+", "i"),
        ("?", "i"),
        ("{1}", "i"),
        ("a{2,1}", "i"),
        ("a{", "u"),
        ("a{2,1}", "u"),
        ("{", "u"),
        ("}", "u"),
        ("]", "u"),
        ("a**", "i"),
        ("^*", "i"),
        ("(?<=a)+", "i"),
        ("(?=a)+", "u"),
        ("(?x:a)", "i"),
        ("(?ii:a)", "i"),
        ("(?i--m:a)", "i"),
        ("(?-:a)", "i"),
        ("(?i", "i"),
        ("(?", "i"),
        ("(?<1>a)", "i"),
        ("(?<a>a)(?<a>b)", "i"),
        ("(?<a>a)\\k<missing>", "i"),
        ("(?<a>a)\\k", "i"),
        ("\\k<x>", "u"),
        ("\\u{GG}", "u"),
        ("\\xG1", "u"),
        ("\\8", "u"),
        ("\\1", "u"),
        ("\\q", "u"),
        ("[\\c]", "u"),
        ("[\\d-a]", "u"),
        ("\\pX", "u"),
        ("\\p{}", "u"),
        ("\\p{Invalid}", "u"),
        ("\\p{Invalid=Other}", "u"),
        ("[\\p{RGI_Emoji}]", "v"),
        ("[", "v"),
        ("[a", "v"),
        ("[a&&]", "v"),
        ("[a--]", "v"),
        ("[a&&b-c]", "v"),
        ("[a--b&&c]", "v"),
        ("[a&&bc]", "v"),
        ("[a--bc]", "v"),
        ("[z-a]", "v"),
        ("[\\d-a]", "v"),
        ("[\\", "v"),
        ("[\\q]", "v"),
        ("[\\q{ab", "v"),
        ("[() ]", "v"),
        ("[!!]", "v"),
        ("[a-", "v"),
        ("[^\\p{RGI_Emoji}]", "v"),
        ("[\\P{RGI_Emoji}]", "v"),
        ("[\\q{ab}--]", "v"),
        ("a", "z"),
        ("a", "ii"),
        ("a", "uv"),
        ("a", "gg"),
        ("a", "dgi"),
        ("a/b\n[", "i"),
    ]
    .into_iter()
    .map(|(pattern, flags)| RegexCase {
        pattern: pattern.to_owned(),
        flags: flags.to_owned(),
        approved_limit_gap: false,
    })
    .collect();
    cases.push(RegexCase {
        pattern: "(".repeat(257) + &")".repeat(257),
        flags: "i".into(),
        approved_limit_gap: true,
    });
    cases.push(RegexCase {
        pattern: "()".repeat(65536),
        flags: "i".into(),
        approved_limit_gap: false,
    });
    cases.push(RegexCase {
        pattern: "a*".repeat(65536),
        flags: "i".into(),
        approved_limit_gap: true,
    });
    cases
}

#[test]
fn regex_error_prefix_retains_unmatched_pattern_units() {
    let mut oracle = oracle::TsOracle::spawn("regexp-diagnostics-oracle.mjs", "packages/tui/src");
    for raw in [vec![0xd800], vec![0xdc00], vec![0xd83d, 0xde00]] {
        let pattern = JsString::from("[") + JsString::from_utf16(raw);
        let expected = oracle.request(&json!({"id":2,"cases":[{"units":pattern.units(),"flags":"i"}]}));
        assert_eq!(expected["ok"], true);
        assert_eq!(expected["results"][0]["accepted"], false);
        let error = JsRegex::with_flags(&pattern, "i").unwrap_err();
        let prefix = JsString::from("Invalid regular expression: /") + pattern + "/i: ";
        assert!(error.message().starts_with(&prefix));
        let expected_units: Vec<u16> = serde_json::from_value(expected["results"][0]["units"].clone()).unwrap();
        assert!(expected_units.starts_with(prefix.units()));
    }
}

#[test]
fn regexp_classification_and_error_prefix_match_pinned_v8_with_approved_gaps() {
    let cases = cases();
    let mut oracle = oracle::TsOracle::spawn("regexp-diagnostics-oracle.mjs", "packages/tui/src");
    let expected=oracle.request(&json!({"id":1,"cases":cases.iter().map(|case|json!({"units":case.pattern.encode_utf16().collect::<Vec<_>>(),"flags":case.flags})).collect::<Vec<_>>()}));
    assert_eq!(expected["ok"], true, "{expected}");
    let mut corpus = Vec::new();
    let mut mismatches = 0;
    for (case, node) in cases.iter().zip(expected["results"].as_array().unwrap()) {
        let pattern = &case.pattern;
        let flags = &case.flags;
        let (native, category) = match JsRegex::with_flags(pattern.as_str(), flags) {
            Ok(_) => (json!({"accepted":true}), Value::Null),
            Err(error) => {
                let text = error.to_string();
                (
                    json!({"accepted":false,"name":"SyntaxError","units":text.encode_utf16().collect::<Vec<_>>()}),
                    json!(text),
                )
            }
        };
        // AGENTS.md, approved 2026-10-22: reason wording may differ, not classification.
        // The two explicitly recorded depth/loop cases remain visible acceptance gaps.
        let matches = if case.approved_limit_gap {
            node["accepted"] == true && native["accepted"] == false
        } else {
            let prefix = if node["accepted"] == true {
                None
            } else if flags == "z" || flags == "ii" || flags == "uv" || flags == "gg" {
                Some("Invalid flags supplied to RegExp constructor '".to_owned())
            } else {
                Some(format!("Invalid regular expression: /{pattern}/{flags}: "))
            };
            let prefix_matches =
                prefix.is_none_or(|prefix| category.as_str().is_some_and(|message| message.starts_with(&prefix)));
            native["accepted"] == node["accepted"] && prefix_matches
        };
        if !matches {
            mismatches += 1;
        }
        let node_message = node["units"]
            .as_array()
            .map(|units| JsString::from_utf16(units.iter().map(|unit| unit.as_u64().unwrap() as u16).collect()));
        let node_scalar = node_message
            .as_ref()
            .and_then(|message| String::from_utf16(message.units()).ok());
        corpus.push(json!({"pattern":pattern,"flags":flags,"regressCategory":category,"native":native,"node":node,"nodeMessage":node_scalar,"matches":matches,"approvedLimitGap":case.approved_limit_gap}));
    }
    let output = json!({"node":expected["node"],"v8":expected["v8"],"executable":expected["executable"],"cases":corpus,"mismatches":mismatches});
    println!("REGEXP_CORPUS {}", output);
    assert_eq!(
        mismatches, 0,
        "unapproved classification/prefix mismatches; see regexp-diagnostic-corpus.json"
    );
}
