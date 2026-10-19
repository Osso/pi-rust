//! Keyboard input parsing and matching for legacy terminals and the Kitty
//! keyboard protocol.
//!
//! Ported from `packages/tui/src/keys.ts`.

use std::sync::atomic::{AtomicBool, Ordering};

use pi_js_compat::string::utf16_len;

/// Event types from the Kitty keyboard protocol (flag 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEventType {
    Press,
    Repeat,
    Release,
}

const MOD_SHIFT: i64 = 1;
const MOD_ALT: i64 = 2;
const MOD_CTRL: i64 = 4;
const MOD_SUPER: i64 = 8;
const LOCK_MASK: i64 = 64 + 128;

const CP_ESCAPE: i64 = 27;
const CP_TAB: i64 = 9;
const CP_ENTER: i64 = 13;
const CP_SPACE: i64 = 32;
const CP_BACKSPACE: i64 = 127;
const CP_KP_ENTER: i64 = 57414;

const CP_UP: i64 = -1;
const CP_DOWN: i64 = -2;
const CP_RIGHT: i64 = -3;
const CP_LEFT: i64 = -4;

const CP_DELETE: i64 = -10;
const CP_INSERT: i64 = -11;
const CP_PAGE_UP: i64 = -12;
const CP_PAGE_DOWN: i64 = -13;
const CP_HOME: i64 = -14;
const CP_END: i64 = -15;

const SYMBOL_KEYS: &str = "`-=[]\\;',./!@#$%^&*()_+|~{}:<>?";

static KITTY_PROTOCOL_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Set the global Kitty keyboard protocol state.
pub fn set_kitty_protocol_active(active: bool) {
    KITTY_PROTOCOL_ACTIVE.store(active, Ordering::Relaxed);
}

/// Query whether the Kitty keyboard protocol is currently active.
pub fn is_kitty_protocol_active() -> bool {
    KITTY_PROTOCOL_ACTIVE.load(Ordering::Relaxed)
}

#[derive(Debug, Clone, Copy)]
struct ParsedKittySequence {
    codepoint: i64,
    shifted_key: Option<u32>,
    base_layout_key: Option<u32>,
    modifier: i64,
}

#[derive(Debug, Clone, Copy)]
struct ParsedModifyOtherKeysSequence {
    codepoint: i64,
    modifier: i64,
}

struct ParsedKeyId {
    key: String,
    ctrl: bool,
    shift: bool,
    alt: bool,
    super_modifier: bool,
}

fn normalize_kitty_functional_codepoint(codepoint: i64) -> i64 {
    match codepoint {
        57399 => 48,
        57400 => 49,
        57401 => 50,
        57402 => 51,
        57403 => 52,
        57404 => 53,
        57405 => 54,
        57406 => 55,
        57407 => 56,
        57408 => 57,
        57409 => 46,
        57410 => 47,
        57411 => 42,
        57412 => 45,
        57413 => 43,
        57415 => 61,
        57416 => 44,
        57417 => CP_LEFT,
        57418 => CP_RIGHT,
        57419 => CP_UP,
        57420 => CP_DOWN,
        57421 => CP_PAGE_UP,
        57422 => CP_PAGE_DOWN,
        57423 => CP_HOME,
        57424 => CP_END,
        57425 => CP_INSERT,
        57426 => CP_DELETE,
        _ => codepoint,
    }
}

fn normalize_shifted_letter_identity_codepoint(codepoint: i64, modifier: i64) -> i64 {
    let effective_modifier = modifier & !LOCK_MASK;
    if effective_modifier & MOD_SHIFT != 0 && (65..=90).contains(&codepoint) {
        codepoint + 32
    } else {
        codepoint
    }
}

fn strip_ascii_suffix<'a>(data: &'a str, prefix: &str, suffixes: &[u8]) -> Option<&'a str> {
    let rest = data.strip_prefix(prefix)?;
    let last = rest.as_bytes().last()?;
    if suffixes.contains(last) {
        Some(&rest[..rest.len() - 1])
    } else {
        None
    }
}

fn parse_digits(bytes: &[u8], index: &mut usize) -> Option<u32> {
    let start = *index;
    while *index < bytes.len() && bytes[*index].is_ascii_digit() {
        *index += 1;
    }
    if *index == start {
        return None;
    }
    std::str::from_utf8(&bytes[start..*index]).ok()?.parse().ok()
}

fn parse_kitty_csi_u(data: &str) -> Option<ParsedKittySequence> {
    let bytes = data.as_bytes();
    if !data.starts_with("\x1b[") || !data.ends_with('u') {
        return None;
    }
    let mut index = 2;
    let codepoint = i64::from(parse_digits(bytes, &mut index)?);

    let mut shifted_key = None;
    let mut base_layout_key = None;
    if bytes.get(index) == Some(&b':') {
        index += 1;
        let shifted_start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        if index > shifted_start {
            shifted_key = Some(std::str::from_utf8(&bytes[shifted_start..index]).ok()?.parse().ok()?);
        }
        if bytes.get(index) == Some(&b':') {
            index += 1;
            base_layout_key = Some(parse_digits(bytes, &mut index)?);
        }
    }

    let mut mod_value = 1;
    if bytes.get(index) == Some(&b';') {
        index += 1;
        mod_value = i64::from(parse_digits(bytes, &mut index)?);
    }

    if bytes.get(index) == Some(&b':') {
        index += 1;
        parse_digits(bytes, &mut index)?;
    }

    if index != bytes.len() - 1 {
        return None;
    }

    Some(ParsedKittySequence {
        codepoint,
        shifted_key,
        base_layout_key,
        modifier: mod_value - 1,
    })
}

fn split_modifier_and_event(body: &str) -> Option<(i64, Option<u32>)> {
    match body.split_once(':') {
        Some((modifier, event)) => Some((modifier.parse().ok()?, Some(event.parse().ok()?))),
        None => Some((body.parse().ok()?, None)),
    }
}

