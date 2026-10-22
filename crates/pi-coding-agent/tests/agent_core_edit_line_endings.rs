use pi_js_compat::js_json::JsString;
use std::fmt::Debug;

use pi_coding_agent::core::tools::edit_primitives::{
    LineEnding, detect_line_ending, normalize_to_lf, restore_line_endings, strip_bom,
};

fn assert_debug<T: Debug>(_: &T) {}

#[test]
fn public_api_preserves_line_ending_literals_and_named_bom_fields() {
    assert_eq!(LineEnding::CrLf.as_str(), "\r\n");
    assert_eq!(LineEnding::Lf.as_str(), "\n");
    assert_eq!(LineEnding::CrLf, LineEnding::CrLf);
    assert_ne!(LineEnding::CrLf, LineEnding::Lf);

    let mut result = strip_bom("\u{feff}text");
    assert_eq!(result.bom, "\u{feff}");
    assert_eq!(result.text, "text");

    result.bom = JsString::from("changed");
    result.text = JsString::from("updated");
    assert_eq!(result.bom, "changed");
    assert_eq!(result.text, "updated");
    assert_debug(&result);
}

#[test]
fn detects_the_first_line_feed_and_ignores_lone_carriage_returns() {
    assert_eq!(detect_line_ending(""), LineEnding::Lf);
    assert_eq!(detect_line_ending("no newline"), LineEnding::Lf);
    assert_eq!(detect_line_ending("lone\rreturn"), LineEnding::Lf);
    assert_eq!(detect_line_ending("first\r\nsecond\n"), LineEnding::CrLf);
    assert_eq!(detect_line_ending("first\nsecond\r\n"), LineEnding::Lf);
    assert_eq!(detect_line_ending("\r\n\r\n"), LineEnding::CrLf);
    assert_eq!(detect_line_ending("\n\r\n"), LineEnding::Lf);
    assert_eq!(detect_line_ending("a\r\nb\nc\r\n"), LineEnding::CrLf);
}

#[test]
fn normalizes_crlf_then_remaining_carriage_returns_to_lf() {
    assert_eq!(normalize_to_lf(""), "");
    assert_eq!(normalize_to_lf("a\r\nb\rc\n"), "a\nb\nc\n");
    assert_eq!(normalize_to_lf("\r\r\n"), "\n\n");
    assert_eq!(normalize_to_lf("😀\r\ne\u{301}\r\0"), "😀\ne\u{301}\n\0");
    assert_eq!(normalize_to_lf("\u{feff}\r\ntext"), "\u{feff}\ntext");
}

#[test]
fn restores_lf_or_replaces_every_lf_for_crlf() {
    assert_eq!(restore_line_endings("a\nb", LineEnding::Lf), "a\nb");
    assert_eq!(restore_line_endings("a\nb\n", LineEnding::CrLf), "a\r\nb\r\n");
    assert_eq!(restore_line_endings("a\r\nb", LineEnding::CrLf), "a\r\r\nb");
    assert_eq!(restore_line_endings("\n\n", LineEnding::CrLf), "\r\n\r\n");
    assert_eq!(restore_line_endings("a\rb", LineEnding::CrLf), "a\rb");
}

#[test]
fn strips_only_one_leading_bom_and_preserves_other_boms() {
    assert_eq!(strip_bom("").bom, "");
    assert_eq!(strip_bom("plain").text, "plain");

    let one = strip_bom("\u{feff}text");
    assert_eq!(one.bom, "\u{feff}");
    assert_eq!(one.text, "text");

    let only = strip_bom("\u{feff}");
    assert_eq!(only.bom, "\u{feff}");
    assert_eq!(only.text, "");

    let multiple = strip_bom("\u{feff}\u{feff}text\u{feff}");
    assert_eq!(multiple.bom, "\u{feff}");
    assert_eq!(multiple.text, "\u{feff}text\u{feff}");

    let interior = strip_bom("a\u{feff}b");
    assert_eq!(interior.bom, "");
    assert_eq!(interior.text, "a\u{feff}b");
}

#[test]
fn preserves_unicode_controls_and_boms_across_helpers() {
    let input = "前😀e\u{301}\0\u{feff}\r\n後";
    assert_eq!(detect_line_ending(input), LineEnding::CrLf);
    assert_eq!(normalize_to_lf(input), "前😀e\u{301}\0\u{feff}\n後");
    assert_eq!(
        restore_line_endings("前😀e\u{301}\0\u{feff}\n後", LineEnding::CrLf),
        input
    );

    let result = strip_bom("\u{feff}前😀e\u{301}\0\u{feff}");
    assert_eq!(result.bom, "\u{feff}");
    assert_eq!(result.text, "前😀e\u{301}\0\u{feff}");
}
