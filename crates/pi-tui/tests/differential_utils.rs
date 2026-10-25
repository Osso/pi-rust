use crate::shared_oracle as oracle;

use pi_js_compat::js_json::JsString;
use pi_tui::utils::{
    apply_background_to_line, extract_ansi_code, extract_segments, is_punctuation_char, is_whitespace_char,
    normalize_terminal_output, slice_with_width, truncate_to_width_with, visible_width, wrap_text_with_ansi,
};
use serde_json::{Value, json};

const SOURCE_DIRECTORY: &str = "packages/tui/src";

const CORPUS: &[&str] = &[
    "",
    "hello",
    "hello world",
    "  ",
    "\t",
    "a\tb",
    "网络",
    "ネットワーク",
    "が",
    "か\u{3099}",
    "e\u{0301}",
    "čřžůú",
    "שָׁ",
    "بّ",
    "རྐ",
    "ᜠ᜴",
    "가〮",
    "가〯",
    "र्क",
    "नेटवर्क",
    "सर्वाधिकार सुरक्षित। ऑर्डर पर क्लिक करें",
    "র্ক",
    "ર્ક",
    "ର୍କ",
    "ర్క",
    "ര്‍ക",
    "ကာ",
    "ကေ",
    "က်",
    "ကျ",
    "ကြ",
    "ကဳ",
    "ကဴ",
    "ကဵ",
    "ကး",
    "ကို",
    "က္",
    "ำ",
    "ຳ",
    "กำ",
    "ກຳ",
    "🇨",
    "🇨🇳",
    "🇯🇵",
    "🇺🇸",
    "👍",
    "👍🏻",
    "✅",
    "⚡",
    "⚡️",
    "👨",
    "👨‍💻",
    "🏳️‍🌈",
    "❤️",
    "©️",
    "1️⃣",
    "☝🏻",
    "❤️‍🔥",
    "⌚",
    "☺️",
    "ℹ️",
    "↔️",
    "🏴󠁧󠁢󠁳󠁣󠁴󠁿",
    "👩🏽‍🚀",
    "\x1b[31mred\x1b[0m",
    "\x1b[1;38;5;240mtext\x1b[0m",
    "\x1b]133;A\x07hello\x1b]133;B\x07",
    "\x1b]133;A\x1b\\hello\x1b]133;B\x1b\\",
    "\x1b]8;;https://example.com\x1b\\link\x1b]8;;\x1b\\",
    "\x1b]8;;https://example.com\x07link\x1b]8;;\x07",
    "\x1b_payload\x07text\x1b_",
    "a\x1bnot-ansi",
    "abc\x1bnot-ansi 🙂🙂",
    "\x1b[31m界\x1b[0m",
    "  \x1b[4munderlined\x1b[24m  ",
    "🙂\t界 \x1b_abc\x07",
    "a\u{200d}b",
    "क्\u{200d}ष",
    "한국어",
    "ＡＢＣ１２３",
    "ｱｲｳ",
    "\u{feff}zero",
    "\u{00a0}nbsp",
    "\u{0085}nel",
];

fn oracle_request(oracle: &mut oracle::TsOracle, operation: &str, field: &str, cases: Value) -> Vec<Value> {
    let response = oracle.request(&json!({
        "id": operation,
        "operation": operation,
        field: cases,
    }));
    assert_eq!(response["ok"], true, "oracle failed for {operation}: {response}");
    response["output"]["results"].as_array().expect("results array").clone()
}

#[test]
fn visible_width_matches_the_pinned_typescript_source() {
    let mut oracle = oracle::TsOracle::spawn("tui-leaves-oracle.ts", SOURCE_DIRECTORY);
    let results = oracle_request(&mut oracle, "utils_visible_width", "cases", json!(CORPUS));
    assert_eq!(results.len(), CORPUS.len());
    for (value, expected) in CORPUS.iter().zip(results) {
        assert_eq!(
            visible_width(*value),
            expected.as_u64().unwrap() as usize,
            "width mismatch for {value:?}"
        );
    }
}