fn parse_kitty_sequence(data: &str) -> Option<ParsedKittySequence> {
    if let Some(parsed) = parse_kitty_csi_u(data) {
        return Some(parsed);
    }

    if let Some(body) = strip_ascii_suffix(data, "\x1b[1;", b"ABCD") {
        let (mod_value, _event) = split_modifier_and_event(body)?;
        let direction = data.as_bytes()[data.len() - 1];
        let codepoint = match direction {
            b'A' => CP_UP,
            b'B' => CP_DOWN,
            b'C' => CP_RIGHT,
            _ => CP_LEFT,
        };
        return Some(ParsedKittySequence {
            codepoint,
            shifted_key: None,
            base_layout_key: None,
            modifier: mod_value - 1,
        });
    }

    if let Some(body) = strip_ascii_suffix(data, "\x1b[", b"~") {
        let (key_number, rest) = match body.split_once(';') {
            Some((key_number, rest)) => (key_number.parse::<u32>().ok()?, Some(rest)),
            None => (body.parse::<u32>().ok()?, None),
        };
        let (mod_value, _event) = match rest {
            Some(rest) => split_modifier_and_event(rest)?,
            None => (1, None),
        };
        let codepoint = match key_number {
            2 => Some(CP_INSERT),
            3 => Some(CP_DELETE),
            5 => Some(CP_PAGE_UP),
            6 => Some(CP_PAGE_DOWN),
            7 => Some(CP_HOME),
            8 => Some(CP_END),
            _ => None,
        };
        if let Some(codepoint) = codepoint {
            return Some(ParsedKittySequence {
                codepoint,
                shifted_key: None,
                base_layout_key: None,
                modifier: mod_value - 1,
            });
        }
    }

    if let Some(body) = strip_ascii_suffix(data, "\x1b[1;", b"HF") {
        let (mod_value, _event) = split_modifier_and_event(body)?;
        let codepoint = if data.ends_with('H') { CP_HOME } else { CP_END };
        return Some(ParsedKittySequence {
            codepoint,
            shifted_key: None,
            base_layout_key: None,
            modifier: mod_value - 1,
        });
    }

    None
}

fn parse_modify_other_keys_sequence(data: &str) -> Option<ParsedModifyOtherKeysSequence> {
    let body = data.strip_prefix("\x1b[27;")?.strip_suffix('~')?;
    let (modifier, codepoint) = body.split_once(';')?;
    Some(ParsedModifyOtherKeysSequence {
        codepoint: codepoint.parse().ok()?,
        modifier: i64::from(modifier.parse::<u32>().ok()?) - 1,
    })
}

fn char_from_utf16_code(code: i64) -> Option<char> {
    char::from_u32(u32::try_from(code & 0xffff).ok()?)
}

fn is_symbol_codepoint(codepoint: i64) -> bool {
    char_from_utf16_code(codepoint).is_some_and(|character| SYMBOL_KEYS.contains(character))
}

fn matches_kitty_sequence(data: &str, expected_codepoint: i64, expected_modifier: i64) -> bool {
    let Some(parsed) = parse_kitty_sequence(data) else {
        return false;
    };

    if (parsed.modifier & !LOCK_MASK) != (expected_modifier & !LOCK_MASK) {
        return false;
    }

    let normalized_codepoint = normalize_shifted_letter_identity_codepoint(
        normalize_kitty_functional_codepoint(parsed.codepoint),
        parsed.modifier,
    );
    let normalized_expected = normalize_shifted_letter_identity_codepoint(
        normalize_kitty_functional_codepoint(expected_codepoint),
        expected_modifier,
    );
    if normalized_codepoint == normalized_expected {
        return true;
    }

    if parsed
        .base_layout_key
        .is_some_and(|key| i64::from(key) == expected_codepoint)
    {
        let is_latin_letter = (97..=122).contains(&normalized_codepoint);
        let is_known_symbol = is_symbol_codepoint(normalized_codepoint);
        if !is_latin_letter && !is_known_symbol {
            return true;
        }
    }

    false
}

fn matches_modify_other_keys(data: &str, expected_keycode: i64, expected_modifier: i64) -> bool {
    parse_modify_other_keys_sequence(data)
        .is_some_and(|parsed| parsed.codepoint == expected_keycode && parsed.modifier == expected_modifier)
}

fn is_windows_terminal_session() -> bool {
    windows_terminal_session(
        environment_flag("WT_SESSION"),
        environment_flag("SSH_CONNECTION"),
        environment_flag("SSH_CLIENT"),
        environment_flag("SSH_TTY"),
    )
}

fn environment_flag(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|value| !value.is_empty())
}

fn windows_terminal_session(wt_session: bool, ssh_connection: bool, ssh_client: bool, ssh_tty: bool) -> bool {
    wt_session && !ssh_connection && !ssh_client && !ssh_tty
}

fn matches_raw_backspace(data: &str, expected_modifier: i64) -> bool {
    if data == "\x7f" {
        return expected_modifier == 0;
    }
    if data != "\x08" {
        return false;
    }
    if is_windows_terminal_session() {
        expected_modifier == MOD_CTRL
    } else {
        expected_modifier == 0
    }
}

fn raw_ctrl_char(key: &str) -> Option<char> {
    let character = key.to_lowercase().chars().next()?;
    let code = character as u32;
    if (97..=122).contains(&code) || matches!(character, '[' | '\\' | ']' | '_') {
        return char::from_u32(code & 0x1f);
    }
    if character == '-' {
        return char::from_u32(31);
    }
    None
}

fn is_digit_key(key: &str) -> bool {
    key.len() == 1 && key.as_bytes()[0].is_ascii_digit()
}

fn is_letter_key(key: &str) -> bool {
    key.len() == 1 && key.as_bytes()[0].is_ascii_lowercase()
}

fn matches_printable_modify_other_keys(data: &str, expected_keycode: i64, expected_modifier: i64) -> bool {
    if expected_modifier == 0 {
        return false;
    }
    let Some(parsed) = parse_modify_other_keys_sequence(data) else {
        return false;
    };
    if parsed.modifier != expected_modifier {
        return false;
    }
    normalize_shifted_letter_identity_codepoint(parsed.codepoint, parsed.modifier)
        == normalize_shifted_letter_identity_codepoint(expected_keycode, expected_modifier)
}

