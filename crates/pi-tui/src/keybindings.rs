//! Ported from `packages/tui/src/keybindings.ts`.
//!
//! The TypeScript registry keys definitions by string and stores user bindings in
//! a `Record`. The Rust port keeps both in insertion order (`Vec` pairs) because
//! conflict order is observable through [`KeybindingsManager::get_conflicts`].

use std::sync::{Mutex, OnceLock};

use crate::keys::matches_key;

/// Key identifier accepted by [`matches_key`].
pub type KeyId = String;

/// One keybinding definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeybindingDefinition {
    pub default_keys: Vec<KeyId>,
    pub description: Option<String>,
}

/// User overrides as ordered id/keys pairs. Absent ids keep defaults.
///
/// The TypeScript source uses an object literal; Rust keeps the pairs ordered so
/// conflict reporting preserves the same claimant order.
pub type KeybindingsConfig = Vec<(String, Vec<KeyId>)>;

/// A key claimed by more than one user binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeybindingConflict {
    pub key: KeyId,
    pub keybindings: Vec<String>,
}

fn definition(default_keys: &[&str], description: &str) -> KeybindingDefinition {
    KeybindingDefinition {
        default_keys: default_keys.iter().map(|key| (*key).to_string()).collect(),
        description: Some(description.to_string()),
    }
}

/// The built-in `TUI_KEYBINDINGS` table.
pub fn tui_keybindings() -> Vec<(String, KeybindingDefinition)> {
    let entries: Vec<(&str, KeybindingDefinition)> = vec![
        ("tui.editor.cursorUp", definition(&["up"], "Move cursor up")),
        ("tui.editor.cursorDown", definition(&["down"], "Move cursor down")),
        (
            "tui.editor.historyPrevious",
            definition(&[], "Select previous prompt history entry"),
        ),
        (
            "tui.editor.historyNext",
            definition(&[], "Select next prompt history entry"),
        ),
        (
            "tui.editor.cursorLeft",
            definition(&["left", "ctrl+b"], "Move cursor left"),
        ),
        (
            "tui.editor.cursorRight",
            definition(&["right", "ctrl+f"], "Move cursor right"),
        ),
        (
            "tui.editor.cursorWordLeft",
            definition(&["alt+left", "ctrl+left", "alt+b"], "Move cursor word left"),
        ),
        (
            "tui.editor.cursorWordRight",
            definition(&["alt+right", "ctrl+right", "alt+f"], "Move cursor word right"),
        ),
        (
            "tui.editor.cursorLineStart",
            definition(&["home", "ctrl+home", "ctrl+a"], "Move to line start"),
        ),
        (
            "tui.editor.cursorLineEnd",
            definition(&["end", "ctrl+end", "ctrl+e"], "Move to line end"),
        ),
        (
            "tui.editor.jumpForward",
            definition(&["ctrl+]"], "Jump forward to character"),
        ),
        (
            "tui.editor.jumpBackward",
            definition(&["ctrl+alt+]"], "Jump backward to character"),
        ),
        ("tui.editor.pageUp", definition(&["pageUp", "ctrl+pageUp"], "Page up")),
        (
            "tui.editor.pageDown",
            definition(&["pageDown", "ctrl+pageDown"], "Page down"),
        ),
        (
            "tui.editor.deleteCharBackward",
            definition(&["backspace"], "Delete character backward"),
        ),
        (
            "tui.editor.deleteCharForward",
            definition(&["delete", "ctrl+d"], "Delete character forward"),
        ),
        (
            "tui.editor.deleteWordBackward",
            definition(&["ctrl+w", "alt+backspace"], "Delete word backward"),
        ),
        (
            "tui.editor.deleteWordForward",
            definition(&["alt+d", "alt+delete"], "Delete word forward"),
        ),
        (
            "tui.editor.deleteToLineStart",
            definition(&["ctrl+u"], "Delete to line start"),
        ),
        (
            "tui.editor.deleteToLineEnd",
            definition(&["ctrl+k"], "Delete to line end"),
        ),
        ("tui.editor.yank", definition(&["ctrl+y"], "Yank")),
        ("tui.editor.yankPop", definition(&["alt+y"], "Yank pop")),
        ("tui.editor.undo", definition(&["ctrl+-"], "Undo")),
        (
            "tui.editor.historySearch",
            definition(&["ctrl+r"], "Reverse search prompt history"),
        ),
        (
            "tui.input.newLine",
            definition(&["shift+enter", "ctrl+j"], "Insert newline"),
        ),
        ("tui.input.submit", definition(&["enter"], "Submit input")),
        ("tui.input.tab", definition(&["tab"], "Tab / autocomplete")),
        ("tui.input.copy", definition(&["ctrl+c"], "Copy selection")),
        ("tui.select.up", definition(&["up"], "Move selection up")),
        ("tui.select.down", definition(&["down"], "Move selection down")),
        ("tui.select.pageUp", definition(&["pageUp"], "Selection page up")),
        ("tui.select.pageDown", definition(&["pageDown"], "Selection page down")),
        ("tui.select.confirm", definition(&["enter"], "Confirm selection")),
        (
            "tui.select.cancel",
            definition(&["escape", "ctrl+c"], "Cancel selection"),
        ),
        (
            "tui.altScreen.pageUp",
            definition(&["pageUp"], "Scroll viewport up one page"),
        ),
        (
            "tui.altScreen.pageDown",
            definition(&["pageDown"], "Scroll viewport down one page"),
        ),
        (
            "tui.altScreen.halfPageUp",
            definition(&[], "Scroll viewport up half a page"),
        ),
        (
            "tui.altScreen.halfPageDown",
            definition(&[], "Scroll viewport down half a page"),
        ),
        ("tui.altScreen.lineUp", definition(&[], "Scroll viewport up one line")),
        (
            "tui.altScreen.lineDown",
            definition(&[], "Scroll viewport down one line"),
        ),
        (
            "tui.altScreen.previousPrompt",
            definition(&["ctrl+shift+up"], "Jump to previous semantic prompt"),
        ),
        (
            "tui.altScreen.nextPrompt",
            definition(&["ctrl+shift+down"], "Jump to next semantic prompt"),
        ),
        (
            "tui.altScreen.search",
            definition(&["ctrl+shift+f"], "Search the primary scroll view"),
        ),
        (
            "tui.altScreen.searchNext",
            definition(&["enter", "ctrl+g"], "Select the next search match"),
        ),
        (
            "tui.altScreen.searchPrevious",
            definition(&["shift+enter", "ctrl+shift+g"], "Select the previous search match"),
        ),
        (
            "tui.altScreen.searchClose",
            definition(&["escape"], "Close transcript search"),
        ),
        ("tui.altScreen.top", definition(&["home"], "Scroll viewport to top")),
        (
            "tui.altScreen.bottom",
            definition(&["end"], "Scroll viewport to bottom"),
        ),
    ];
    entries.into_iter().map(|(id, value)| (id.to_string(), value)).collect()
}