#[test]
fn visible_width_matches_for_broad_codepoint_samples() {
    let mut oracle = oracle::TsOracle::spawn("tui-leaves-oracle.ts", SOURCE_DIRECTORY);
    let mut samples: Vec<String> = Vec::new();
    for (start, end) in [
        (0x0000, 0x02ff),
        (0x0300, 0x036f),
        (0x0590, 0x05ff),
        (0x0600, 0x06ff),
        (0x0900, 0x0dff),
        (0x0e00, 0x0eff),
        (0x1000, 0x109f),
        (0x1100, 0x11ff),
        (0x1ab0, 0x1aff),
        (0x1f000, 0x1faff),
        (0x2000, 0x206f),
        (0x2600, 0x27bf),
        (0x2b50, 0x2b55),
        (0x3000, 0x30ff),
        (0x4e00, 0x4eff),
        (0xac00, 0xacff),
        (0xfe00, 0xfe0f),
        (0xff00, 0xffef),
    ] {
        for code_point in start..=end {
            if let Some(character) = char::from_u32(code_point) {
                samples.push(character.to_string());
            }
        }
    }
    for code_point in 0x1f000..=0x1faff {
        if let Some(character) = char::from_u32(code_point) {
            samples.push(format!("{character}\u{fe0f}"));
        }
    }
    for code_point in 0x2600..=0x27bf {
        if let Some(character) = char::from_u32(code_point) {
            samples.push(format!("{character}\u{fe0f}"));
        }
    }
    for first in 0x1f1e6..=0x1f1ff {
        for second in 0x1f1e6..=0x1f1ff {
            samples.push(format!(
                "{}{}",
                char::from_u32(first).unwrap(),
                char::from_u32(second).unwrap()
            ));
        }
    }

    const BATCH: usize = 2000;
    for batch in samples.chunks(BATCH) {
        let results = oracle_request(&mut oracle, "utils_visible_width", "cases", json!(batch));
        for (sample, expected) in batch.iter().zip(results) {
            assert_eq!(
                visible_width(sample),
                expected.as_u64().unwrap() as usize,
                "width mismatch for {sample:?}"
            );
        }
    }
}

#[test]
fn normalize_terminal_output_matches_the_pinned_typescript_source() {
    let mut oracle = oracle::TsOracle::spawn("tui-leaves-oracle.ts", SOURCE_DIRECTORY);
    let results = oracle_request(&mut oracle, "utils_normalize", "cases", json!(CORPUS));
    for (value, expected) in CORPUS.iter().zip(results) {
        assert_eq!(
            normalize_terminal_output(*value),
            expected.as_str().unwrap(),
            "normalize mismatch for {value:?}"
        );
    }
}

fn scan_ansi(value: &str) -> Vec<JsString> {
    let value = JsString::from(value);
    let mut codes = Vec::new();
    let mut index = 0;
    while index < value.len() {
        if let Some(ansi) = extract_ansi_code(&value, index) {
            codes.push(ansi.code);
            index += ansi.length;
        } else {
            index += 1;
        }
    }
    codes
}

#[test]
fn extract_ansi_code_matches_the_pinned_typescript_source() {
    let mut oracle = oracle::TsOracle::spawn("tui-leaves-oracle.ts", SOURCE_DIRECTORY);
    let results = oracle_request(&mut oracle, "utils_extract_ansi", "cases", json!(CORPUS));
    for (value, expected) in CORPUS.iter().zip(results) {
        let expected: Vec<String> = expected
            .as_array()
            .unwrap()
            .iter()
            .map(|code| code.as_str().unwrap().to_string())
            .collect();
        assert_eq!(scan_ansi(value), expected, "ansi scan mismatch for {value:?}");
    }
}

struct TruncateCase {
    text: &'static str,
    max_width: usize,
    ellipsis: &'static str,
    pad: bool,
}