fn format_key_name_with_modifiers(key_name: &str, modifier: i64) -> Option<String> {
    let effective_modifier = modifier & !LOCK_MASK;
    let supported_modifier_mask = MOD_SHIFT | MOD_CTRL | MOD_ALT | MOD_SUPER;
    if effective_modifier & !supported_modifier_mask != 0 {
        return None;
    }

    let mut modifiers = Vec::new();
    if effective_modifier & MOD_SHIFT != 0 {
        modifiers.push("shift");
    }
    if effective_modifier & MOD_CTRL != 0 {
        modifiers.push("ctrl");
    }
    if effective_modifier & MOD_ALT != 0 {
        modifiers.push("alt");
    }
    if effective_modifier & MOD_SUPER != 0 {
        modifiers.push("super");
    }

    if modifiers.is_empty() {
        Some(key_name.to_string())
    } else {
        Some(format!("{}+{key_name}", modifiers.join("+")))
    }
}

fn parse_key_id(key_id: &str) -> Option<ParsedKeyId> {
    let lowercase = key_id.to_lowercase();
    let parts: Vec<&str> = lowercase.split('+').collect();
    let key = parts.last()?;
    if key.is_empty() {
        return None;
    }
    Some(ParsedKeyId {
        key: (*key).to_string(),
        ctrl: parts.contains(&"ctrl"),
        shift: parts.contains(&"shift"),
        alt: parts.contains(&"alt"),
        super_modifier: parts.contains(&"super"),
    })
}

const LEGACY_UP: &[&str] = &["\x1b[A", "\x1bOA"];
const LEGACY_DOWN: &[&str] = &["\x1b[B", "\x1bOB"];
const LEGACY_RIGHT: &[&str] = &["\x1b[C", "\x1bOC"];
const LEGACY_LEFT: &[&str] = &["\x1b[D", "\x1bOD"];
const LEGACY_HOME: &[&str] = &["\x1b[H", "\x1bOH", "\x1b[1~", "\x1b[7~"];
const LEGACY_END: &[&str] = &["\x1b[F", "\x1bOF", "\x1b[4~", "\x1b[8~"];
const LEGACY_INSERT: &[&str] = &["\x1b[2~"];
const LEGACY_DELETE: &[&str] = &["\x1b[3~"];
const LEGACY_PAGE_UP: &[&str] = &["\x1b[5~", "\x1b[[5~"];
const LEGACY_PAGE_DOWN: &[&str] = &["\x1b[6~", "\x1b[[6~"];
const LEGACY_CLEAR: &[&str] = &["\x1b[E", "\x1bOE"];
const LEGACY_F1: &[&str] = &["\x1bOP", "\x1b[11~", "\x1b[[A"];
const LEGACY_F2: &[&str] = &["\x1bOQ", "\x1b[12~", "\x1b[[B"];
const LEGACY_F3: &[&str] = &["\x1bOR", "\x1b[13~", "\x1b[[C"];
const LEGACY_F4: &[&str] = &["\x1bOS", "\x1b[14~", "\x1b[[D"];
const LEGACY_F5: &[&str] = &["\x1b[15~", "\x1b[[E"];
const LEGACY_F6: &[&str] = &["\x1b[17~"];
const LEGACY_F7: &[&str] = &["\x1b[18~"];
const LEGACY_F8: &[&str] = &["\x1b[19~"];
const LEGACY_F9: &[&str] = &["\x1b[20~"];
const LEGACY_F10: &[&str] = &["\x1b[21~"];
const LEGACY_F11: &[&str] = &["\x1b[23~"];
const LEGACY_F12: &[&str] = &["\x1b[24~"];

const LEGACY_SHIFT_UP: &[&str] = &["\x1b[a"];
const LEGACY_SHIFT_DOWN: &[&str] = &["\x1b[b"];
const LEGACY_SHIFT_RIGHT: &[&str] = &["\x1b[c"];
const LEGACY_SHIFT_LEFT: &[&str] = &["\x1b[d"];
const LEGACY_SHIFT_CLEAR: &[&str] = &["\x1b[e"];
const LEGACY_SHIFT_INSERT: &[&str] = &["\x1b[2$"];
const LEGACY_SHIFT_DELETE: &[&str] = &["\x1b[3$"];
const LEGACY_SHIFT_PAGE_UP: &[&str] = &["\x1b[5$"];
const LEGACY_SHIFT_PAGE_DOWN: &[&str] = &["\x1b[6$"];
const LEGACY_SHIFT_HOME: &[&str] = &["\x1b[7$"];
const LEGACY_SHIFT_END: &[&str] = &["\x1b[8$"];

const LEGACY_CTRL_UP: &[&str] = &["\x1bOa"];
const LEGACY_CTRL_DOWN: &[&str] = &["\x1bOb"];
const LEGACY_CTRL_RIGHT: &[&str] = &["\x1bOc"];
const LEGACY_CTRL_LEFT: &[&str] = &["\x1bOd"];
const LEGACY_CTRL_CLEAR: &[&str] = &["\x1bOe"];
const LEGACY_CTRL_INSERT: &[&str] = &["\x1b[2^"];
const LEGACY_CTRL_DELETE: &[&str] = &["\x1b[3^"];
const LEGACY_CTRL_PAGE_UP: &[&str] = &["\x1b[5^"];
const LEGACY_CTRL_PAGE_DOWN: &[&str] = &["\x1b[6^"];
const LEGACY_CTRL_HOME: &[&str] = &["\x1b[7^"];
const LEGACY_CTRL_END: &[&str] = &["\x1b[8^"];

fn matches_legacy_sequence(data: &str, sequences: &[&str]) -> bool {
    sequences.contains(&data)
}

#[derive(Clone, Copy)]
enum LegacyModifierKey {
    Up,
    Down,
    Right,
    Left,
    Clear,
    Insert,
    Delete,
    PageUp,
    PageDown,
    Home,
    End,
}

