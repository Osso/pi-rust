use std::sync::Mutex;

use pi_tui::keys::{
    KeyEventType, decode_kitty_printable, decode_printable_key, is_key_release, is_key_repeat,
    is_kitty_protocol_active, matches_key, parse_key, set_kitty_protocol_active,
};

static KITTY_LOCK: Mutex<()> = Mutex::new(());

fn with_kitty(active: bool, test: impl FnOnce()) {
    let _guard = KITTY_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    set_kitty_protocol_active(active);
    test();
    set_kitty_protocol_active(false);
}

#[test]
fn kitty_protocol_state_round_trips() {
    with_kitty(true, || assert!(is_kitty_protocol_active()));
    assert!(!is_kitty_protocol_active());
}

#[test]
fn matches_cyrillic_ctrl_keys_through_base_layout() {
    with_kitty(true, || {
        assert!(matches_key("\x1b[1089::99;5u", "ctrl+c"));
        assert!(matches_key("\x1b[1074::100;5u", "ctrl+d"));
        assert!(matches_key("\x1b[1103::122;5u", "ctrl+z"));
        assert!(matches_key("\x1b[1079::112;6u", "ctrl+shift+p"));
        assert!(matches_key("\x1b[99;5u", "ctrl+c"));
        assert!(!matches_key("\x1b[1089::99;5u", "ctrl+d"));
        assert!(!matches_key("\x1b[1089::99;5u", "ctrl+shift+c"));
    });
}

#[test]
fn matches_super_modifiers_including_combined_modifiers() {
    with_kitty(true, || {
        assert!(matches_key("\x1b[107;9u", "super+k"));
        assert!(matches_key("\x1b[13;9u", "super+enter"));
        assert!(matches_key("\x1b[107;13u", "ctrl+super+k"));
        assert!(matches_key("\x1b[107;14u", "ctrl+shift+super+k"));
        assert!(!matches_key("\x1b[107;13u", "super+k"));
        assert_eq!(parse_key("\x1b[107;9u").as_deref(), Some("super+k"));
        assert_eq!(parse_key("\x1b[13;9u").as_deref(), Some("super+enter"));
        assert_eq!(parse_key("\x1b[107;13u").as_deref(), Some("ctrl+super+k"));
        assert_eq!(parse_key("\x1b[107;14u").as_deref(), Some("shift+ctrl+super+k"));
    });
}

#[test]
fn matches_digit_bindings_through_kitty_csi_u() {
    with_kitty(true, || {
        assert!(matches_key("\x1b[49u", "1"));
        assert!(matches_key("\x1b[49;5u", "ctrl+1"));
        assert!(!matches_key("\x1b[49;5u", "ctrl+2"));
        assert_eq!(parse_key("\x1b[49u").as_deref(), Some("1"));
        assert_eq!(parse_key("\x1b[49;5u").as_deref(), Some("ctrl+1"));
    });
}

#[test]
fn normalizes_kitty_keypad_functional_keys() {
    with_kitty(true, || {
        assert!(matches_key("\x1b[57400u", "1"));
        assert!(matches_key("\x1b[57410u", "/"));
        assert!(matches_key("\x1b[57417u", "left"));
        assert!(matches_key("\x1b[57426u", "delete"));
        assert_eq!(parse_key("\x1b[57399u").as_deref(), Some("0"));
        assert_eq!(parse_key("\x1b[57409u").as_deref(), Some("."));
        assert_eq!(parse_key("\x1b[57413u").as_deref(), Some("+"));
        assert_eq!(parse_key("\x1b[57416u").as_deref(), Some(","));
        assert_eq!(parse_key("\x1b[57417u").as_deref(), Some("left"));
        assert_eq!(parse_key("\x1b[57418u").as_deref(), Some("right"));
        assert_eq!(parse_key("\x1b[57419u").as_deref(), Some("up"));
        assert_eq!(parse_key("\x1b[57420u").as_deref(), Some("down"));
        assert_eq!(parse_key("\x1b[57421u").as_deref(), Some("pageUp"));
        assert_eq!(parse_key("\x1b[57422u").as_deref(), Some("pageDown"));
        assert_eq!(parse_key("\x1b[57423u").as_deref(), Some("home"));
        assert_eq!(parse_key("\x1b[57424u").as_deref(), Some("end"));
        assert_eq!(parse_key("\x1b[57425u").as_deref(), Some("insert"));
        assert_eq!(parse_key("\x1b[57426u").as_deref(), Some("delete"));
    });
}

