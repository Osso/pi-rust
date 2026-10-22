use pi_coding_agent::utils::ansi::strip_ansi;
use pi_js_compat::js_json::JsString;

struct Case {
    name: &'static str,
    input: &'static str,
    expected: &'static str,
}

const CASES: [Case; 18] = [
    Case {
        name: "plain text is unchanged",
        input: "plain",
        expected: "plain",
    },
    Case {
        name: "SGR colors",
        input: "a\x1b[31mred\x1b[0mz",
        expected: "aredz",
    },
    Case {
        name: "OSC 8 hyperlink with BEL terminators",
        input: "a\x1b]8;;https://example.com\x07link\x1b]8;;\x07z",
        expected: "alinkz",
    },
    Case {
        name: "unterminated OSC is preserved",
        input: "a\x1b]unterminated",
        expected: "anterminated",
    },
    Case {
        name: "DCS is not stripped",
        input: "a\x1bPabc\x1b\\z",
        expected: "aabc\x1b\\z",
    },
    Case {
        name: "PM is not stripped",
        input: "a\x1b^abc\x07z",
        expected: "a\x1b^abc\x07z",
    },
    Case {
        name: "APC is not stripped",
        input: "a\x1b_abc\u{9c}z",
        expected: "a\x1b_abc\u{9c}z",
    },
    Case {
        name: "C1 DCS is not stripped",
        input: "a\u{90}abc\u{9c}z",
        expected: "a\u{90}abc\u{9c}z",
    },
    Case {
        name: "C1 OSC is not stripped",
        input: "a\u{9d}abc\u{9c}z",
        expected: "a\u{9d}abc\u{9c}z",
    },
    Case {
        name: "8-bit CSI",
        input: "a\u{9b}31mred",
        expected: "ared",
    },
    Case {
        name: "ESC intermediate and digit",
        input: "a\x1b(0x",
        expected: "ax",
    },
    Case {
        name: "ESC star is not a CSI",
        input: "a\x1b*0x",
        expected: "a\x1b*0x",
    },
    Case {
        name: "ESC plus is not a CSI",
        input: "a\x1b+c",
        expected: "a\x1b+c",
    },
    Case {
        name: "ESC slash is not a CSI",
        input: "a\x1b/0x",
        expected: "a\x1b/0x",
    },
    Case {
        name: "RIS is stripped with its final byte",
        input: "a\x1bcok",
        expected: "aok",
    },
    Case {
        name: "lone ST is preserved",
        input: "a\x1b\\ok",
        expected: "a\x1b\\ok",
    },
    Case {
        name: "C1 CSI with colon params",
        input: "x\u{9b}1:2mred",
        expected: "xred",
    },
    Case {
        name: "OSC terminated by ST",
        input: "x\x1b]0;title\x1b\\y",
        expected: "xy",
    },
];

#[test]
fn strips_every_source_case() {
    for case in &CASES {
        assert_eq!(strip_ansi(&JsString::from(case.input)), case.expected, "{}", case.name);
    }
}

#[test]
fn strips_each_single_byte_esc_final_from_g_through_m() {
    for code in 'g'..='m' {
        assert_eq!(strip_ansi(&JsString::from(format!("\x1b{code}ok"))), "ok");
    }
    for code in 'r'..='t' {
        assert_eq!(strip_ansi(&JsString::from(format!("\x1b{code}ok"))), "ok");
    }
}

#[test]
fn preserves_text_without_escape_introducers() {
    let input = "no escapes here \u{9c} but no introducer";
    assert_eq!(strip_ansi(&JsString::from(input)), input);
}