fn matches_legacy_modifier_sequence(data: &str, key: LegacyModifierKey, modifier: i64) -> bool {
    let sequences = match (modifier, key) {
        (MOD_SHIFT, LegacyModifierKey::Up) => LEGACY_SHIFT_UP,
        (MOD_SHIFT, LegacyModifierKey::Down) => LEGACY_SHIFT_DOWN,
        (MOD_SHIFT, LegacyModifierKey::Right) => LEGACY_SHIFT_RIGHT,
        (MOD_SHIFT, LegacyModifierKey::Left) => LEGACY_SHIFT_LEFT,
        (MOD_SHIFT, LegacyModifierKey::Clear) => LEGACY_SHIFT_CLEAR,
        (MOD_SHIFT, LegacyModifierKey::Insert) => LEGACY_SHIFT_INSERT,
        (MOD_SHIFT, LegacyModifierKey::Delete) => LEGACY_SHIFT_DELETE,
        (MOD_SHIFT, LegacyModifierKey::PageUp) => LEGACY_SHIFT_PAGE_UP,
        (MOD_SHIFT, LegacyModifierKey::PageDown) => LEGACY_SHIFT_PAGE_DOWN,
        (MOD_SHIFT, LegacyModifierKey::Home) => LEGACY_SHIFT_HOME,
        (MOD_SHIFT, LegacyModifierKey::End) => LEGACY_SHIFT_END,
        (MOD_CTRL, LegacyModifierKey::Up) => LEGACY_CTRL_UP,
        (MOD_CTRL, LegacyModifierKey::Down) => LEGACY_CTRL_DOWN,
        (MOD_CTRL, LegacyModifierKey::Right) => LEGACY_CTRL_RIGHT,
        (MOD_CTRL, LegacyModifierKey::Left) => LEGACY_CTRL_LEFT,
        (MOD_CTRL, LegacyModifierKey::Clear) => LEGACY_CTRL_CLEAR,
        (MOD_CTRL, LegacyModifierKey::Insert) => LEGACY_CTRL_INSERT,
        (MOD_CTRL, LegacyModifierKey::Delete) => LEGACY_CTRL_DELETE,
        (MOD_CTRL, LegacyModifierKey::PageUp) => LEGACY_CTRL_PAGE_UP,
        (MOD_CTRL, LegacyModifierKey::PageDown) => LEGACY_CTRL_PAGE_DOWN,
        (MOD_CTRL, LegacyModifierKey::Home) => LEGACY_CTRL_HOME,
        (MOD_CTRL, LegacyModifierKey::End) => LEGACY_CTRL_END,
        _ => return false,
    };
    matches_legacy_sequence(data, sequences)
}

fn legacy_sequence_key_id(data: &str) -> Option<&'static str> {
    Some(match data {
        "\x1bOA" => "up",
        "\x1bOB" => "down",
        "\x1bOC" => "right",
        "\x1bOD" => "left",
        "\x1bOH" => "home",
        "\x1bOF" => "end",
        "\x1b[E" | "\x1bOE" => "clear",
        "\x1bOe" => "ctrl+clear",
        "\x1b[e" => "shift+clear",
        "\x1b[2~" => "insert",
        "\x1b[2$" => "shift+insert",
        "\x1b[2^" => "ctrl+insert",
        "\x1b[3$" => "shift+delete",
        "\x1b[3^" => "ctrl+delete",
        "\x1b[[5~" => "pageUp",
        "\x1b[[6~" => "pageDown",
        "\x1b[a" => "shift+up",
        "\x1b[b" => "shift+down",
        "\x1b[c" => "shift+right",
        "\x1b[d" => "shift+left",
        "\x1bOa" => "ctrl+up",
        "\x1bOb" => "ctrl+down",
        "\x1bOc" => "ctrl+right",
        "\x1bOd" => "ctrl+left",
        "\x1b[5$" => "shift+pageUp",
        "\x1b[6$" => "shift+pageDown",
        "\x1b[7$" => "shift+home",
        "\x1b[8$" => "shift+end",
        "\x1b[5^" => "ctrl+pageUp",
        "\x1b[6^" => "ctrl+pageDown",
        "\x1b[7^" => "ctrl+home",
        "\x1b[8^" => "ctrl+end",
        "\x1bOP" => "f1",
        "\x1bOQ" => "f2",
        "\x1bOR" => "f3",
        "\x1bOS" => "f4",
        "\x1b[11~" => "f1",
        "\x1b[12~" => "f2",
        "\x1b[13~" => "f3",
        "\x1b[14~" => "f4",
        "\x1b[[A" => "f1",
        "\x1b[[B" => "f2",
        "\x1b[[C" => "f3",
        "\x1b[[D" => "f4",
        "\x1b[[E" => "f5",
        "\x1b[15~" => "f5",
        "\x1b[17~" => "f6",
        "\x1b[18~" => "f7",
        "\x1b[19~" => "f8",
        "\x1b[20~" => "f9",
        "\x1b[21~" => "f10",
        "\x1b[23~" => "f11",
        "\x1b[24~" => "f12",
        "\x1bb" => "alt+left",
        "\x1bf" => "alt+right",
        "\x1bp" => "alt+up",
        "\x1bn" => "alt+down",
        _ => return None,
    })
}

