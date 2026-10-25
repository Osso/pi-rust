//! UTF-16 matching contracts used by the bwrap worker.
use crate::shared_oracle_2 as oracle;

use pi_js_compat::minimatch::{MinimatchOptions, minimatch};
use serde_json::json;

#[test]
fn worker_extglob_question_counts_utf16_units() {
    let mut oracle = oracle::TsOracle::spawn("js-compat-oracle.ts", "node_modules/minimatch/dist/esm/index.js");
    let result = oracle.request(&json!({"id": 1, "operation": "minimatch", "cases": [
        {"path": "😀", "pattern": "@(?)", "options": {}}
    ]}));
    assert_eq!(result["output"]["results"][0], false);
    assert!(!minimatch("😀", "@(?)", &MinimatchOptions::default()));
}

use pi_js_compat::js_json::JsString;
use pi_js_compat::js_regex::JsRegex;

fn raw(units: &[u16]) -> JsString {
    JsString::from_utf16(units.to_vec())
}

#[test]
fn raw_minimatch_matches_pinned_ast_braces_extglobs_and_classes() {
    let mut oracle = oracle::TsOracle::spawn("js-compat-oracle.ts", "node_modules/minimatch/dist/esm/index.js");
    let atoms = [
        raw(&[0xd800]),
        raw(&[0xdc00]),
        raw(&[0xd801]),
        raw(&[0xd83d, 0xde00]),
        raw(&[0xfffd]),
        raw(&[0xe000]),
        JsString::from("A"),
        JsString::from("K"),
    ];
    let mut paths = vec![
        JsString::default(),
        JsString::from("."),
        JsString::from(".a"),
        JsString::from("a/b"),
    ];
    let mut patterns: Vec<JsString> = [
        "*",
        "?",
        "??",
        "@(?)",
        "@(??)",
        "*(?)",
        "+(?)",
        "!(?)",
        "**/*",
        "[!a]",
        "[[:alpha:]]",
        "[[:upper:]]",
    ]
    .into_iter()
    .map(JsString::from)
    .collect();
    for atom in &atoms {
        paths.extend([
            atom.clone(),
            JsString::from("a/") + atom,
            atom.clone() + ".x",
            JsString::from(".") + atom,
        ]);
        patterns.extend([
            atom.clone(),
            JsString::from("{") + atom + ",x}",
            JsString::from("@(") + atom + "|x)",
            JsString::from("!(") + atom + "|x)",
            JsString::from("[") + atom + "]",
            JsString::from("[!") + atom + "]",
            JsString::from("*") + atom + "*",
            JsString::from("**/") + atom,
        ]);
    }
    patterns.push(raw(&[91, 0xd800, 45, 0xdfff, 93]));
    for nocase in [false, true] {
        let options = MinimatchOptions {
            nocase,
            ..Default::default()
        };
        let mut cases = Vec::new();
        let mut actual = Vec::new();
        for pattern in &patterns {
            for path in &paths {
                cases.push(json!({"path":"", "pattern":"", "pathUnits":path.units(), "patternUnits":pattern.units(), "options":{"nocase":nocase}}));
                actual.push(minimatch(path, pattern, &options));
            }
        }
        let response = oracle.request(&json!({"id":2,"operation":"minimatch","cases":cases}));
        assert_eq!(response["ok"], true, "{response}");
        let expected = response["output"]["results"].as_array().unwrap();
        for (index, result) in actual.iter().enumerate() {
            assert_eq!(
                *result,
                expected[index].as_bool().unwrap(),
                "case {:?} nocase={nocase}",
                cases[index]
            );
        }
    }
}