fn truncate_cases() -> Vec<TruncateCase> {
    let long_styled = format!("\x1b[31m{}", "hello".repeat(100));
    let long_malformed = format!("abc\x1bnot-ansi {}", "🙂".repeat(1000));
    let link_open = "\x1b]8;;https://example.com\x07";
    let link_close = "\x1b]8;;\x07";
    let link_text = format!("{link_open}some-longer-label-here{link_close}");
    vec![
        TruncateCase {
            text: "abcdef",
            max_width: 1,
            ellipsis: "🙂",
            pad: false,
        },
        TruncateCase {
            text: "abcdef",
            max_width: 2,
            ellipsis: "🙂",
            pad: false,
        },
        TruncateCase {
            text: "a",
            max_width: 2,
            ellipsis: "🙂",
            pad: false,
        },
        TruncateCase {
            text: "界",
            max_width: 2,
            ellipsis: "🙂",
            pad: false,
        },
        TruncateCase {
            text: "hello world",
            max_width: 8,
            ellipsis: "...",
            pad: false,
        },
        TruncateCase {
            text: "hello world",
            max_width: 8,
            ellipsis: "...",
            pad: true,
        },
        TruncateCase {
            text: "hello",
            max_width: 10,
            ellipsis: "...",
            pad: true,
        },
        TruncateCase {
            text: "",
            max_width: 5,
            ellipsis: "...",
            pad: true,
        },
        TruncateCase {
            text: "🙂界🙂界🙂界",
            max_width: 8,
            ellipsis: "…",
            pad: true,
        },
        TruncateCase {
            text: "🙂\t界 \x1b_abc\x07",
            max_width: 7,
            ellipsis: "…",
            pad: true,
        },
        TruncateCase {
            text: "网络网络网络",
            max_width: 7,
            ellipsis: "…",
            pad: false,
        },
        TruncateCase {
            text: "🇨🇳🇺🇸🇯🇵",
            max_width: 5,
            ellipsis: "…",
            pad: false,
        },
        TruncateCase {
            text: "e\u{0301}e\u{0301}e\u{0301}",
            max_width: 2,
            ellipsis: "…",
            pad: false,
        },
        TruncateCase {
            text: "क्\u{200d}षक्\u{200d}ष",
            max_width: 3,
            ellipsis: "…",
            pad: false,
        },
        TruncateCase {
            text: "\x1b[31mred text\x1b[0m",
            max_width: 6,
            ellipsis: "...",
            pad: false,
        },
        TruncateCase {
            text: "a\tb\tc",
            max_width: 5,
            ellipsis: "…",
            pad: false,
        },
        TruncateCase {
            text: "abcdef",
            max_width: 3,
            ellipsis: "",
            pad: false,
        },
        TruncateCase {
            text: "abc",
            max_width: 0,
            ellipsis: "...",
            pad: true,
        },
        TruncateCase {
            text: Box::leak(long_styled.into_boxed_str()),
            max_width: 10,
            ellipsis: "",
            pad: false,
        },
        TruncateCase {
            text: Box::leak(link_text.into_boxed_str()),
            max_width: 15,
            ellipsis: "...",
            pad: false,
        },
        TruncateCase {
            text: Box::leak(long_malformed.into_boxed_str()),
            max_width: 20,
            ellipsis: "…",
            pad: false,
        },
        TruncateCase {
            text: "   ",
            max_width: 2,
            ellipsis: "...",
            pad: false,
        },
        TruncateCase {
            text: "\x1b[41m背景\x1b[0m",
            max_width: 3,
            ellipsis: "…",
            pad: true,
        },
        TruncateCase {
            text: "👨‍👩‍👧‍👦family",
            max_width: 6,
            ellipsis: "…",
            pad: false,
        },
    ]
}

#[test]
fn truncate_to_width_matches_the_pinned_typescript_source() {
    let mut oracle = oracle::TsOracle::spawn("tui-leaves-oracle.ts", SOURCE_DIRECTORY);
    let cases = truncate_cases();
    let request: Vec<Value> = cases
        .iter()
        .map(|case| {
            json!({
                "text": case.text,
                "maxWidth": case.max_width,
                "ellipsis": case.ellipsis,
                "pad": case.pad,
            })
        })
        .collect();
    let results = oracle_request(&mut oracle, "utils_truncate", "cases", json!(request));
    for (case, expected) in cases.iter().zip(results) {
        let actual = truncate_to_width_with(case.text, case.max_width, case.ellipsis, case.pad);
        assert_eq!(
            actual,
            expected.as_str().unwrap(),
            "truncate mismatch for {:?} width {} ellipsis {:?} pad {}",
            case.text,
            case.max_width,
            case.ellipsis,
            case.pad
        );
    }
}

struct WrapCase {
    text: &'static str,
    width: usize,
}

