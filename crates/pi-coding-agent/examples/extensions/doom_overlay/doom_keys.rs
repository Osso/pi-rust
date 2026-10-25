//! Separable key mapper from the pinned Doom overlay; no extension is loaded.
use pi_js_compat::js_json::JsString;
use pi_tui::keys::{matches_key, parse_key};

pub const DOOM_KEYS: &[(&str, u32)] = &[
    ("KEY_RIGHTARROW", 0xae),
    ("KEY_LEFTARROW", 0xac),
    ("KEY_UPARROW", 0xad),
    ("KEY_DOWNARROW", 0xaf),
    ("KEY_STRAFE_L", 0xa0),
    ("KEY_STRAFE_R", 0xa1),
    ("KEY_USE", 0xa2),
    ("KEY_FIRE", 0xa3),
    ("KEY_ESCAPE", 27),
    ("KEY_ENTER", 13),
    ("KEY_TAB", 9),
    ("KEY_F1", 0xbb),
    ("KEY_F2", 0xbc),
    ("KEY_F3", 0xbd),
    ("KEY_F4", 0xbe),
    ("KEY_F5", 0xbf),
    ("KEY_F6", 0xc0),
    ("KEY_F7", 0xc1),
    ("KEY_F8", 0xc2),
    ("KEY_F9", 0xc3),
    ("KEY_F10", 0xc4),
    ("KEY_F11", 0xd7),
    ("KEY_F12", 0xd8),
    ("KEY_BACKSPACE", 127),
    ("KEY_PAUSE", 0xff),
    ("KEY_EQUALS", 0x3d),
    ("KEY_MINUS", 0x2d),
    ("KEY_RSHIFT", 0xb6),
    ("KEY_RCTRL", 0x9d),
    ("KEY_RALT", 0xb8),
];

pub fn map_key_to_doom(input: &JsString) -> Vec<u32> {
    let units = input.units();
    // Existing pi-tui String-typed key APIs use the approved U+FFFD projection;
    // raw code units remain available for the example's charCodeAt branches.
    let projected = input.to_utf8_lossy();
    let data = projected.as_str();
    for (key, code) in [("up", 0xad), ("down", 0xaf), ("right", 0xae), ("left", 0xac)] {
        if matches_key(data, key) {
            return vec![code];
        }
    }
    for (raw, shifted, code) in [("w", "W", 0xad), ("s", "S", 0xaf), ("a", "A", 0xa0), ("d", "D", 0xa1)] {
        if data == raw || matches_key(data, raw) {
            return vec![code];
        }
        if data == shifted || matches_key(data, format!("shift+{raw}")) {
            return vec![code, 0xb6];
        }
    }
    if matches_letter(data, "f", "F") {
        return vec![0xa3];
    }
    if data == " " || matches_key(data, "space") {
        return vec![0xa2];
    }
    for (key, code) in [("enter", 13), ("escape", 27), ("tab", 9), ("backspace", 127)] {
        if matches_key(data, key) {
            return vec![code];
        }
    }
    if parse_key(data).is_some_and(|key| key.starts_with("ctrl+") && key != "ctrl+c") {
        return vec![0xa3];
    }
    let first = units.first().copied();
    let single = units.len() == 1;
    if single && first.is_some_and(|unit| unit < 32) && data != "\u{3}" {
        return vec![0xa3];
    }
    // Preserve the source's lexical comparison, including strings like "12".
    if units >= [48].as_slice() && units <= [57].as_slice() {
        return vec![u32::from(first.unwrap())];
    }
    if data == "+" || data == "=" {
        return vec![0x3d];
    }
    if data == "-" {
        return vec![0x2d];
    }
    if matches_letter(data, "y", "Y") {
        return vec![u32::from(b'y')];
    }
    if matches_letter(data, "n", "N") {
        return vec![u32::from(b'n')];
    }
    if single && first.is_some_and(|unit| unit >= 32) {
        let first = first.unwrap();
        if (0xd800..=0xdfff).contains(&first) {
            return vec![u32::from(first)];
        }
        return vec![u32::from(data.to_lowercase().encode_utf16().next().unwrap())];
    }
    Vec::new()
}

fn matches_letter(data: &str, lower: &str, upper: &str) -> bool {
    data == lower || data == upper || matches_key(data, lower) || matches_key(data, format!("shift+{lower}"))
}