/// Match input data against a key identifier string.
pub fn matches_key(data: &str, key_id: &str) -> bool {
    let Some(parsed) = parse_key_id(key_id) else {
        return false;
    };

    let mut modifier = 0;
    if parsed.shift {
        modifier |= MOD_SHIFT;
    }
    if parsed.alt {
        modifier |= MOD_ALT;
    }
    if parsed.ctrl {
        modifier |= MOD_CTRL;
    }
    if parsed.super_modifier {
        modifier |= MOD_SUPER;
    }

    match parsed.key.as_str() {
        "escape" | "esc" => {
            if modifier != 0 {
                return false;
            }
            data == "\x1b"
                || matches_kitty_sequence(data, CP_ESCAPE, 0)
                || matches_modify_other_keys(data, CP_ESCAPE, 0)
        }
        "space" => {
            if !is_kitty_protocol_active() {
                if modifier == MOD_CTRL && data == "\x00" {
                    return true;
                }
                if modifier == MOD_ALT && data == "\x1b " {
                    return true;
                }
            }
            if modifier == 0 {
                return data == " "
                    || matches_kitty_sequence(data, CP_SPACE, 0)
                    || matches_modify_other_keys(data, CP_SPACE, 0);
            }
            matches_kitty_sequence(data, CP_SPACE, modifier) || matches_modify_other_keys(data, CP_SPACE, modifier)
        }
        "tab" => {
            if modifier == MOD_SHIFT {
                return data == "\x1b[Z"
                    || matches_kitty_sequence(data, CP_TAB, MOD_SHIFT)
                    || matches_modify_other_keys(data, CP_TAB, MOD_SHIFT);
            }
            if modifier == 0 {
                return data == "\t" || matches_kitty_sequence(data, CP_TAB, 0);
            }
            matches_kitty_sequence(data, CP_TAB, modifier) || matches_modify_other_keys(data, CP_TAB, modifier)
        }
        "enter" | "return" => match modifier {
            MOD_SHIFT => {
                if matches_kitty_sequence(data, CP_ENTER, MOD_SHIFT)
                    || matches_kitty_sequence(data, CP_KP_ENTER, MOD_SHIFT)
                {
                    return true;
                }
                if matches_modify_other_keys(data, CP_ENTER, MOD_SHIFT) {
                    return true;
                }
                if is_kitty_protocol_active() {
                    return data == "\x1b\r" || data == "\n";
                }
                false
            }
            MOD_ALT => {
                if matches_kitty_sequence(data, CP_ENTER, MOD_ALT) || matches_kitty_sequence(data, CP_KP_ENTER, MOD_ALT)
                {
                    return true;
                }
                if matches_modify_other_keys(data, CP_ENTER, MOD_ALT) {
                    return true;
                }
                if !is_kitty_protocol_active() {
                    return data == "\x1b\r";
                }
                false
            }
            0 => {
                data == "\r"
                    || (!is_kitty_protocol_active() && data == "\n")
                    || data == "\x1bOM"
                    || matches_kitty_sequence(data, CP_ENTER, 0)
                    || matches_kitty_sequence(data, CP_KP_ENTER, 0)
            }
            _ => {
                matches_kitty_sequence(data, CP_ENTER, modifier)
                    || matches_kitty_sequence(data, CP_KP_ENTER, modifier)
                    || matches_modify_other_keys(data, CP_ENTER, modifier)
            }
        },
        "backspace" => match modifier {
            MOD_ALT => {
                if data == "\x1b\x7f" || data == "\x1b\x08" {
                    return true;
                }
                matches_kitty_sequence(data, CP_BACKSPACE, MOD_ALT)
                    || matches_modify_other_keys(data, CP_BACKSPACE, MOD_ALT)
            }
            MOD_CTRL => {
                if matches_raw_backspace(data, MOD_CTRL) {
                    return true;
                }
                matches_kitty_sequence(data, CP_BACKSPACE, MOD_CTRL)
                    || matches_modify_other_keys(data, CP_BACKSPACE, MOD_CTRL)
            }
            0 => {
                matches_raw_backspace(data, 0)
                    || matches_kitty_sequence(data, CP_BACKSPACE, 0)
                    || matches_modify_other_keys(data, CP_BACKSPACE, 0)
            }
            _ => {
                matches_kitty_sequence(data, CP_BACKSPACE, modifier)
                    || matches_modify_other_keys(data, CP_BACKSPACE, modifier)
            }
        },
        "insert" => {
            if modifier == 0 {
                return matches_legacy_sequence(data, LEGACY_INSERT) || matches_kitty_sequence(data, CP_INSERT, 0);
            }
            if matches_legacy_modifier_sequence(data, LegacyModifierKey::Insert, modifier) {
                return true;
            }
            matches_kitty_sequence(data, CP_INSERT, modifier)
        }
        "delete" => {
            if modifier == 0 {
                return matches_legacy_sequence(data, LEGACY_DELETE) || matches_kitty_sequence(data, CP_DELETE, 0);
            }
            if matches_legacy_modifier_sequence(data, LegacyModifierKey::Delete, modifier) {
                return true;
            }
            matches_kitty_sequence(data, CP_DELETE, modifier)
        }
        "clear" => {
            if modifier == 0 {
                return matches_legacy_sequence(data, LEGACY_CLEAR);
            }
            matches_legacy_modifier_sequence(data, LegacyModifierKey::Clear, modifier)
        }
        "home" => {
            if modifier == 0 {
                return matches_legacy_sequence(data, LEGACY_HOME) || matches_kitty_sequence(data, CP_HOME, 0);
            }
            if matches_legacy_modifier_sequence(data, LegacyModifierKey::Home, modifier) {
                return true;
            }
            matches_kitty_sequence(data, CP_HOME, modifier)
        }
        "end" => {
            if modifier == 0 {
                return matches_legacy_sequence(data, LEGACY_END) || matches_kitty_sequence(data, CP_END, 0);
            }
            if matches_legacy_modifier_sequence(data, LegacyModifierKey::End, modifier) {
                return true;
            }
            matches_kitty_sequence(data, CP_END, modifier)
        }
        "pageup" => {
            if modifier == 0 {
                return matches_legacy_sequence(data, LEGACY_PAGE_UP) || matches_kitty_sequence(data, CP_PAGE_UP, 0);
            }
            if matches_legacy_modifier_sequence(data, LegacyModifierKey::PageUp, modifier) {
                return true;
            }
            matches_kitty_sequence(data, CP_PAGE_UP, modifier)
        }
        "pagedown" => {
            if modifier == 0 {
                return matches_legacy_sequence(data, LEGACY_PAGE_DOWN)
                    || matches_kitty_sequence(data, CP_PAGE_DOWN, 0);
            }
            if matches_legacy_modifier_sequence(data, LegacyModifierKey::PageDown, modifier) {
                return true;
            }
            matches_kitty_sequence(data, CP_PAGE_DOWN, modifier)
        }
        "up" => match modifier {
            MOD_ALT => data == "\x1bp" || matches_kitty_sequence(data, CP_UP, MOD_ALT),
            0 => matches_legacy_sequence(data, LEGACY_UP) || matches_kitty_sequence(data, CP_UP, 0),
            _ => {
                matches_legacy_modifier_sequence(data, LegacyModifierKey::Up, modifier)
                    || matches_kitty_sequence(data, CP_UP, modifier)
            }
        },
        "down" => match modifier {
            MOD_ALT => data == "\x1bn" || matches_kitty_sequence(data, CP_DOWN, MOD_ALT),
            0 => matches_legacy_sequence(data, LEGACY_DOWN) || matches_kitty_sequence(data, CP_DOWN, 0),
            _ => {
                matches_legacy_modifier_sequence(data, LegacyModifierKey::Down, modifier)
                    || matches_kitty_sequence(data, CP_DOWN, modifier)
            }
        },
        "left" => match modifier {
            MOD_ALT => {
                data == "\x1b[1;3D"
                    || (!is_kitty_protocol_active() && data == "\x1bB")
                    || data == "\x1bb"
                    || matches_kitty_sequence(data, CP_LEFT, MOD_ALT)
            }
            MOD_CTRL => {
                data == "\x1b[1;5D"
                    || matches_legacy_modifier_sequence(data, LegacyModifierKey::Left, MOD_CTRL)
                    || matches_kitty_sequence(data, CP_LEFT, MOD_CTRL)
            }
            0 => matches_legacy_sequence(data, LEGACY_LEFT) || matches_kitty_sequence(data, CP_LEFT, 0),
            _ => {
                matches_legacy_modifier_sequence(data, LegacyModifierKey::Left, modifier)
                    || matches_kitty_sequence(data, CP_LEFT, modifier)
            }
        },
        "right" => match modifier {
            MOD_ALT => {
                data == "\x1b[1;3C"
                    || (!is_kitty_protocol_active() && data == "\x1bF")
                    || data == "\x1bf"
                    || matches_kitty_sequence(data, CP_RIGHT, MOD_ALT)
            }
            MOD_CTRL => {
                data == "\x1b[1;5C"
                    || matches_legacy_modifier_sequence(data, LegacyModifierKey::Right, MOD_CTRL)
                    || matches_kitty_sequence(data, CP_RIGHT, MOD_CTRL)
            }
            0 => matches_legacy_sequence(data, LEGACY_RIGHT) || matches_kitty_sequence(data, CP_RIGHT, 0),
            _ => {
                matches_legacy_modifier_sequence(data, LegacyModifierKey::Right, modifier)
                    || matches_kitty_sequence(data, CP_RIGHT, modifier)
            }
        },
        "f1" | "f2" | "f3" | "f4" | "f5" | "f6" | "f7" | "f8" | "f9" | "f10" | "f11" | "f12" => {
            if modifier != 0 {
                return false;
            }
            let sequences = match parsed.key.as_str() {
                "f1" => LEGACY_F1,
                "f2" => LEGACY_F2,
                "f3" => LEGACY_F3,
                "f4" => LEGACY_F4,
                "f5" => LEGACY_F5,
                "f6" => LEGACY_F6,
                "f7" => LEGACY_F7,
                "f8" => LEGACY_F8,
                "f9" => LEGACY_F9,
                "f10" => LEGACY_F10,
                "f11" => LEGACY_F11,
                _ => LEGACY_F12,
            };
            matches_legacy_sequence(data, sequences)
        }
        _ => match_single_key(data, &parsed, modifier),
    }
}

