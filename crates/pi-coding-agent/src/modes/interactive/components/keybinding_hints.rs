//! Ported from `coding-agent/src/modes/interactive/components/keybinding-hints.ts`.

use std::sync::Arc;

use pi_tui::get_keybindings;

use crate::modes::interactive::theme::{Theme, ThemeColor};

/// `KeyTextFormatOptions`.
#[derive(Debug, Clone, Copy, Default)]
pub struct KeyTextFormatOptions {
    pub capitalize: bool,
}

fn format_key_part(part: &str, options: KeyTextFormatOptions) -> String {
    let display_part = if cfg!(target_os = "macos") && part.to_lowercase() == "alt" {
        "option"
    } else {
        part
    };
    if !options.capitalize {
        return display_part.to_string();
    }
    let mut chars = display_part.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// `formatKeyText`: formats `ctrl+a/shift+b` style key lists.
pub fn format_key_text(key: &str, options: KeyTextFormatOptions) -> String {
    key.split('/')
        .map(|combo| {
            combo
                .split('+')
                .map(|part| format_key_part(part, options))
                .collect::<Vec<_>>()
                .join("+")
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn format_keys(keys: &[String], options: KeyTextFormatOptions) -> String {
    if keys.is_empty() {
        return String::new();
    }
    format_key_text(&keys.join("/"), options)
}

/// `keyText`: resolved bindings for a keybinding id.
pub fn key_text(keybinding: &str) -> String {
    format_keys(&get_keybindings().get_keys(keybinding), KeyTextFormatOptions::default())
}

/// `keyDisplayText`: resolved bindings, capitalized.
pub fn key_display_text(keybinding: &str) -> String {
    format_keys(
        &get_keybindings().get_keys(keybinding),
        KeyTextFormatOptions { capitalize: true },
    )
}

/// `keyHint`: dim keys followed by a muted description.
pub fn key_hint(theme: &Arc<Theme>, keybinding: &str, description: &str) -> String {
    theme.fg(ThemeColor::Dim, &key_text(keybinding)) + &theme.fg(ThemeColor::Muted, &format!(" {description}"))
}

/// `rawKeyHint`: dim literal key text followed by a muted description.
pub fn raw_key_hint(theme: &Arc<Theme>, key: &str, description: &str) -> String {
    theme.fg(ThemeColor::Dim, &format_key_text(key, KeyTextFormatOptions::default()))
        + &theme.fg(ThemeColor::Muted, &format!(" {description}"))
}
