//! Differential proof for the minimatch 10.2.5 port.

use crate::shared_oracle_2 as oracle;

use pi_js_compat::minimatch::{MinimatchOptions, brace_expand, minimatch};
use serde_json::{Map, Value, json};

const MINIMATCH_MODULE: &str = "node_modules/minimatch/dist/esm/index.js";

const PATTERNS: &[&str] = &[
    "*",
    "**",
    "**/*",
    "**/*.js",
    "*.js",
    "*.{js,ts}",
    "**/*.{js,ts}",
    "a/**",
    "a/**/b",
    "a/*/b",
    "a/*",
    "?",
    "??",
    "a?c",
    "a[bc]d",
    "a[!bc]d",
    "a[a-z]d",
    "[[:alpha:]]*",
    "*.{js,ts,jsx,tsx}",
    "{a,b}/c",
    "a/{b,c}/d",
    "a{1..3}",
    "a{01..03}",
    "a{3..1}",
    "a{a..c}",
    "@(a|b)",
    "*(a|b)",
    "+(a|b)",
    "?(a|b)",
    "!(a|b)",
    "a/!(b|c)/d",
    "a/@(b|c)/d",
    "a/**/!(b|c).js",
    "**/.*",
    ".*",
    "a/.*",
    "a/*.js",
    "**/*.test.js",
    "src/**/*.{ts,tsx}",
    "src/**/test/**",
    "!*.js",
    "!!a",
    "#comment",
    "a/**/../b",
    "./a",
    "a//b",
    "a\\*b",
    "a\\|*",
    "a[b-d]e",
    "a[]]b",
    "a[!]b",
    "[.]",
    "[.]*",
    "*.*",
    "*.",
    "a/**/b/**/c",
    "**/**/a",
    "{a,{b,c}}",
    "{{a,b},c}",
    "{,a}",
    "a{,b}",
    "{1..3..2}",
    "{-2..2}",
    "x{a,b}y{c,d}",
    "a+(b|c)d",
    "a*(b|c)d",
    "a?(b|c)d",
    "a@(b|c)d",
    "a!(b|c)d",
    "a/**/*.js",
    "**/a/**",
    "a/**/b/**",
    "a{b,c",
    "a}b{c",
    "[",
    "[]",
    "[!]",
    "[a-",
    "a\\",
    "**/",
    "/**",
    "a/**/",
    "a/**//b",
];

const PATHS: &[&str] = &[
    "",
    "a",
    "A",
    "b",
    "ab",
    "abc",
    "a/b",
    "a/c",
    "a/b/c",
    "a/b/c.js",
    "a/b/c.ts",
    "a/b/c.jsx",
    "a/b/test/c.js",
    "a/b/test/c.ts",
    ".",
    "..",
    ".a",
    ".a/b",
    "a/.b",
    "a/.b/c",
    "a/b/.c.js",
    "a/b/c.test.js",
    "src/a.ts",
    "src/a.tsx",
    "src/test/a.ts",
    "src/a/b.ts",
    "a1",
    "a2",
    "a3",
    "a01",
    "a02",
    "a-2",
    "a2b",
    "a/b/",
    "/a",
    "/",
    "a//b",
    "a/b/../c",
    "a|b",
    "a*b",
    "a[b]c",
    "a-b",
    "a_b",
    "a b",
    "中/文",
    "a/中.js",
    "a}b",
    "a{b",
    "[",
    "]",
    "a\\b",
    "a/b/c/d/e.js",
];

fn options_json(options: &MinimatchOptions) -> Value {
    let mut map = Map::new();
    if options.nocase {
        map.insert("nocase".to_owned(), Value::Bool(true));
    }
    if options.dot {
        map.insert("dot".to_owned(), Value::Bool(true));
    }
    if options.match_base {
        map.insert("matchBase".to_owned(), Value::Bool(true));
    }
    if options.nobrace {
        map.insert("nobrace".to_owned(), Value::Bool(true));
    }
    if options.noglobstar {
        map.insert("noglobstar".to_owned(), Value::Bool(true));
    }
    if options.noext {
        map.insert("noext".to_owned(), Value::Bool(true));
    }
    if options.nonegate {
        map.insert("nonegate".to_owned(), Value::Bool(true));
    }
    if options.nocomment {
        map.insert("nocomment".to_owned(), Value::Bool(true));
    }
    if options.partial {
        map.insert("partial".to_owned(), Value::Bool(true));
    }
    if options.flip_negate {
        map.insert("flipNegate".to_owned(), Value::Bool(true));
    }
    if options.preserve_multiple_slashes {
        map.insert("preserveMultipleSlashes".to_owned(), Value::Bool(true));
    }
    if options.optimization_level != 1 {
        map.insert("optimizationLevel".to_owned(), Value::from(options.optimization_level));
    }
    if options.nocase_magic_only {
        map.insert("nocaseMagicOnly".to_owned(), Value::Bool(true));
    }
    if options.brace_expand_max != MinimatchOptions::default().brace_expand_max {
        map.insert("braceExpandMax".to_owned(), Value::from(options.brace_expand_max));
    }
    if options.max_globstar_recursion != 200 {
        map.insert(
            "maxGlobstarRecursion".to_owned(),
            Value::from(options.max_globstar_recursion),
        );
    }
    if options.max_extglob_recursion != 2 {
        map.insert(
            "maxExtglobRecursion".to_owned(),
            Value::from(options.max_extglob_recursion),
        );
    }
    Value::Object(map)
}