#[test]
fn raw_regex_and_literal_match_actual_frozen_worker() {
    let mut oracle = oracle::TsOracle::spawn(
        "bwrap-worker-matching-oracle.cjs",
        "packages/coding-agent/extensions/bwrap/src/fs-worker.cjs",
    );
    let texts = [
        raw(&[0xd800]),
        raw(&[0xdc00]),
        raw(&[0xd83d, 0xde00]),
        raw(&[0xfffd]),
        raw(&[0xe000]),
        JsString::from("AKſ"),
        JsString::from("K"),
        JsString::from("ſ"),
        JsString::from("K"),
        JsString::from("S"),
        JsString::from("İΣ"),
    ];
    let patterns = [
        raw(&[0xd800]),
        raw(&[0xdc00]),
        raw(&[0xd83d, 0xde00]),
        raw(&[91, 0xd800, 45, 0xdfff, 93]),
        JsString::from("^.$"),
        JsString::from("^..$"),
        JsString::from("a|k|s"),
        JsString::from("(?<=A)K"),
        JsString::from("[^a]"),
        JsString::from("Σ"),
        JsString::from("\\uD800"),
        JsString::from("[\\uD800-\\uDFFF]"),
        JsString::from("k"),
        JsString::from("s"),
    ];
    let mut cases = Vec::new();
    let mut actual = Vec::new();
    for ignore_case in [false, true] {
        for literal in [false, true] {
            for pattern in &patterns {
                for text in &texts {
                    cases.push(json!({"kind":if literal {"literal"} else {"regex"},"pattern":pattern.units(),"text":text.units(),"ignoreCase":ignore_case}));
                    let matched = if literal {
                        if ignore_case {
                            text.to_lowercase().contains(pattern.to_lowercase())
                        } else {
                            text.contains(pattern)
                        }
                    } else {
                        JsRegex::with_flags(pattern, if ignore_case { "i" } else { "" })
                            .unwrap()
                            .is_match(text)
                    };
                    actual.push(matched);
                }
            }
        }
    }
    let response = oracle.request(&json!({"id":3,"cases":cases}));
    assert_eq!(response["ok"], true);
    for (index, result) in actual.iter().enumerate() {
        assert_eq!(
            *result,
            response["output"][index]["matched"].as_bool().unwrap(),
            "case {:?}",
            cases[index]
        );
    }
    // Behavioral mutants: replacement conflates distinct units; Unicode mode merges a pair.
    assert!(!JsRegex::new(raw(&[0xd800])).unwrap().is_match(raw(&[0xdc00])));
    assert!(JsRegex::new("^..$").unwrap().is_match("😀"));
    assert!(!JsRegex::new("^.$").unwrap().is_match("😀"));
    assert!(JsRegex::unicode("^.$").unwrap().is_match("😀"));
}

#[test]
fn legacy_and_unicode_canonicalization_matches_node_for_literals_classes_ranges_and_backrefs() {
    let mut oracle = oracle::TsOracle::spawn(
        "bwrap-worker-matching-oracle.cjs",
        "packages/coding-agent/extensions/bwrap/src/fs-worker.cjs",
    );
    let patterns = [
        "s", "[s]", "[a-z]", "[S-Z]", "(.)\\1", "(s)\\1", "k", "[k]", "(k)\\1", "i", "[i]", "([i])\\1", "\\w", "\\W",
        "[\\w]", "[^s]",
    ];
    let mut texts: Vec<JsString> = ["ſ", "sſ", "Sſ", "K", "kK", "KK", "ı", "iı", "Iı", "s", "k", "i", "µΜ"]
        .into_iter()
        .map(JsString::from)
        .collect();
    texts.extend([
        raw(&[0xd800, 0xd800]),
        raw(&[0xd800, 0xdc00]),
        raw(&[0xd83d, 0xde00, 0xd83d, 0xde00]),
    ]);
    let mut cases = Vec::new();
    let mut actual = Vec::new();
    for flags in ["i", "iu"] {
        for pattern in &patterns {
            let regex = JsRegex::with_flags(*pattern, flags).unwrap();
            for text in &texts {
                cases.push(json!({"kind":"sharedRegex","pattern":JsString::from(*pattern).units(),"text":text.units(),"flags":flags}));
                actual.push(regex.is_match(text));
            }
        }
    }
    let response = oracle.request(&json!({"id":4,"cases":cases}));
    let mismatches: Vec<_> = actual
        .iter()
        .enumerate()
        .filter_map(|(index, result)| {
            let expected = response["output"][index]["matched"].as_bool().unwrap();
            (*result != expected).then(|| format!("{:?} expected={expected} actual={result}", cases[index]))
        })
        .collect();
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}