#[test]
fn handles_shifted_keys_base_keys_and_event_types() {
    with_kitty(true, || {
        assert!(matches_key("\x1b[99:67:99;2u", "shift+c"));
        assert!(matches_key("\x1b[1089::99;5:3u", "ctrl+c"));
        assert!(matches_key("\x1b[1089:1057:99;6:2u", "ctrl+shift+c"));
    });
}

#[test]
fn prefers_codepoint_over_base_layout_for_latin_and_symbols() {
    with_kitty(true, || {
        assert!(matches_key("\x1b[107::118;5u", "ctrl+k"));
        assert!(!matches_key("\x1b[107::118;5u", "ctrl+v"));
        assert!(matches_key("\x1b[47::91;5u", "ctrl+/"));
        assert!(!matches_key("\x1b[47::91;5u", "ctrl+["));
        assert_eq!(parse_key("\x1b[107::118;5u").as_deref(), Some("ctrl+k"));
        assert_eq!(parse_key("\x1b[47::91;5u").as_deref(), Some("ctrl+/"));
    });
}

#[test]
fn matches_xterm_modify_other_keys() {
    with_kitty(false, || {
        assert!(matches_key("\x1b[27;5;99~", "ctrl+c"));
        assert_eq!(parse_key("\x1b[27;5;99~").as_deref(), Some("ctrl+c"));
        assert!(matches_key("\x1b[27;5;100~", "ctrl+d"));
        assert_eq!(parse_key("\x1b[27;5;100~").as_deref(), Some("ctrl+d"));
        assert!(matches_key("\x1b[27;5;122~", "ctrl+z"));
        assert_eq!(parse_key("\x1b[27;5;122~").as_deref(), Some("ctrl+z"));
        assert!(matches_key("\x1b[27;5;13~", "ctrl+enter"));
        assert!(matches_key("\x1b[27;2;13~", "shift+enter"));
        assert!(matches_key("\x1b[27;3;13~", "alt+enter"));
        assert_eq!(parse_key("\x1b[27;5;13~").as_deref(), Some("ctrl+enter"));
        assert_eq!(parse_key("\x1b[27;2;13~").as_deref(), Some("shift+enter"));
        assert_eq!(parse_key("\x1b[27;3;13~").as_deref(), Some("alt+enter"));
        assert!(matches_key("\x1b[27;2;9~", "shift+tab"));
        assert!(matches_key("\x1b[27;5;9~", "ctrl+tab"));
        assert!(matches_key("\x1b[27;3;9~", "alt+tab"));
        assert_eq!(parse_key("\x1b[27;2;9~").as_deref(), Some("shift+tab"));
        assert_eq!(parse_key("\x1b[27;5;9~").as_deref(), Some("ctrl+tab"));
        assert_eq!(parse_key("\x1b[27;3;9~").as_deref(), Some("alt+tab"));
        assert!(matches_key("\x1b[27;1;127~", "backspace"));
        assert!(matches_key("\x1b[27;5;127~", "ctrl+backspace"));
        assert!(matches_key("\x1b[27;3;127~", "alt+backspace"));
        assert_eq!(parse_key("\x1b[27;1;127~").as_deref(), Some("backspace"));
        assert_eq!(parse_key("\x1b[27;5;127~").as_deref(), Some("ctrl+backspace"));
        assert_eq!(parse_key("\x1b[27;3;127~").as_deref(), Some("alt+backspace"));
        assert!(matches_key("\x1b[27;1;27~", "escape"));
        assert_eq!(parse_key("\x1b[27;1;27~").as_deref(), Some("escape"));
        assert!(matches_key("\x1b[27;1;32~", "space"));
        assert!(matches_key("\x1b[27;5;32~", "ctrl+space"));
        assert_eq!(parse_key("\x1b[27;1;32~").as_deref(), Some("space"));
        assert_eq!(parse_key("\x1b[27;5;32~").as_deref(), Some("ctrl+space"));
        assert!(matches_key("\x1b[27;5;47~", "ctrl+/"));
        assert_eq!(parse_key("\x1b[27;5;47~").as_deref(), Some("ctrl+/"));
        assert!(matches_key("\x1b[27;5;49~", "ctrl+1"));
        assert!(matches_key("\x1b[27;2;49~", "shift+1"));
        assert_eq!(parse_key("\x1b[27;5;49~").as_deref(), Some("ctrl+1"));
        assert_eq!(parse_key("\x1b[27;2;49~").as_deref(), Some("shift+1"));
        assert!(matches_key("\x1b[27;2;69~", "shift+e"));
        assert!(matches_key("\x1b[27;6;69~", "ctrl+shift+e"));
        assert_eq!(parse_key("\x1b[27;2;69~").as_deref(), Some("shift+e"));
        assert_eq!(parse_key("\x1b[27;6;69~").as_deref(), Some("shift+ctrl+e"));
    });
}