fn match_single_key(data: &str, parsed: &ParsedKeyId, modifier: i64) -> bool {
    let key = parsed.key.as_str();
    if utf16_len(key) != 1 || !(is_letter_key(key) || is_digit_key(key) || SYMBOL_KEYS.contains(key)) {
        return false;
    }

    let codepoint = key.chars().next().map_or(0, |character| character as i64);
    let raw_ctrl = raw_ctrl_char(key);
    let is_letter = is_letter_key(key);
    let is_digit = is_digit_key(key);

    if modifier == MOD_CTRL + MOD_ALT
        && !is_kitty_protocol_active()
        && let Some(raw_ctrl) = raw_ctrl
        && data == format!("\x1b{raw_ctrl}")
    {
        return true;
    }

    if modifier == MOD_ALT && (is_digit || (!is_kitty_protocol_active() && is_letter)) && data == format!("\x1b{key}") {
        return true;
    }

    if modifier == MOD_CTRL {
        if let Some(raw_ctrl) = raw_ctrl
            && data == raw_ctrl.to_string()
        {
            return true;
        }
        return matches_kitty_sequence(data, codepoint, MOD_CTRL)
            || matches_printable_modify_other_keys(data, codepoint, MOD_CTRL);
    }

    if modifier == MOD_SHIFT + MOD_CTRL {
        return matches_kitty_sequence(data, codepoint, MOD_SHIFT + MOD_CTRL)
            || matches_printable_modify_other_keys(data, codepoint, MOD_SHIFT + MOD_CTRL);
    }

    if modifier == MOD_SHIFT {
        if is_letter && data == key.to_uppercase() {
            return true;
        }
        return matches_kitty_sequence(data, codepoint, MOD_SHIFT)
            || matches_printable_modify_other_keys(data, codepoint, MOD_SHIFT);
    }

    if modifier != 0 {
        return matches_kitty_sequence(data, codepoint, modifier)
            || matches_printable_modify_other_keys(data, codepoint, modifier);
    }

    data == key || matches_kitty_sequence(data, codepoint, 0)
}

fn format_parsed_key(codepoint: i64, modifier: i64, base_layout_key: Option<u32>) -> Option<String> {
    let normalized_codepoint = normalize_kitty_functional_codepoint(codepoint);
    let identity_codepoint = normalize_shifted_letter_identity_codepoint(normalized_codepoint, modifier);

    let is_latin_letter = (97..=122).contains(&identity_codepoint);
    let is_digit = (48..=57).contains(&identity_codepoint);
    let is_known_symbol = is_symbol_codepoint(identity_codepoint);
    let effective_codepoint = if is_latin_letter || is_digit || is_known_symbol {
        identity_codepoint
    } else {
        base_layout_key.map_or(identity_codepoint, i64::from)
    };

    let key_name = match effective_codepoint {
        CP_ESCAPE => "escape",
        CP_TAB => "tab",
        CP_ENTER | CP_KP_ENTER => "enter",
        CP_SPACE => "space",
        CP_BACKSPACE => "backspace",
        CP_DELETE => "delete",
        CP_INSERT => "insert",
        CP_HOME => "home",
        CP_END => "end",
        CP_PAGE_UP => "pageUp",
        CP_PAGE_DOWN => "pageDown",
        CP_UP => "up",
        CP_DOWN => "down",
        CP_LEFT => "left",
        CP_RIGHT => "right",
        value if (48..=57).contains(&value) => return format_digit_key(value, modifier),
        value if (97..=122).contains(&value) => return format_letter_key(value, modifier),
        value if is_symbol_codepoint(value) => return format_symbol_key(value, modifier),
        _ => return None,
    };

    format_key_name_with_modifiers(key_name, modifier)
}

