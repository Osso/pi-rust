use crate::shared_oracle as oracle;

use pi_coding_agent::core::export_html::ansi_to_html::{ansi_lines_to_html, ansi_to_html};
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};

const HUGE: &str =
    "9999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999";
const ASTRONOMICAL: &str = "179769313486231570000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";

#[test]
fn coding_agent_ansi_to_html_matches_the_pinned_typescript_source() {
    let cases = cases();
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-ansi-to-html-oracle.ts",
        "packages/coding-agent/src/core/export-html/ansi-to-html.ts",
    );
    let expected = oracle.request(&json!({
        "id": 27_004,
        "operation": "coding_agent_ansi_to_html",
        "cases": cases.iter().map(|(name, text)| json!({"name": name, "text": text})).collect::<Vec<_>>(),
    }));
    let actual = json!({"id": 27_004, "ok": true, "output": cases.iter().map(|(name, text)| actual_case(name, text)).collect::<Vec<_>>()});

    assert_eq!(expected["ok"], Value::Bool(true), "{expected}");
    let expected_results = expected["output"].as_array().expect("oracle output array");
    let actual_results = actual["output"].as_array().expect("actual output array");
    assert_eq!(expected_results.len(), cases.len());
    for (index, (actual, expected)) in actual_results.iter().zip(expected_results).enumerate() {
        assert_eq!(actual, expected, "case {index} ({})", cases[index].0);
    }
}

fn actual_case(name: &str, text: &str) -> Value {
    json!({
        "name": name,
        "html": ansi_to_html(&JsString::from(text)),
        "linesHtml": ansi_lines_to_html(&text.split('\u{0}').map(str::to_owned).collect::<Vec<_>>().iter().map(JsString::from).collect::<Vec<_>>()),
    })
}

fn cases() -> Vec<(&'static str, String)> {
    let mut cases: Vec<(&'static str, String)> = vec![
        ("empty", String::new()),
        ("plain", "plain text".to_owned()),
        ("escape-text", "<a href=\"x\">&'</a>".to_owned()),
        ("red-reset", "\u{1b}[31mone\u{1b}[0m".to_owned()),
        (
            "styles",
            "\u{1b}[1;2;3;4mstyled\u{1b}[22m22\u{1b}[23m23\u{1b}[24m24".to_owned(),
        ),
        ("bright", "\u{1b}[90;107mx\u{1b}[39;49my".to_owned()),
        ("all-standard", "\u{1b}[30;47ma\u{1b}[37;40mb".to_owned()),
        ("empty-params", "\u{1b}[ma\u{1b}[;;mb".to_owned()),
        ("unknown", "\u{1b}[5;6;99mplain".to_owned()),
        ("256-low", "\u{1b}[38;5;0ma\u{1b}[38;5;15mb\u{1b}[48;5;16mc".to_owned()),
        (
            "256-cube",
            "\u{1b}[38;5;17ma\u{1b}[38;5;21mb\u{1b}[38;5;231mc".to_owned(),
        ),
        ("256-gray", "\u{1b}[38;5;232ma\u{1b}[38;5;255mb".to_owned()),
        ("256-truncated", "\u{1b}[38;5ma\u{1b}[48;5;b".to_owned()),
        ("rgb", "\u{1b}[38;2;0;127;255ma\u{1b}[48;2;1;2;3mb".to_owned()),
        ("rgb-truncated", "\u{1b}[38;2;1;2ma\u{1b}[38;2;1;2;3".to_owned()),
        ("no-terminator", "\u{1b}[31".to_owned()),
        ("non-sgr", "\u{1b}[2Ka\u{1b}[1Ab".to_owned()),
        ("multi-line", "one\u{0}\u{1b}[31mtwo\u{0}".to_owned()),
        ("unicode", "héllo 😀\u{1b}[32m✓".to_owned()),
    ];
    for index in [
        "16",
        "17",
        "21",
        "51",
        "196",
        "231",
        "232",
        "255",
        "256",
        "1000",
        "65535",
        "1000000000000000",
        "9007199254740993",
        "123456789012345678",
        HUGE,
        ASTRONOMICAL,
    ] {
        cases.push(("gray-or-cube", format!("\u{1b}[38;5;{index}mx")));
        cases.push(("bg-gray-or-cube", format!("\u{1b}[48;5;{index}mx")));
    }
    for index in [HUGE, ASTRONOMICAL] {
        cases.push(("rgb-huge", format!("\u{1b}[38;2;{index};{index};{index}mx")));
    }
    cases.push(("infinite-index", "\u{1b}[38;5;999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999mx".to_owned()));
    cases
}