#[test]
fn matches_ctrl_alt_letter_through_csi_u_and_modify_other_keys() {
    with_kitty(false, || {
        assert!(matches_key("\x1b[104;7u", "ctrl+alt+h"));
        assert_eq!(parse_key("\x1b[104;7u").as_deref(), Some("ctrl+alt+h"));
        assert!(matches_key("\x1b[27;7;104~", "ctrl+alt+h"));
        assert_eq!(parse_key("\x1b[27;7;104~").as_deref(), Some("ctrl+alt+h"));
    });
}

#[test]
fn matches_legacy_control_characters() {
    with_kitty(false, || {
        assert!(matches_key("\x03", "ctrl+c"));
        assert!(matches_key("\x04", "ctrl+d"));
        assert!(matches_key("\x1b", "escape"));
        assert!(matches_key("\n", "enter"));
        assert_eq!(parse_key("\n").as_deref(), Some("enter"));
        assert!(matches_key("\x00", "ctrl+space"));
        assert_eq!(parse_key("\x00").as_deref(), Some("ctrl+space"));
        assert!(matches_key("\x1c", "ctrl+\\"));
        assert_eq!(parse_key("\x1c").as_deref(), Some("ctrl+\\"));
        assert!(matches_key("\x1d", "ctrl+]"));
        assert_eq!(parse_key("\x1d").as_deref(), Some("ctrl+]"));
        assert!(matches_key("\x1f", "ctrl+_"));
        assert!(matches_key("\x1f", "ctrl+-"));
        assert_eq!(parse_key("\x1f").as_deref(), Some("ctrl+-"));
        assert!(matches_key("\x1b\x1b", "ctrl+alt+["));
        assert_eq!(parse_key("\x1b\x1b").as_deref(), Some("ctrl+alt+["));
        assert!(matches_key("\x1b\x1c", "ctrl+alt+\\"));
        assert_eq!(parse_key("\x1b\x1c").as_deref(), Some("ctrl+alt+\\"));
        assert!(matches_key("\x1b\x1d", "ctrl+alt+]"));
        assert_eq!(parse_key("\x1b\x1d").as_deref(), Some("ctrl+alt+]"));
        assert!(matches_key("\x1b\x1f", "ctrl+alt+_"));
        assert!(matches_key("\x1b\x1f", "ctrl+alt+-"));
        assert_eq!(parse_key("\x1b\x1f").as_deref(), Some("ctrl+alt+-"));
    });
}

#[test]
fn treats_linefeed_as_shift_enter_when_kitty_is_active() {
    with_kitty(true, || {
        assert!(matches_key("\n", "shift+enter"));
        assert!(!matches_key("\n", "enter"));
        assert_eq!(parse_key("\n").as_deref(), Some("shift+enter"));
    });
}

#[test]
fn raw_backspace_defaults_to_plain_backspace() {
    with_kitty(false, || {
        assert!(matches_key("\x7f", "backspace"));
        assert!(!matches_key("\x7f", "ctrl+backspace"));
        assert_eq!(parse_key("\x7f").as_deref(), Some("backspace"));
        assert!(matches_key("\x08", "backspace"));
        assert!(!matches_key("\x08", "ctrl+backspace"));
        assert_eq!(parse_key("\x08").as_deref(), Some("backspace"));
        assert!(matches_key("\x08", "ctrl+h"));
    });
}

#[test]
fn parses_legacy_alt_prefixed_sequences_when_kitty_is_inactive() {
    with_kitty(false, || {
        assert!(matches_key("\x1b ", "alt+space"));
        assert_eq!(parse_key("\x1b ").as_deref(), Some("alt+space"));
        assert!(matches_key("\x1b\x08", "alt+backspace"));
        assert_eq!(parse_key("\x1b\x08").as_deref(), Some("alt+backspace"));
        assert!(matches_key("\x1b\x03", "ctrl+alt+c"));
        assert_eq!(parse_key("\x1b\x03").as_deref(), Some("ctrl+alt+c"));
        assert!(matches_key("\x1bB", "alt+left"));
        assert_eq!(parse_key("\x1bB").as_deref(), Some("alt+left"));
        assert!(matches_key("\x1bF", "alt+right"));
        assert_eq!(parse_key("\x1bF").as_deref(), Some("alt+right"));
        assert!(matches_key("\x1ba", "alt+a"));
        assert_eq!(parse_key("\x1ba").as_deref(), Some("alt+a"));
        assert!(matches_key("\x1b1", "alt+1"));
        assert_eq!(parse_key("\x1b1").as_deref(), Some("alt+1"));
        assert!(matches_key("\x1by", "alt+y"));
        assert_eq!(parse_key("\x1by").as_deref(), Some("alt+y"));
        assert!(matches_key("\x1bz", "alt+z"));
        assert_eq!(parse_key("\x1bz").as_deref(), Some("alt+z"));
    });
}

