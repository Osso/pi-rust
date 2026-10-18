//! Ported from `packages/coding-agent/src/modes/interactive/components/keybinding-hints.ts`.

use pi_tui::keybindings::{KeyId, get_keybindings};

use super::super::theme::{ThemeColor, with_theme};

/// `KeyTextFormatOptions`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyTextFormatOptions {
    pub capitalize: bool,
}

fn format_key_part(part: &str, options: KeyTextFormatOptions) -> String {
    let display_part = if cfg!(target_os = "macos") && part.eq_ignore_ascii_case("alt") {
        "option"
    } else {
        part
    };
    if options.capitalize {
        let mut characters = display_part.chars();
        match characters.next() {
            Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
            None => String::new(),
        }
    } else {
        display_part.to_owned()
    }
}

/// Format a key spec such as `ctrl+a/alt+b`.
pub fn format_key_text(key: &str, options: KeyTextFormatOptions) -> String {
    key.split('/')
        .map(|part| {
            part.split('+')
                .map(|component| format_key_part(component, options))
                .collect::<Vec<_>>()
                .join("+")
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn format_keys(keys: &[KeyId], options: KeyTextFormatOptions) -> String {
    if keys.is_empty() {
        return String::new();
    }
    format_key_text(&keys.join("/"), options)
}

pub fn key_text(keybinding: &str) -> String {
    format_keys(&get_keybindings().get_keys(keybinding), KeyTextFormatOptions::default())
}

pub fn key_display_text(keybinding: &str) -> String {
    format_keys(
        &get_keybindings().get_keys(keybinding),
        KeyTextFormatOptions { capitalize: true },
    )
}

pub fn key_hint(keybinding: &str, description: &str) -> String {
    let key = with_theme(|theme| theme.fg(ThemeColor::Dim, &key_text(keybinding)));
    let description = with_theme(|theme| theme.fg(ThemeColor::Muted, &format!(" {description}")));
    key + &description
}

pub fn raw_key_hint(key: &str, description: &str) -> String {
    let key = with_theme(|theme| theme.fg(ThemeColor::Dim, &format_key_text(key, KeyTextFormatOptions::default())));
    let description = with_theme(|theme| theme.fg(ThemeColor::Muted, &format!(" {description}")));
    key + &description
}