fn normalize_keys(keys: Option<&Vec<KeyId>>) -> Vec<KeyId> {
    let mut result = Vec::new();
    for key in keys.into_iter().flatten() {
        if !result.contains(key) {
            result.push(key.clone());
        }
    }
    result
}

/// Resolves keybinding ids against defaults and user overrides.
#[derive(Debug, Clone)]
pub struct KeybindingsManager {
    definitions: Vec<(String, KeybindingDefinition)>,
    user_bindings: KeybindingsConfig,
    keys_by_id: Vec<(String, Vec<KeyId>)>,
    conflicts: Vec<KeybindingConflict>,
}

impl KeybindingsManager {
    pub fn new(definitions: Vec<(String, KeybindingDefinition)>, user_bindings: KeybindingsConfig) -> Self {
        let mut manager = Self {
            definitions,
            user_bindings,
            keys_by_id: Vec::new(),
            conflicts: Vec::new(),
        };
        manager.rebuild();
        manager
    }

    fn rebuild(&mut self) {
        self.keys_by_id.clear();
        self.conflicts.clear();

        // User claim order follows the config insertion order, matching `Map` order in the source.
        let mut user_claims: Vec<(KeyId, Vec<String>)> = Vec::new();
        for (keybinding, keys) in &self.user_bindings {
            if !self.definitions.iter().any(|(id, _)| id == keybinding) {
                continue;
            }
            for key in normalize_keys(Some(keys)) {
                match user_claims.iter_mut().find(|(claimed, _)| *claimed == key) {
                    Some((_, claimants)) => {
                        if !claimants.contains(keybinding) {
                            claimants.push(keybinding.clone());
                        }
                    }
                    None => user_claims.push((key, vec![keybinding.clone()])),
                }
            }
        }

        for (key, keybindings) in user_claims {
            if keybindings.len() > 1 {
                self.conflicts.push(KeybindingConflict { key, keybindings });
            }
        }

        for (id, definition) in &self.definitions {
            let keys = match self
                .user_bindings
                .iter()
                .find(|(binding, _)| binding == id)
                .map(|(_, keys)| keys)
            {
                Some(user_keys) => normalize_keys(Some(user_keys)),
                None => normalize_keys(Some(&definition.default_keys)),
            };
            self.keys_by_id.push((id.clone(), keys));
        }
    }

    pub fn matches(&self, data: &str, keybinding: &str) -> bool {
        self.get_keys(keybinding).iter().any(|key| matches_key(data, key))
    }

    pub fn get_keys(&self, keybinding: &str) -> Vec<KeyId> {
        self.keys_by_id
            .iter()
            .find(|(id, _)| id == keybinding)
            .map(|(_, keys)| keys.clone())
            .unwrap_or_default()
    }

    pub fn get_definition(&self, keybinding: &str) -> Option<&KeybindingDefinition> {
        self.definitions
            .iter()
            .find(|(id, _)| id == keybinding)
            .map(|(_, definition)| definition)
    }

    pub fn get_conflicts(&self) -> Vec<KeybindingConflict> {
        self.conflicts.clone()
    }

    pub fn set_user_bindings(&mut self, user_bindings: KeybindingsConfig) {
        self.user_bindings = user_bindings;
        self.rebuild();
    }

    pub fn get_user_bindings(&self) -> KeybindingsConfig {
        self.user_bindings.clone()
    }

    /// Resolved keys for every definition, in definition order.
    pub fn get_resolved_bindings(&self) -> Vec<(String, Vec<KeyId>)> {
        self.keys_by_id.clone()
    }
}

fn global_keybindings() -> &'static Mutex<Option<KeybindingsManager>> {
    static GLOBAL: OnceLock<Mutex<Option<KeybindingsManager>>> = OnceLock::new();
    GLOBAL.get_or_init(|| Mutex::new(None))
}

/// Install the process-wide keybinding manager.
pub fn set_keybindings(keybindings: KeybindingsManager) {
    *global_keybindings().lock().expect("keybinding registry lock poisoned") = Some(keybindings);
}

/// Return the process-wide manager, creating the default one on first use.
pub fn get_keybindings() -> KeybindingsManager {
    let mut guard = global_keybindings().lock().expect("keybinding registry lock poisoned");
    guard
        .get_or_insert_with(|| KeybindingsManager::new(tui_keybindings(), KeybindingsConfig::new()))
        .clone()
}