#[test]
fn drops_ambiguous_legacy_sequences_when_kitty_is_active() {
    with_kitty(true, || {
        assert!(!matches_key("\x1b ", "alt+space"));
        assert_eq!(parse_key("\x1b "), None);
        assert!(matches_key("\x1b\x08", "alt+backspace"));
        assert_eq!(parse_key("\x1b\x08").as_deref(), Some("alt+backspace"));
        assert!(!matches_key("\x1b\x03", "ctrl+alt+c"));
        assert_eq!(parse_key("\x1b\x03"), None);
        assert!(!matches_key("\x1bB", "alt+left"));
        assert_eq!(parse_key("\x1bB"), None);
        assert!(!matches_key("\x1bF", "alt+right"));
        assert_eq!(parse_key("\x1bF"), None);
        assert!(!matches_key("\x1ba", "alt+a"));
        assert_eq!(parse_key("\x1ba"), None);
        assert!(matches_key("\x1b1", "alt+1"));
        assert_eq!(parse_key("\x1b1").as_deref(), Some("alt+1"));
        assert!(matches_key("\x1b2", "alt+2"));
        assert_eq!(parse_key("\x1b2").as_deref(), Some("alt+2"));
        assert!(!matches_key("\x1by", "alt+y"));
        assert_eq!(parse_key("\x1by"), None);
    });
}

#[test]
fn matches_arrow_keys_and_ss3_sequences() {
    with_kitty(false, || {
        assert!(matches_key("\x1b[A", "up"));
        assert!(matches_key("\x1b[B", "down"));
        assert!(matches_key("\x1b[C", "right"));
        assert!(matches_key("\x1b[D", "left"));
        assert!(matches_key("\x1bOA", "up"));
        assert!(matches_key("\x1bOB", "down"));
        assert!(matches_key("\x1bOC", "right"));
        assert!(matches_key("\x1bOD", "left"));
        assert!(matches_key("\x1bOH", "home"));
        assert!(matches_key("\x1bOF", "end"));
    });
}

#[test]
fn matches_xterm_ctrl_modified_viewport_navigation() {
    with_kitty(false, || {
        assert!(matches_key("\x1b[1;5H", "ctrl+home"));
        assert!(matches_key("\x1b[1;5F", "ctrl+end"));
        assert!(matches_key("\x1b[5;5~", "ctrl+pageUp"));
        assert!(matches_key("\x1b[6;5~", "ctrl+pageDown"));
        assert_eq!(parse_key("\x1b[1;5H").as_deref(), Some("ctrl+home"));
        assert_eq!(parse_key("\x1b[1;5F").as_deref(), Some("ctrl+end"));
        assert_eq!(parse_key("\x1b[5;5~").as_deref(), Some("ctrl+pageUp"));
        assert_eq!(parse_key("\x1b[6;5~").as_deref(), Some("ctrl+pageDown"));
    });
}

#[test]
fn matches_legacy_function_keys_clear_and_alt_arrows() {
    with_kitty(false, || {
        assert!(matches_key("\x1bOP", "f1"));
        assert!(matches_key("\x1b[24~", "f12"));
        assert!(matches_key("\x1b[E", "clear"));
        assert!(matches_key("\x1bp", "alt+up"));
        assert!(!matches_key("\x1bp", "up"));
        assert!(matches_key("\x1b[a", "shift+up"));
        assert!(matches_key("\x1bOa", "ctrl+up"));
        assert!(matches_key("\x1b[2$", "shift+insert"));
        assert!(matches_key("\x1b[2^", "ctrl+insert"));
        assert!(matches_key("\x1b[7$", "shift+home"));
    });
}