fn format_digit_key(codepoint: i64, modifier: i64) -> Option<String> {
    let character = char::from_u32(u32::try_from(codepoint).ok()?)?;
    format_key_name_with_modifiers(&character.to_string(), modifier)
}

fn format_letter_key(codepoint: i64, modifier: i64) -> Option<String> {
    let character = char::from_u32(u32::try_from(codepoint).ok()?)?;
    format_key_name_with_modifiers(&character.to_string(), modifier)
}

fn format_symbol_key(codepoint: i64, modifier: i64) -> Option<String> {
    let character = char_from_utf16_code(codepoint)?;
    format_key_name_with_modifiers(&character.to_string(), modifier)
}

/// Parse input data and return the key identifier if recognized.
pub fn parse_key(data: &str) -> Option<String> {
    if let Some(kitty) = parse_kitty_sequence(data) {
        return format_parsed_key(kitty.codepoint, kitty.modifier, kitty.base_layout_key);
    }

    if let Some(modify_other) = parse_modify_other_keys_sequence(data) {
        return format_parsed_key(modify_other.codepoint, modify_other.modifier, None);
    }

    if is_kitty_protocol_active() && (data == "\x1b\r" || data == "\n") {
        return Some("shift+enter".to_string());
    }

    if let Some(key_id) = legacy_sequence_key_id(data) {
        return Some(key_id.to_string());
    }

    match data {
        "\x1b" => return Some("escape".to_string()),
        "\x1c" => return Some("ctrl+\\".to_string()),
        "\x1d" => return Some("ctrl+]".to_string()),
        "\x1f" => return Some("ctrl+-".to_string()),
        "\x1b\x1b" => return Some("ctrl+alt+[".to_string()),
        "\x1b\x1c" => return Some("ctrl+alt+\\".to_string()),
        "\x1b\x1d" => return Some("ctrl+alt+]".to_string()),
        "\x1b\x1f" => return Some("ctrl+alt+-".to_string()),
        "\t" => return Some("tab".to_string()),
        "\r" => return Some("enter".to_string()),
        "\x00" => return Some("ctrl+space".to_string()),
        " " => return Some("space".to_string()),
        "\x7f" => return Some("backspace".to_string()),
        "\x1b[Z" => return Some("shift+tab".to_string()),
        "\x1b\x7f" | "\x1b\x08" => return Some("alt+backspace".to_string()),
        "\x1b[A" => return Some("up".to_string()),
        "\x1b[B" => return Some("down".to_string()),
        "\x1b[C" => return Some("right".to_string()),
        "\x1b[D" => return Some("left".to_string()),
        "\x1b[H" | "\x1bOH" => return Some("home".to_string()),
        "\x1b[F" | "\x1bOF" => return Some("end".to_string()),
        "\x1b[3~" => return Some("delete".to_string()),
        "\x1b[5~" => return Some("pageUp".to_string()),
        "\x1b[6~" => return Some("pageDown".to_string()),
        "\n" if !is_kitty_protocol_active() => return Some("enter".to_string()),
        "\x1bOM" => return Some("enter".to_string()),
        "\x1b\r" if !is_kitty_protocol_active() => return Some("alt+enter".to_string()),
        "\x1b " if !is_kitty_protocol_active() => return Some("alt+space".to_string()),
        "\x1bB" if !is_kitty_protocol_active() => return Some("alt+left".to_string()),
        "\x1bF" if !is_kitty_protocol_active() => return Some("alt+right".to_string()),
        "\x08" => {
            return Some(
                if is_windows_terminal_session() {
                    "ctrl+backspace"
                } else {
                    "backspace"
                }
                .to_string(),
            );
        }
        _ => {}
    }

    if utf16_len(data) == 2 && data.encode_utf16().next() == Some(0x1b) {
        let code = data.encode_utf16().nth(1).unwrap_or(0);
        if !is_kitty_protocol_active() && (1..=26).contains(&code) {
            let letter = char::from_u32(u32::from(code) + 96)?;
            return Some(format!("ctrl+alt+{letter}"));
        }
        if (48..=57).contains(&code) {
            let digit = char::from_u32(u32::from(code))?;
            return Some(format!("alt+{digit}"));
        }
        if !is_kitty_protocol_active() && (97..=122).contains(&code) {
            let letter = char::from_u32(u32::from(code))?;
            return Some(format!("alt+{letter}"));
        }
    }

    if utf16_len(data) == 1 {
        let code = data.encode_utf16().next().unwrap_or(0);
        if (1..=26).contains(&code) {
            let letter = char::from_u32(u32::from(code) + 96)?;
            return Some(format!("ctrl+{letter}"));
        }
        if (32..=126).contains(&code) {
            return Some(data.to_string());
        }
    }

    None
}

const KITTY_PRINTABLE_ALLOWED_MODIFIERS: i64 = MOD_SHIFT | LOCK_MASK;

/// Decode a Kitty CSI-u sequence into a printable character, if applicable.
pub fn decode_kitty_printable(data: &str) -> Option<String> {
    let parsed = parse_kitty_csi_u(data)?;

    let modifier = parsed.modifier;
    if modifier & !KITTY_PRINTABLE_ALLOWED_MODIFIERS != 0 {
        return None;
    }
    if modifier & (MOD_ALT | MOD_CTRL) != 0 {
        return None;
    }

    let mut effective_codepoint = parsed.codepoint;
    if modifier & MOD_SHIFT != 0
        && let Some(shifted_key) = parsed.shifted_key
    {
        effective_codepoint = i64::from(shifted_key);
    }
    effective_codepoint = normalize_kitty_functional_codepoint(effective_codepoint);
    if effective_codepoint < 32 {
        return None;
    }

    char::from_u32(u32::try_from(effective_codepoint).ok()?).map(|character| character.to_string())
}

fn decode_modify_other_keys_printable(data: &str) -> Option<String> {
    let parsed = parse_modify_other_keys_sequence(data)?;
    let modifier = parsed.modifier & !LOCK_MASK;
    if modifier & !MOD_SHIFT != 0 {
        return None;
    }
    if parsed.codepoint < 32 {
        return None;
    }
    char::from_u32(u32::try_from(parsed.codepoint).ok()?).map(|character| character.to_string())
}

