//! Ported from `packages/coding-agent/src/modes/interactive/components/keybinding-hints.ts`.

use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use pi_tui::keybindings::{KeyId, get_keybindings};

use super::super::theme::{ThemeColor, with_theme};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyTextFormatOptions {
    pub capitalize: bool,
}

fn uppercase_first_unit(text: &JsString) -> JsString {
    let Some(first) = text.units().first() else {
        return JsString::default();
    };
    let initial = match char::from_u32(u32::from(*first)) {
        Some(character) => JsString::from(character.to_uppercase().collect::<String>()),
        None => text.slice(0, 1),
    };
    initial + text.slice(1, text.len())
}

fn format_key_part(part: &JsString, options: KeyTextFormatOptions) -> JsString {
    let display_part = if cfg!(target_os = "macos") && part.to_lowercase() == "alt" {
        JsString::from("option")
    } else {
        part.clone()
    };
    if options.capitalize {
        uppercase_first_unit(&display_part)
    } else {
        display_part
    }
}

/// Format a key spec such as `ctrl+a/alt+b`, retaining arbitrary source units.
pub fn format_key_text(key: impl Into<JsString>, options: KeyTextFormatOptions) -> JsString {
    let key = key.into();
    JsString::join(
        key.split('/').map(|part| {
            JsString::join(
                part.split('+').map(|component| format_key_part(&component, options)),
                "+",
            )
        }),
        "/",
    )
}

fn format_keys(keys: &[KeyId], options: KeyTextFormatOptions) -> JsString {
    format_key_text(JsString::join(keys, "/"), options)
}

pub fn key_text(keybinding: impl Into<JsString>) -> JsString {
    format_keys(&get_keybindings().get_keys(keybinding), KeyTextFormatOptions::default())
}

pub fn key_display_text(keybinding: impl Into<JsString>) -> JsString {
    format_keys(
        &get_keybindings().get_keys(keybinding),
        KeyTextFormatOptions { capitalize: true },
    )
}

pub fn key_hint(keybinding: impl Into<JsString>, description: impl Into<JsString>) -> JsString {
    let key = with_theme(|theme| theme.fg(ThemeColor::Dim, key_text(keybinding)));
    let description = description.into();
    let description = with_theme(|theme| theme.fg(ThemeColor::Muted, js_format!(" {}", description)));
    key + description
}

pub fn raw_key_hint(key: impl Into<JsString>, description: impl Into<JsString>) -> JsString {
    let key = key.into();
    let key = with_theme(|theme| theme.fg(ThemeColor::Dim, format_key_text(key, KeyTextFormatOptions::default())));
    let description = description.into();
    let description = with_theme(|theme| theme.fg(ThemeColor::Muted, js_format!(" {}", description)));
    key + description
}

#[cfg(test)]
mod lossless_tests {
    use super::*;

    #[test]
    fn capitalize_uses_first_unit_without_replacing_a_surrogate() {
        let key = JsString::from_utf16(vec![0xd800]) + "+ß/ctrl+" + JsString::from_utf16(vec![0xdfff]);
        let actual = format_key_text(&key, KeyTextFormatOptions { capitalize: true });
        assert_eq!(
            actual,
            JsString::from_utf16(vec![0xd800]) + "+SS/Ctrl+" + JsString::from_utf16(vec![0xdfff])
        );
    }
}