#[test]
fn parses_legacy_sequences() {
    with_kitty(false, || {
        assert_eq!(parse_key("\x03").as_deref(), Some("ctrl+c"));
        assert_eq!(parse_key("\x04").as_deref(), Some("ctrl+d"));
        assert_eq!(parse_key("\x1b").as_deref(), Some("escape"));
        assert_eq!(parse_key("\t").as_deref(), Some("tab"));
        assert_eq!(parse_key("\r").as_deref(), Some("enter"));
        assert_eq!(parse_key("\n").as_deref(), Some("enter"));
        assert_eq!(parse_key("\x00").as_deref(), Some("ctrl+space"));
        assert_eq!(parse_key(" ").as_deref(), Some("space"));
        assert_eq!(parse_key("1").as_deref(), Some("1"));
        assert!(matches_key("1", "1"));
        assert_eq!(parse_key("\x1b[A").as_deref(), Some("up"));
        assert_eq!(parse_key("\x1b[B").as_deref(), Some("down"));
        assert_eq!(parse_key("\x1b[C").as_deref(), Some("right"));
        assert_eq!(parse_key("\x1b[D").as_deref(), Some("left"));
        assert_eq!(parse_key("\x1bOA").as_deref(), Some("up"));
        assert_eq!(parse_key("\x1bOB").as_deref(), Some("down"));
        assert_eq!(parse_key("\x1bOC").as_deref(), Some("right"));
        assert_eq!(parse_key("\x1bOD").as_deref(), Some("left"));
        assert_eq!(parse_key("\x1bOH").as_deref(), Some("home"));
        assert_eq!(parse_key("\x1bOF").as_deref(), Some("end"));
        assert_eq!(parse_key("\x1bOP").as_deref(), Some("f1"));
        assert_eq!(parse_key("\x1b[24~").as_deref(), Some("f12"));
        assert_eq!(parse_key("\x1b[E").as_deref(), Some("clear"));
        assert_eq!(parse_key("\x1b[2^").as_deref(), Some("ctrl+insert"));
        assert_eq!(parse_key("\x1bp").as_deref(), Some("alt+up"));
        assert_eq!(parse_key("\x1b[[5~").as_deref(), Some("pageUp"));
    });
}

#[test]
fn decodes_kitty_printable_keypad_keys() {
    assert_eq!(decode_kitty_printable("\x1b[57399u").as_deref(), Some("0"));
    assert_eq!(decode_kitty_printable("\x1b[57400u").as_deref(), Some("1"));
    assert_eq!(decode_kitty_printable("\x1b[57409u").as_deref(), Some("."));
    assert_eq!(decode_kitty_printable("\x1b[57410u").as_deref(), Some("/"));
    assert_eq!(decode_kitty_printable("\x1b[57411u").as_deref(), Some("*"));
    assert_eq!(decode_kitty_printable("\x1b[57412u").as_deref(), Some("-"));
    assert_eq!(decode_kitty_printable("\x1b[57413u").as_deref(), Some("+"));
    assert_eq!(decode_kitty_printable("\x1b[57415u").as_deref(), Some("="));
    assert_eq!(decode_kitty_printable("\x1b[57416u").as_deref(), Some(","));
    assert_eq!(decode_kitty_printable("\x1b[57417u"), None);
}

#[test]
fn decodes_printable_modify_other_keys() {
    assert_eq!(decode_printable_key("\x1b[27;2;69~").as_deref(), Some("E"));
    assert_eq!(decode_printable_key("\x1b[27;2;196~").as_deref(), Some("Ä"));
    assert_eq!(decode_printable_key("\x1b[27;2;32~").as_deref(), Some(" "));
    assert_eq!(decode_printable_key("\x1b[27;2;13~"), None);
    assert_eq!(decode_printable_key("\x1b[27;6;69~"), None);
}

#[test]
fn parses_shifted_uppercase_csi_u_letters() {
    with_kitty(true, || {
        assert!(matches_key("\x1b[69;2u", "shift+e"));
        assert_eq!(parse_key("\x1b[69;2u").as_deref(), Some("shift+e"));
        assert_eq!(parse_key("\x1b[99;17u"), None);
    });
}

#[test]
fn detects_key_release_and_repeat_events() {
    assert!(is_key_release("\x1b[97;1:3u"));
    assert!(is_key_release("\x1b[3;1:3~"));
    assert!(is_key_release("\x1b[1;1:3A"));
    assert!(!is_key_release("\x1b[97;1:1u"));
    assert!(!is_key_release("\x1b[200~90:62:3F:A5\x1b[201~"));

    assert!(is_key_repeat("\x1b[97;1:2u"));
    assert!(is_key_repeat("\x1b[3;1:2~"));
    assert!(!is_key_repeat("\x1b[97;1:1u"));
    assert!(!is_key_repeat("\x1b[200~90:62:2F:A5\x1b[201~"));
}

#[test]
fn key_event_type_is_exported() {
    assert_eq!(KeyEventType::Press, KeyEventType::Press);
    assert_ne!(KeyEventType::Press, KeyEventType::Release);
}
