use crate::shared_oracle_2 as oracle;

use pi_js_compat::js_json::JsString;
use pi_js_compat::node_ignore::Ignore;
use serde_json::{Value, json};

fn compare(oracle: &mut oracle::TsOracle, rules: &[JsString], paths: &[JsString]) {
    let expected = oracle.request(&json!({
        "rules": rules.iter().map(JsString::units).collect::<Vec<_>>(),
        "paths": paths.iter().map(JsString::units).collect::<Vec<_>>()
    }));
    let mut matcher = Ignore::new();
    for rule in rules {
        matcher.add(rule.clone());
    }
    let actual: Vec<Value> = paths
        .iter()
        .map(|path| match matcher.ignores(path) {
            Ok(ignored) => json!({"ignored": ignored}),
            Err(error) => json!({"name": error.name.units(), "message": error.message.units()}),
        })
        .collect();
    let expected = expected.as_array().expect("oracle returns one result per path");
    assert_eq!(actual.len(), expected.len());
    for ((actual, expected), path) in actual.iter().zip(expected).zip(paths) {
        assert_eq!(actual, expected, "rules: {rules:?}; path: {path:?}");
    }
}

#[test]
fn frozen_node_ignore_utf16_rules_paths_and_errors() {
    let mut oracle = oracle::TsOracle::spawn("node-ignore-oracle.mjs", "node_modules/ignore/index.js");
    let mut paths: Vec<JsString> = [
        "", ".", "..", "./a", "../a", "/a", "a", "A", "a/", "a/b", "a/b/", "a/b/c", "b", "b/a", "b/a/", "a.js",
        "x/a.js", "x/y/a.js", "a ", "a  ", "a*b", "#foo", "!foo", "[", "z", "0", "ä", "Ä", "a//b", "a/./b", "a/../b",
    ]
    .into_iter()
    .map(JsString::from)
    .collect();
    paths.extend(
        [
            vec![0xd800],
            vec![0xfffd],
            vec![0xdc00],
            vec![0xd83d, 0xde00],
            vec![0xd800, 47, 0xdc00],
        ]
        .into_iter()
        .map(JsString::from_utf16),
    );
    let patterns = [
        "",
        " ",
        "#comment",
        "!a",
        "a",
        "a/",
        "/a",
        "*",
        "?",
        "??",
        "*.js",
        "**/*.js",
        "a/**",
        "a/**/b",
        "**/a",
        "a/*",
        "a**b",
        "a\\*b",
        "\\#foo",
        "\\!foo",
        "a ",
        "a\\ ",
        "a\\  ",
        "a\\\\ ",
        "a\\",
        "a\\\\",
        "[",
        "[a-z]",
        "[z-a]",
        "[!a]",
        "[^a]",
        "[]",
        "[a\\]",
        "a?b",
        "a.b",
        "a+b",
        "a{b}",
        "\u{feff}a",
        "ä",
        "a\r\nb\n!a",
        "a\\\r",
        "a\r",
        "a\\\r\nb",
        "a\\\r\r\n",
    ];
    for pattern in [
        JsString::from(r"\[[\-"),
        JsString::from("[ä-a]"),
        JsString::from_utf16(vec![91, 0xdfff, 45, 0xd800, 93]),
        JsString::from("[\\u00ff-\\u0001]"),
    ] {
        compare(&mut oracle, &[pattern], &paths);
    }
    for pattern in patterns {
        compare(&mut oracle, &[pattern.into()], &paths);
    }
    for pattern in &paths[31..] {
        compare(&mut oracle, std::slice::from_ref(pattern), &paths);
    }
    for rules in [
        vec!["a/", "!a/b"],
        vec!["*", "!a", "a"],
        vec!["a", "!a", "a/b"],
        vec!["*.js", "!a.js", "a.js"],
        vec!["a/", "!a/", "a/b"],
        vec!["![ä-a]"],
        vec!["*", "[ä-a]"],
        vec!["a/", "[ä-a]"],
        vec!["node_modules/\n*.log\n/build/\n!keep.log", "**/cache/**"],
    ] {
        compare(
            &mut oracle,
            &rules.into_iter().map(JsString::from).collect::<Vec<_>>(),
            &paths,
        );
    }
    let alphabet = [91, 93, 92, 97, 122, 48, 45, 47, 42, 63, 33, 32, 0xd800, 0xdc00, 0xfffd];
    let mut seed = 0x18_u64;
    for index in 0..600 {
        let units = (0..(index % 9 + 1))
            .map(|_| {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                alphabet[(seed >> 32) as usize % alphabet.len()]
            })
            .collect();
        compare(&mut oracle, &[JsString::from_utf16(units)], &paths);
    }
    let atoms = ["a", "b", "*", "**", "?", "[ab]", "[z-a]", "\\*", " ", "\\ ", "é"];
    let original_paths = paths.clone();
    for base in &original_paths[1..] {
        for prefix in ["a/", "b/", "x/y/"] {
            paths.push(JsString::from(prefix) + base);
            paths.push(JsString::from(prefix) + base + "/");
        }
    }
    for first in &original_paths[31..] {
        for second in ["*", "?", "/**/", "[a-z]", "\\\\"] {
            compare(&mut oracle, &[first.clone() + second], &paths);
        }
    }
    for first in atoms {
        for second in atoms {
            for separator in ["", "/", "/**/"] {
                compare(&mut oracle, &[format!("{first}{separator}{second}").into()], &paths);
            }
        }
    }
}

#[test]
fn additions_and_clone_do_not_reuse_stale_results() {
    let mut matcher = Ignore::default();
    assert!(!matcher.ignores("a/b").unwrap());
    matcher.add("a/");
    assert!(matcher.ignores("a/b").unwrap());
    let mut cloned = matcher.clone();
    cloned.add("!a/");
    assert!(!cloned.ignores("a/b").unwrap());
    assert!(matcher.ignores("a/b").unwrap());
}