fn variants() -> Vec<(&'static str, MinimatchOptions)> {
    vec![
        ("default", MinimatchOptions::default()),
        (
            "nocase",
            MinimatchOptions {
                nocase: true,
                ..MinimatchOptions::default()
            },
        ),
        (
            "dot",
            MinimatchOptions {
                dot: true,
                ..MinimatchOptions::default()
            },
        ),
        (
            "matchBase",
            MinimatchOptions {
                match_base: true,
                ..MinimatchOptions::default()
            },
        ),
    ]
}

fn compare_variants(
    oracle: &mut oracle::TsOracle,
    patterns: &[&str],
    paths: &[&str],
    variants: &[(&str, MinimatchOptions)],
    request_id: u64,
) {
    for (name, options) in variants {
        let mut cases = Vec::new();
        let mut labels = Vec::new();
        let mut expected = Vec::new();
        for pattern in patterns {
            for path in paths {
                cases.push(json!({
                    "path": path,
                    "pattern": pattern,
                    "options": options_json(options),
                }));
                labels.push((*path, *pattern));
                expected.push(minimatch(*path, *pattern, options));
            }
        }
        let response = oracle.request(&json!({
            "id": request_id,
            "operation": "minimatch",
            "cases": cases,
        }));
        assert_eq!(response["ok"], true, "oracle failed: {response}");
        let actual: Vec<bool> = response["output"]["results"]
            .as_array()
            .expect("results array")
            .iter()
            .map(|value| value.as_bool().expect("boolean"))
            .collect();
        assert_eq!(actual.len(), expected.len());
        for (index, (actual, expected)) in actual.iter().zip(&expected).enumerate() {
            assert_eq!(
                actual, expected,
                "variant {name}: case {index}: path {:?} pattern {:?}",
                labels[index].0, labels[index].1
            );
        }
    }
}

#[test]
fn matches_minimatch_over_the_default_variants() {
    let mut oracle = oracle::TsOracle::spawn("js-compat-oracle.ts", MINIMATCH_MODULE);
    compare_variants(&mut oracle, PATTERNS, PATHS, &variants(), 1);
}

#[test]
fn matches_minimatch_over_remaining_options() {
    let mut oracle = oracle::TsOracle::spawn("js-compat-oracle.ts", MINIMATCH_MODULE);
    let mut variants = Vec::new();
    type Mutation = fn(&mut MinimatchOptions);
    let mutations: [(&'static str, Mutation); 11] = [
        ("nobrace", |options: &mut MinimatchOptions| options.nobrace = true),
        ("noglobstar", |options: &mut MinimatchOptions| {
            options.noglobstar = true;
        }),
        ("noext", |options: &mut MinimatchOptions| options.noext = true),
        ("nonegate", |options: &mut MinimatchOptions| {
            options.nonegate = true;
        }),
        ("nocomment", |options: &mut MinimatchOptions| {
            options.nocomment = true;
        }),
        ("partial", |options: &mut MinimatchOptions| {
            options.partial = true;
        }),
        ("flipNegate", |options: &mut MinimatchOptions| {
            options.flip_negate = true;
        }),
        ("preserveMultipleSlashes", |options: &mut MinimatchOptions| {
            options.preserve_multiple_slashes = true;
        }),
        ("optimizationLevel0", |options: &mut MinimatchOptions| {
            options.optimization_level = 0;
        }),
        ("nocaseMagicOnly", |options: &mut MinimatchOptions| {
            options.nocase = true;
            options.nocase_magic_only = true;
        }),
        ("nocaseDot", |options: &mut MinimatchOptions| {
            options.nocase = true;
            options.dot = true;
        }),
    ];
    for (name, mutate) in mutations {
        let mut options = MinimatchOptions::default();
        mutate(&mut options);
        variants.push((name, options));
    }
    let patterns: Vec<&str> = PATTERNS.iter().copied().take(45).collect();
    compare_variants(&mut oracle, &patterns, PATHS, &variants, 2);
}

#[test]
fn matches_minimatch_brace_expansion() {
    let mut oracle = oracle::TsOracle::spawn("js-compat-oracle.ts", MINIMATCH_MODULE);
    let patterns = [
        "a{b,c}d",
        "a{1..3}b",
        "a{01..03}b",
        "a{3..1}b",
        "a{-2..2}b",
        "a{a..c}b",
        "a{1..5..2}b",
        "a{,b}c",
        "a{b,}c",
        "{a,{b,c}}d",
        "{{a,b},c}d",
        "a{b}c",
        "a{b,c",
        "a}b{c",
        "{a,b}{c,d}",
        "x{1..3}y{a,b}z",
        "a\\{b,c}d",
        "{}a",
        "a{}",
        "a{b,{c,d}e}f",
        "a{1..3",
    ];
    let options = MinimatchOptions::default();
    let response = oracle.request(&json!({
        "id": 3,
        "operation": "brace_expand",
        "patterns": patterns,
        "options": options_json(&options),
    }));
    assert_eq!(response["ok"], true, "oracle failed: {response}");
    let expected: Vec<Vec<String>> = response["output"]["results"]
        .as_array()
        .expect("results array")
        .iter()
        .map(|value| {
            value
                .as_array()
                .expect("expansion array")
                .iter()
                .map(|item| item.as_str().expect("string").to_owned())
                .collect()
        })
        .collect();
    let actual: Vec<Vec<pi_js_compat::js_json::JsString>> = patterns
        .iter()
        .map(|pattern| brace_expand(*pattern, &options))
        .collect();
    assert_eq!(actual, expected);
}