fn wrap_cases() -> Vec<WrapCase> {
    let red = "\x1b[31m";
    let reset = "\x1b[0m";
    let underline_on = "\x1b[4m";
    let underline_off = "\x1b[24m";
    let url = "https://example.com/very/long/path/that/will/wrap";
    let cjk = "This is an example 中文汉字测试段落内容中文汉字测试段落内容.";
    let oauth_url = format!("https://example.com/oauth/{}", "a".repeat(32));
    vec![
        WrapCase {
            text: "first\nsecond\r\nthird\rfourth",
            width: 80,
        },
        WrapCase {
            text: Box::leak(format!("{red}first\r\nsecond\rthird{reset}").into_boxed_str()),
            width: 80,
        },
        WrapCase {
            text: "hello world this is a test",
            width: 10,
        },
        WrapCase { text: cjk, width: 40 },
        WrapCase {
            text: Box::leak(format!("{red}{cjk}{reset}").into_boxed_str()),
            width: 40,
        },
        WrapCase { text: "  ", width: 1 },
        WrapCase {
            text: Box::leak(format!("read this thread {underline_on}{url}{underline_off}").into_boxed_str()),
            width: 40,
        },
        WrapCase {
            text: Box::leak(format!("{underline_on}underlined text here {underline_off}more").into_boxed_str()),
            width: 18,
        },
        WrapCase {
            text: Box::leak(format!("{underline_on}UNDERLINED_CONTENT_THAT_WRAPS{underline_off}").into_boxed_str()),
            width: 20,
        },
        WrapCase {
            text: Box::leak(format!("\x1b[44mhello world this is blue background text{reset}").into_boxed_str()),
            width: 15,
        },
        WrapCase {
            text: Box::leak(
                format!("\x1b[41mprefix {underline_on}UNDERLINED_CONTENT_THAT_WRAPS{underline_off} suffix{reset}")
                    .into_boxed_str(),
            ),
            width: 20,
        },
        WrapCase {
            text: Box::leak(
                "\x1b]8;;https://example.com\x1b\\0123456789\x1b]8;;\x1b\\"
                    .to_string()
                    .into_boxed_str(),
            ),
            width: 6,
        },
        WrapCase {
            text: Box::leak(format!("\x1b]8;;{oauth_url}\x07{oauth_url}\x1b]8;;\x07").into_boxed_str()),
            width: 20,
        },
        WrapCase {
            text: "before \x1b]8;;https://example.com\x1b\\link\x1b]8;;\x1b\\ after",
            width: 80,
        },
        WrapCase {
            text: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            width: 10,
        },
        WrapCase {
            text: "\t\t\t",
            width: 4,
        },
        WrapCase {
            text: "ab\tcd\tef",
            width: 5,
        },
        WrapCase {
            text: "🇨🇳🇺🇸🇯🇵",
            width: 4,
        },
        WrapCase {
            text: "👨‍👩‍👧‍👦x", width: 3
        },
        WrapCase {
            text: "word-with-hyphen-and/slash",
            width: 7,
        },
        WrapCase {
            text: "e\u{0301}e\u{0301}e\u{0301}e\u{0301}",
            width: 2,
        },
        WrapCase {
            text: "क्\u{200d}षक्\u{200d}ष",
            width: 2,
        },
        WrapCase { text: "", width: 10 },
    ]
}

#[test]
fn wrap_text_with_ansi_matches_the_pinned_typescript_source() {
    let mut oracle = oracle::TsOracle::spawn("tui-leaves-oracle.ts", SOURCE_DIRECTORY);
    let cases = wrap_cases();
    let request: Vec<Value> = cases
        .iter()
        .map(|case| json!({ "text": case.text, "width": case.width }))
        .collect();
    let results = oracle_request(&mut oracle, "utils_wrap", "cases", json!(request));
    for (case, expected) in cases.iter().zip(results) {
        let expected: Vec<String> = expected
            .as_array()
            .unwrap()
            .iter()
            .map(|line| line.as_str().unwrap().to_string())
            .collect();
        let actual = wrap_text_with_ansi(case.text, case.width);
        assert_eq!(
            actual, expected,
            "wrap mismatch for {:?} width {}",
            case.text, case.width
        );
    }
}