/// Decode a printable character from Kitty CSI-u or xterm modifyOtherKeys input.
pub fn decode_printable_key(data: &str) -> Option<String> {
    decode_kitty_printable(data).or_else(|| decode_modify_other_keys_printable(data))
}

/// Check if the input contains a Kitty key-release event.
pub fn is_key_release(data: &str) -> bool {
    if data.contains("\x1b[200~") {
        return false;
    }
    [":3u", ":3~", ":3A", ":3B", ":3C", ":3D", ":3H", ":3F"]
        .iter()
        .any(|marker| data.contains(marker))
}

/// Check if the input contains a Kitty key-repeat event.
pub fn is_key_repeat(data: &str) -> bool {
    if data.contains("\x1b[200~") {
        return false;
    }
    [":2u", ":2~", ":2A", ":2B", ":2C", ":2D", ":2H", ":2F"]
        .iter()
        .any(|marker| data.contains(marker))
}

/// Typed key-identifier helpers, mirroring the source `Key` object.
///
/// TypeScript builds the identifiers with a const object; Rust exposes them as
/// associated constants and functions on a namespace type.
pub struct Key;

impl Key {
    pub const ESCAPE: &'static str = "escape";
    pub const ESC: &'static str = "esc";
    pub const ENTER: &'static str = "enter";
    pub const RETURN: &'static str = "return";
    pub const TAB: &'static str = "tab";
    pub const SPACE: &'static str = "space";
    pub const BACKSPACE: &'static str = "backspace";
    pub const DELETE: &'static str = "delete";
    pub const INSERT: &'static str = "insert";
    pub const CLEAR: &'static str = "clear";
    pub const HOME: &'static str = "home";
    pub const END: &'static str = "end";
    pub const PAGE_UP: &'static str = "pageUp";
    pub const PAGE_DOWN: &'static str = "pageDown";
    pub const UP: &'static str = "up";
    pub const DOWN: &'static str = "down";
    pub const LEFT: &'static str = "left";
    pub const RIGHT: &'static str = "right";
    pub const F1: &'static str = "f1";
    pub const F2: &'static str = "f2";
    pub const F3: &'static str = "f3";
    pub const F4: &'static str = "f4";
    pub const F5: &'static str = "f5";
    pub const F6: &'static str = "f6";
    pub const F7: &'static str = "f7";
    pub const F8: &'static str = "f8";
    pub const F9: &'static str = "f9";
    pub const F10: &'static str = "f10";
    pub const F11: &'static str = "f11";
    pub const F12: &'static str = "f12";

    pub const BACKTICK: &'static str = "`";
    pub const HYPHEN: &'static str = "-";
    pub const EQUALS: &'static str = "=";
    pub const LEFT_BRACKET: &'static str = "[";
    pub const RIGHT_BRACKET: &'static str = "]";
    pub const BACKSLASH: &'static str = "\\";
    pub const SEMICOLON: &'static str = ";";
    pub const QUOTE: &'static str = "'";
    pub const COMMA: &'static str = ",";
    pub const PERIOD: &'static str = ".";
    pub const SLASH: &'static str = "/";
    pub const EXCLAMATION: &'static str = "!";
    pub const AT: &'static str = "@";
    pub const HASH: &'static str = "#";
    pub const DOLLAR: &'static str = "$";
    pub const PERCENT: &'static str = "%";
    pub const CARET: &'static str = "^";
    pub const AMPERSAND: &'static str = "&";
    pub const ASTERISK: &'static str = "*";
    pub const LEFT_PAREN: &'static str = "(";
    pub const RIGHT_PAREN: &'static str = ")";
    pub const UNDERSCORE: &'static str = "_";
    pub const PLUS: &'static str = "+";
    pub const PIPE: &'static str = "|";
    pub const TILDE: &'static str = "~";
    pub const LEFT_BRACE: &'static str = "{";
    pub const RIGHT_BRACE: &'static str = "}";
    pub const COLON: &'static str = ":";
    pub const LESS_THAN: &'static str = "<";
    pub const GREATER_THAN: &'static str = ">";
    pub const QUESTION: &'static str = "?";

    pub fn ctrl(key: &str) -> String {
        format!("ctrl+{key}")
    }

    pub fn shift(key: &str) -> String {
        format!("shift+{key}")
    }

    pub fn alt(key: &str) -> String {
        format!("alt+{key}")
    }

    pub fn super_key(key: &str) -> String {
        format!("super+{key}")
    }

    pub fn ctrl_shift(key: &str) -> String {
        format!("ctrl+shift+{key}")
    }

    pub fn shift_ctrl(key: &str) -> String {
        format!("shift+ctrl+{key}")
    }

    pub fn ctrl_alt(key: &str) -> String {
        format!("ctrl+alt+{key}")
    }

    pub fn alt_ctrl(key: &str) -> String {
        format!("alt+ctrl+{key}")
    }

    pub fn shift_alt(key: &str) -> String {
        format!("shift+alt+{key}")
    }

    pub fn alt_shift(key: &str) -> String {
        format!("alt+shift+{key}")
    }

    pub fn ctrl_super(key: &str) -> String {
        format!("ctrl+super+{key}")
    }

    pub fn super_ctrl(key: &str) -> String {
        format!("super+ctrl+{key}")
    }

    pub fn shift_super(key: &str) -> String {
        format!("shift+super+{key}")
    }

    pub fn super_shift(key: &str) -> String {
        format!("super+shift+{key}")
    }

    pub fn alt_super(key: &str) -> String {
        format!("alt+super+{key}")
    }

    pub fn super_alt(key: &str) -> String {
        format!("super+alt+{key}")
    }

    pub fn ctrl_shift_alt(key: &str) -> String {
        format!("ctrl+shift+alt+{key}")
    }

    pub fn ctrl_shift_super(key: &str) -> String {
        format!("ctrl+shift+super+{key}")
    }
}

#[cfg(test)]
mod tests {
    use super::windows_terminal_session;

    #[test]
    fn windows_terminal_detection_requires_local_session() {
        assert!(windows_terminal_session(true, false, false, false));
        assert!(!windows_terminal_session(false, false, false, false));
        assert!(!windows_terminal_session(true, true, false, false));
        assert!(!windows_terminal_session(true, false, true, false));
        assert!(!windows_terminal_session(true, false, false, true));
    }
}
