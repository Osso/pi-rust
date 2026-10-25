use crate::shared_oracle as oracle;

use pi_js_compat::js_json::JsString;
use std::collections::BTreeSet;

use pi_tui::word_navigation::{find_word_backward, find_word_forward};
use serde_json::{Value, json};

const SOURCE_DIRECTORY: &str = "packages/tui/src";

const CORPUS: &[&str] = &[
    "",
    "a",
    "hello world",
    "foo.bar",
    "foo:bar",
    "path/to/file",
    "foo...bar",
    "  hello  ",
    "\t\n x",
    "a_b",
    "3.14",
    "can't",
    "don’t",
    "x²",
    "a-b",
    "foo::bar",
    "--flag",
    "A1B2",
    "你好世界 test",
    "日本語テスト",
    "한국어 테스트",
    "中国人民",
    "สวัสดีชาวโลก",
    "ภาษาไทย",
    "👨‍👩‍👧‍👦 x",
    "👍🏽ok",
    "éclair",
    "e\u{0301}clair",
    "🇮🇹🇫🇷 flags",
    "a\u{00a0}b",
    "x\u{200b}y",
    "zero\u{200d}width",
    "مرحبا بالعالم",
    "שלום עולם",
    "Ωμέγα λόγος",
    "Привет мир",
    "word_with_underscores and-dashes",
    "http://example.com/a/b?q=1",
    "@file path/with space.txt",
    "[paste #1 +5 lines] world",
    "e.g., i.e.; etc!",
    "(paren) {brace} [bracket]",
    "a/b\\c|d&e=f",
    "123 456.789 0x10",
    "ＡＢＣ　１２３",
    "क ख ग घ",
    "ᚠᚢᚦᚨᚱᚲ",
    "\u{0000}\u{0001}",
];

/// Scripts where ICU4X's dictionary/LSTM segmentation can differ from ICU4C's.
const DICTIONARY_SENSITIVE_RANGES: &[(char, char)] = &[
    ('\u{0e00}', '\u{0e7f}'),   // Thai
    ('\u{0e80}', '\u{0eff}'),   // Lao
    ('\u{1000}', '\u{109f}'),   // Myanmar
    ('\u{1100}', '\u{11ff}'),   // Hangul Jamo
    ('\u{1780}', '\u{17ff}'),   // Khmer
    ('\u{3040}', '\u{309f}'),   // Hiragana
    ('\u{30a0}', '\u{30ff}'),   // Katakana
    ('\u{3400}', '\u{4dbf}'),   // CJK extension A
    ('\u{4e00}', '\u{9fff}'),   // CJK unified ideographs
    ('\u{ac00}', '\u{d7af}'),   // Hangul syllables
    ('\u{f900}', '\u{faff}'),   // CJK compatibility ideographs
    ('\u{ff66}', '\u{ff9d}'),   // Halfwidth Katakana
    ('\u{20000}', '\u{2fa1f}'), // CJK extensions B+
];

fn is_dictionary_sensitive(text: &str) -> bool {
    text.chars().any(|character| {
        DICTIONARY_SENSITIVE_RANGES
            .iter()
            .any(|(start, end)| (*start..=*end).contains(&character))
    })
}

fn generated_corpus() -> Vec<String> {
    let tokens = [
        "alpha", "βeta", "你", "好", "中", "国", "test", " ", ".", "/", ":", "_", "-", "1", "😀", "é", "\u{0301}", "(",
        ")", "\\", "@", "\t",
    ];
    let mut corpus = Vec::new();
    for first in tokens {
        for second in tokens {
            corpus.push(format!("{first}{second}"));
            corpus.push(format!("{first}{second}{first}"));
        }
    }
    corpus
}

fn boundary_offsets(text: &str) -> Vec<usize> {
    (0..=JsString::from(text).len()).collect()
}

struct Case {
    text: String,
    cursor: usize,
}