#[derive(Debug)]
struct SliceCase {
    line: &'static str,
    start: usize,
    length: usize,
    strict: bool,
}

const SLICE_CASES: &[SliceCase] = &[
    SliceCase {
        line: "out 192M\t.pi/skill-tests/results-ha",
        start: 0,
        length: 10,
        strict: true,
    },
    SliceCase {
        line: "out 192M\t.pi/skill-tests/results-ha",
        start: 11,
        length: 10,
        strict: true,
    },
    SliceCase {
        line: "abcdef",
        start: 1,
        length: 3,
        strict: false,
    },
    SliceCase {
        line: "a界b",
        start: 1,
        length: 2,
        strict: true,
    },
    SliceCase {
        line: "a界b",
        start: 1,
        length: 2,
        strict: false,
    },
    SliceCase {
        line: "a界b",
        start: 1,
        length: 1,
        strict: true,
    },
    SliceCase {
        line: "a界b",
        start: 0,
        length: 3,
        strict: true,
    },
    SliceCase {
        line: "\x1b[31mabc\x1b[0m",
        start: 0,
        length: 2,
        strict: false,
    },
    SliceCase {
        line: "\x1b[31mabc\x1b[0m",
        start: 1,
        length: 2,
        strict: false,
    },
    SliceCase {
        line: "a\x1b[31mbc\x1b[0m",
        start: 1,
        length: 2,
        strict: false,
    },
    SliceCase {
        line: "\x1b]8;;https://a.test\x07link\x1b]8;;\x07",
        start: 0,
        length: 4,
        strict: false,
    },
    SliceCase {
        line: "🇨🇳🇺🇸",
        start: 0,
        length: 3,
        strict: false,
    },
    SliceCase {
        line: "🇨🇳🇺🇸",
        start: 0,
        length: 3,
        strict: true,
    },
    SliceCase {
        line: "e\u{0301}e\u{0301}",
        start: 0,
        length: 1,
        strict: false,
    },
    SliceCase {
        line: "",
        start: 0,
        length: 5,
        strict: false,
    },
    SliceCase {
        line: "abc",
        start: 5,
        length: 2,
        strict: false,
    },
    SliceCase {
        line: "abc",
        start: 0,
        length: 0,
        strict: false,
    },
    SliceCase {
        line: "\t\t\t",
        start: 3,
        length: 6,
        strict: false,
    },
];

#[test]
fn slice_with_width_matches_the_pinned_typescript_source() {
    let mut oracle = oracle::TsOracle::spawn("tui-leaves-oracle.ts", SOURCE_DIRECTORY);
    let request: Vec<Value> = SLICE_CASES
        .iter()
        .map(|case| {
            json!({
                "line": case.line,
                "start": case.start,
                "length": case.length,
                "strict": case.strict,
            })
        })
        .collect();
    let results = oracle_request(&mut oracle, "utils_slice", "cases", json!(request));
    for (case, expected) in SLICE_CASES.iter().zip(results) {
        let actual = slice_with_width(case.line, case.start, case.length, case.strict);
        assert_eq!(
            actual.text,
            expected["text"].as_str().unwrap(),
            "slice text mismatch for {case:?}"
        );
        assert_eq!(
            actual.width,
            expected["width"].as_u64().unwrap() as usize,
            "slice width mismatch for {case:?}"
        );
    }
}

#[derive(Debug)]
struct SegmentCase {
    line: &'static str,
    before_end: usize,
    after_start: usize,
    after_len: usize,
    strict_after: bool,
}

