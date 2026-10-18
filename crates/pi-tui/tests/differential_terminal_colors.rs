#[path = "support/oracle.rs"]
mod oracle;

use pi_tui::terminal_colors::{
    RgbColor, TerminalColorScheme, is_osc11_background_color_response, parse_osc11_background_color,
    parse_terminal_color_scheme_report,
};
use serde_json::{Value, json};

const SOURCE_DIRECTORY: &str = "packages/tui/src";

const CASES: &[&str] = &[
    "\x1b]11;rgb:0000/8000/ffff\x07",
    "\x1b]11;#ffffff\x1b\\",
    "\x1b]11;#000000\x07",
    "x\x1b]11;#ffffff\x07",
    "\x1b]11;#ffffff\x07x",
    "\x1b]10;#ffffff\x07",
    "\x1b]11;#FFFFFFFFFFFF\x07",
    "\x1b]11;#000000000000\x07",
    "\x1b]11;#fff\x07",
    "\x1b]11;#gggggg\x07",
    "\x1b]11;rgb:ff/80/00\x07",
    "\x1b]11;rgba:ff/80/00\x07",
    "\x1b]11;RGB:ffff/0000/8080\x07",
    "\x1b]11;  #ffffff  \x07",
    "\x1b]11;rgb:fff/80/00\x07",
    "\x1b]11;rgb:0/8/f\x07",
    "\x1b]11;rgb:000/800/fff\x07",
    "\x1b]11;rgb:ff/80\x07",
    "\x1b]11;rgb:ff/80/00/40\x07",
    "\x1b]11;rgb:/80/00\x07",
    "\x1b]11;rgb:gg/80/00\x07",
    "\x1b]11;not-a-color\x07",
    "\x1b]11;\x07",
    "\x1b]11;\x1b\\",
    "\x1b[?997;1n",
    "\x1b[?997;2n",
    "\x1b[?997;2n\x1b[?997;1n\x1b[?997;1n",
    "\x1b[?997;1n\x1b[?997;2n\x1b[?997;2n",
    "\x1b[?997;1n\x1b[?997;2n",
    "\x1b[?997;2n\x1b[?997;1n",
    "\x1b[?997;3n",
    "\x1b[?996n",
    "x\x1b[?997;1n",
    "",
];

fn rust_result(data: &str) -> Value {
    let rgb = match parse_osc11_background_color(data) {
        Some(color) => json!({ "r": color.r, "g": color.g, "b": color.b }),
        None => Value::Null,
    };
    json!({
        "rgb": rgb,
        "isResponse": is_osc11_background_color_response(data),
        "scheme": match parse_terminal_color_scheme_report(data) {
            Some(TerminalColorScheme::Dark) => Some("dark"),
            Some(TerminalColorScheme::Light) => Some("light"),
            None => None,
        },
    })
}

#[test]
fn terminal_colors_match_the_pinned_typescript_source() {
    let mut oracle = oracle::TsOracle::spawn("tui-leaves-oracle.ts", SOURCE_DIRECTORY);
    let response = oracle.request(&json!({
        "id": "terminal-colors",
        "operation": "terminal_colors",
        "cases": CASES,
    }));
    assert_eq!(response["ok"], true, "oracle failed: {response}");

    let results = response["output"]["results"].as_array().expect("results array");
    assert_eq!(results.len(), CASES.len());
    for (data, expected) in CASES.iter().zip(results) {
        assert_eq!(&rust_result(data), expected, "mismatch for {data:?}");
    }
}

#[test]
fn rgb_color_is_byte_sized() {
    let color = RgbColor { r: 0, g: 128, b: 255 };
    assert_eq!((color.r, color.g, color.b), (0, 128, 255));
}