#[test]
fn word_navigation_matches_the_pinned_typescript_source_outside_dictionary_scripts() {
    let mut texts: Vec<String> = CORPUS.iter().map(|text| (*text).to_string()).collect();
    texts.extend(generated_corpus());

    let cases: Vec<Case> = texts
        .into_iter()
        .flat_map(|text| {
            boundary_offsets(&text).into_iter().map(move |cursor| Case {
                text: text.clone(),
                cursor,
            })
        })
        .collect();

    let requests: Vec<Value> = cases
        .iter()
        .map(|case| json!({ "text": case.text, "cursor": case.cursor }))
        .collect();

    let mut oracle = oracle::TsOracle::spawn("word-navigation-oracle.ts", SOURCE_DIRECTORY);
    let response = oracle.request(&json!({
        "id": "word-navigation",
        "operation": "word_navigation",
        "cases": requests,
    }));
    assert_eq!(response["ok"], true, "oracle failed: {response}");

    let results = response["output"]["results"].as_array().expect("results array");
    assert_eq!(results.len(), cases.len());

    let mut mismatched_texts: BTreeSet<String> = BTreeSet::new();
    for (case, expected) in cases.iter().zip(results) {
        let expected_backward = expected["backward"].as_u64().expect("backward");
        let expected_forward = expected["forward"].as_u64().expect("forward");
        let actual_backward = find_word_backward(&JsString::from(&case.text), case.cursor, None) as u64;
        let actual_forward = find_word_forward(&JsString::from(&case.text), case.cursor, None) as u64;

        if actual_backward != expected_backward || actual_forward != expected_forward {
            assert!(
                is_dictionary_sensitive(&case.text),
                "unexpected segmentation mismatch outside dictionary scripts for {:?} at unit {}: \
                 backward {actual_backward} != {expected_backward}, forward {actual_forward} != {expected_forward}",
                case.text,
                case.cursor
            );
            mismatched_texts.insert(case.text.clone());
        }
    }

    for documented in ["好好好", "中国人民", "สวัสดีชาวโลก"] {
        assert!(
            mismatched_texts.contains(documented),
            "documented ICU4X dictionary divergence {documented:?} no longer differs; update the traceability doc"
        );
    }
}

/// Pins the approved ICU4X-vs-ICU4C dictionary divergence. The TypeScript
/// values come from the pinned Node 26 / ICU 78.3 oracle; the Rust values are
/// ICU4X 2.3.0 segment boundaries.
#[test]
fn known_icu4x_dictionary_divergences_are_pinned() {
    struct Divergence {
        text: &'static str,
        cursor: usize,
        rust_backward: usize,
        rust_forward: usize,
        ts_backward: usize,
        ts_forward: usize,
    }

    let cases = [
        Divergence {
            text: "好好好",
            cursor: 0,
            rust_backward: 0,
            rust_forward: 2,
            ts_backward: 0,
            ts_forward: 1,
        },
        Divergence {
            text: "好好好",
            cursor: 2,
            rust_backward: 0,
            rust_forward: 3,
            ts_backward: 0,
            ts_forward: 3,
        },
        Divergence {
            text: "中国人民",
            cursor: 0,
            rust_backward: 0,
            rust_forward: 3,
            ts_backward: 0,
            ts_forward: 2,
        },
        Divergence {
            text: "中国人民",
            cursor: 4,
            rust_backward: 3,
            rust_forward: 4,
            ts_backward: 2,
            ts_forward: 4,
        },
        Divergence {
            text: "สวัสดีชาวโลก",
            cursor: 1,
            rust_backward: 0,
            rust_forward: 6,
            ts_backward: 0,
            ts_forward: 4,
        },
    ];

    let requests: Vec<Value> = cases
        .iter()
        .map(|case| json!({ "text": case.text, "cursor": case.cursor }))
        .collect();
    let mut oracle = oracle::TsOracle::spawn("word-navigation-oracle.ts", SOURCE_DIRECTORY);
    let response = oracle.request(&json!({
        "id": "word-navigation-divergences",
        "operation": "word_navigation",
        "cases": requests,
    }));
    assert_eq!(response["ok"], true, "oracle failed: {response}");
    let results = response["output"]["results"].as_array().expect("results array");

    for (case, expected) in cases.iter().zip(results) {
        assert_eq!(
            find_word_backward(&JsString::from(case.text), case.cursor, None),
            case.rust_backward,
            "ICU4X backward drift for {:?} at unit {}",
            case.text,
            case.cursor
        );
        assert_eq!(
            find_word_forward(&JsString::from(case.text), case.cursor, None),
            case.rust_forward,
            "ICU4X forward drift for {:?} at unit {}",
            case.text,
            case.cursor
        );
        assert_eq!(
            expected["backward"].as_u64().expect("backward") as usize,
            case.ts_backward,
            "TypeScript backward drift for {:?} at unit {}",
            case.text,
            case.cursor
        );
        assert_eq!(
            expected["forward"].as_u64().expect("forward") as usize,
            case.ts_forward,
            "TypeScript forward drift for {:?} at unit {}",
            case.text,
            case.cursor
        );
    }
}