const SEGMENT_CASES: &[SegmentCase] = &[
    SegmentCase {
        line: "out 192M\t.pi/skill-tests/results-ha",
        before_end: 10,
        after_start: 13,
        after_len: 10,
        strict_after: true,
    },
    SegmentCase {
        line: "out 192M\t.pi/skill-tests/results-ha",
        before_end: 11,
        after_start: 13,
        after_len: 10,
        strict_after: true,
    },
    SegmentCase {
        line: "\x1b[31mabcdef\x1b[0m",
        before_end: 2,
        after_start: 4,
        after_len: 2,
        strict_after: false,
    },
    SegmentCase {
        line: "\x1b[31mabcdef\x1b[0m",
        before_end: 2,
        after_start: 4,
        after_len: 2,
        strict_after: true,
    },
    SegmentCase {
        line: "a界b",
        before_end: 2,
        after_start: 3,
        after_len: 1,
        strict_after: true,
    },
    SegmentCase {
        line: "a界b",
        before_end: 1,
        after_start: 2,
        after_len: 1,
        strict_after: false,
    },
    SegmentCase {
        line: "\x1b]8;;https://a.test\x07link\x1b]8;;\x07",
        before_end: 2,
        after_start: 3,
        after_len: 1,
        strict_after: false,
    },
    SegmentCase {
        line: "abcdef",
        before_end: 2,
        after_start: 4,
        after_len: 0,
        strict_after: false,
    },
    SegmentCase {
        line: "",
        before_end: 2,
        after_start: 4,
        after_len: 2,
        strict_after: false,
    },
];

#[test]
fn extract_segments_matches_the_pinned_typescript_source() {
    let mut oracle = oracle::TsOracle::spawn("tui-leaves-oracle.ts", SOURCE_DIRECTORY);
    let request: Vec<Value> = SEGMENT_CASES
        .iter()
        .map(|case| {
            json!({
                "line": case.line,
                "beforeEnd": case.before_end,
                "afterStart": case.after_start,
                "afterLen": case.after_len,
                "strictAfter": case.strict_after,
            })
        })
        .collect();
    let results = oracle_request(&mut oracle, "utils_extract_segments", "cases", json!(request));
    for (case, expected) in SEGMENT_CASES.iter().zip(results) {
        let actual = extract_segments(
            case.line,
            case.before_end,
            case.after_start,
            case.after_len,
            case.strict_after,
        );
        assert_eq!(
            actual.before,
            expected["before"].as_str().unwrap(),
            "before mismatch for {case:?}"
        );
        assert_eq!(actual.before_width, expected["beforeWidth"].as_u64().unwrap() as usize);
        assert_eq!(
            actual.after,
            expected["after"].as_str().unwrap(),
            "after mismatch for {case:?}"
        );
        assert_eq!(actual.after_width, expected["afterWidth"].as_u64().unwrap() as usize);
    }
}

#[test]
fn background_padding_matches_the_pinned_typescript_source() {
    let mut oracle = oracle::TsOracle::spawn("tui-leaves-oracle.ts", SOURCE_DIRECTORY);
    let cases = [
        ("abc", 5),
        ("abcde", 3),
        ("\x1b[31mab\x1b[0m", 4),
        ("网络", 5),
        ("", 3),
        ("a\tb", 6),
    ];
    let request: Vec<Value> = cases
        .iter()
        .map(|(line, width)| json!({ "line": line, "width": width }))
        .collect();
    let results = oracle_request(&mut oracle, "utils_background", "cases", json!(request));
    for ((line, width), expected) in cases.iter().zip(results) {
        assert_eq!(
            apply_background_to_line(*line, *width, |text| JsString::from("[") + text + "]"),
            expected.as_str().unwrap()
        );
    }
}

#[test]
fn character_classes_match_the_pinned_typescript_source() {
    let mut oracle = oracle::TsOracle::spawn("tui-leaves-oracle.ts", SOURCE_DIRECTORY);
    let cases = [
        " ",
        "\t",
        "\n",
        "\r",
        "\u{00a0}",
        "\u{1680}",
        "\u{2000}",
        "\u{2028}",
        "\u{2029}",
        "\u{202f}",
        "\u{205f}",
        "\u{3000}",
        "\u{feff}",
        "\u{0085}",
        "a",
        "a b",
        "",
        "(){}[]<>.,;:'\"!?+-=*/\\|&%^$#@~`",
        "a,b",
        "abc",
        "é",
        "界",
    ];
    let results = oracle_request(&mut oracle, "utils_char_classes", "cases", json!(cases));
    for (value, expected) in cases.iter().zip(results) {
        assert_eq!(
            is_whitespace_char(*value),
            expected["whitespace"].as_bool().unwrap(),
            "whitespace mismatch for {value:?}"
        );
        assert_eq!(
            is_punctuation_char(*value),
            expected["punctuation"].as_bool().unwrap(),
            "punctuation mismatch for {value:?}"
        );
    }
}
