//! Coding-agent keybindings (`core/keybindings.ts`), layered on the TUI registry.

use pi_js_compat::js_json::{JsObject, JsString, JsValue};
use pi_tui::keybindings::{
    KeyId, KeybindingDefinition, KeybindingsConfig, KeybindingsManager as TuiKeybindingsManager, tui_keybindings,
};

use crate::config::get_agent_dir;
use pi_js_compat::js_json;

/// Coding-agent bindings merged after [`tui_keybindings`].
pub fn app_keybindings() -> Vec<(JsString, KeybindingDefinition)> {
    let entries: Vec<(&str, KeybindingDefinition)> = vec![
        ("app.interrupt", definition(&["escape"], "Cancel or abort")),
        ("app.clear", definition(&["ctrl+c"], "Clear editor")),
        ("app.exit", definition(&["ctrl+d"], "Exit when editor is empty")),
        (
            "app.suspend",
            definition(
                if cfg!(target_os = "windows") { &[] } else { &["ctrl+z"] },
                "Suspend to background",
            ),
        ),
        ("app.thinking.cycle", definition(&["shift+tab"], "Cycle thinking level")),
        ("app.model.cycleForward", definition(&["ctrl+p"], "Cycle to next model")),
        (
            "app.model.cycleBackward",
            definition(&["shift+ctrl+p"], "Cycle to previous model"),
        ),
        ("app.model.select", definition(&["ctrl+l"], "Open model selector")),
        ("app.tools.expand", definition(&["ctrl+o"], "Toggle tool output")),
        (
            "app.tool.background",
            definition(&["ctrl+b"], "Detach running tool to background"),
        ),
        ("app.thinking.toggle", definition(&["ctrl+t"], "Toggle thinking blocks")),
        (
            "app.session.toggleNamedFilter",
            definition(&["ctrl+n"], "Toggle named session filter"),
        ),
        ("app.editor.external", definition(&["ctrl+g"], "Open external editor")),
        (
            "app.message.followUp",
            definition(&["alt+enter"], "Queue follow-up message"),
        ),
        (
            "app.message.dequeue",
            definition(&["alt+up"], "Restore queued messages"),
        ),
        (
            "app.clipboard.pasteImage",
            definition(
                if cfg!(target_os = "windows") {
                    &["alt+v"]
                } else {
                    &["ctrl+v"]
                },
                "Paste image from clipboard",
            ),
        ),
        ("app.session.new", definition(&[], "Start a new session")),
        ("app.session.tree", definition(&[], "Open session tree")),
        ("app.session.fork", definition(&[], "Fork current session")),
        ("app.session.resume", definition(&[], "Resume a session")),
        (
            "app.tree.foldOrUp",
            definition(&["ctrl+left", "alt+left"], "Fold tree branch or move up"),
        ),
        (
            "app.tree.unfoldOrDown",
            definition(&["ctrl+right", "alt+right"], "Unfold tree branch or move down"),
        ),
        ("app.tree.editLabel", definition(&["shift+l"], "Edit tree label")),
        (
            "app.tree.toggleLabelTimestamp",
            definition(&["shift+t"], "Toggle tree label timestamps"),
        ),
        (
            "app.session.togglePath",
            definition(&["ctrl+p"], "Toggle session path display"),
        ),
        (
            "app.session.toggleSort",
            definition(&["ctrl+s"], "Toggle session sort mode"),
        ),
        ("app.session.rename", definition(&["ctrl+r"], "Rename session")),
        ("app.session.delete", definition(&["ctrl+d"], "Delete session")),
        (
            "app.session.deleteNoninvasive",
            definition(&["ctrl+backspace"], "Delete session when query is empty"),
        ),
        ("app.session.archive", definition(&["ctrl+a"], "Archive session")),
        (
            "app.models.save",
            definition(&["ctrl+s", "ctrl+enter"], "Save model selection"),
        ),
        ("app.models.enableAll", definition(&["ctrl+a"], "Enable all models")),
        ("app.models.clearAll", definition(&["ctrl+x"], "Clear all models")),
        (
            "app.models.toggleProvider",
            definition(&["ctrl+p"], "Toggle all models for provider"),
        ),
        (
            "app.models.reorderUp",
            definition(&["alt+up"], "Move model up in order"),
        ),
        (
            "app.models.reorderDown",
            definition(&["alt+down"], "Move model down in order"),
        ),
        ("app.agent.select", definition(&["alt+0"], "Open agent switcher")),
        (
            "app.agent.toggleClosed",
            definition(&["ctrl+h"], "Toggle closed agents in the agent switcher"),
        ),
        ("app.agent.slot1", definition(&["alt+1"], "Switch to main thread")),
        (
            "app.agent.slot2",
            definition(&["alt+2"], "Switch to active agent slot 1"),
        ),
        (
            "app.agent.slot3",
            definition(&["alt+3"], "Switch to active agent slot 2"),
        ),
        (
            "app.agent.slot4",
            definition(&["alt+4"], "Switch to active agent slot 3"),
        ),
        (
            "app.agent.slot5",
            definition(&["alt+5"], "Switch to active agent slot 4"),
        ),
        (
            "app.agent.slot6",
            definition(&["alt+6"], "Switch to active agent slot 5"),
        ),
        (
            "app.agent.slot7",
            definition(&["alt+7"], "Switch to active agent slot 6"),
        ),
        (
            "app.agent.slot8",
            definition(&["alt+8"], "Switch to active agent slot 7"),
        ),
        (
            "app.agent.slot9",
            definition(&["alt+9"], "Switch to active agent slot 8"),
        ),
        (
            "app.tree.filter.default",
            definition(&["ctrl+d"], "Tree filter: default view"),
        ),
        (
            "app.tree.filter.noTools",
            definition(&["ctrl+t"], "Tree filter: hide tool results"),
        ),
        (
            "app.tree.filter.userOnly",
            definition(&["ctrl+u"], "Tree filter: user messages only"),
        ),
        (
            "app.tree.filter.labeledOnly",
            definition(&["ctrl+l"], "Tree filter: labeled entries only"),
        ),
        (
            "app.tree.filter.all",
            definition(&["ctrl+a"], "Tree filter: show all entries"),
        ),
        (
            "app.tree.filter.cycleForward",
            definition(&["ctrl+o"], "Tree filter: cycle forward"),
        ),
        (
            "app.tree.filter.cycleBackward",
            definition(&["shift+ctrl+o"], "Tree filter: cycle backward"),
        ),
        (
            "app.questions.next",
            definition(&["right", "tab"], "Next question in ask_questions"),
        ),
        (
            "app.questions.previous",
            definition(&["left", "shift+tab"], "Previous question in ask_questions"),
        ),
    ];
    entries
        .into_iter()
        .map(|(name, definition)| (JsString::from(name), definition))
        .collect()
}

fn definition(default_keys: &[&str], description: &str) -> KeybindingDefinition {
    KeybindingDefinition {
        default_keys: default_keys.iter().map(|key| JsString::from(*key)).collect(),
        description: Some(description.into()),
    }
}

/// `KEYBINDINGS`: TUI definitions followed by coding-agent definitions.
pub fn keybindings() -> Vec<(JsString, KeybindingDefinition)> {
    let mut definitions = tui_keybindings();
    definitions.extend(app_keybindings());
    definitions
}

/// Legacy key names mapped to their namespaced ids.
pub fn keybinding_name_migrations() -> Vec<(&'static str, &'static str)> {
    vec![
        ("cursorUp", "tui.editor.cursorUp"),
        ("cursorDown", "tui.editor.cursorDown"),
        ("cursorLeft", "tui.editor.cursorLeft"),
        ("cursorRight", "tui.editor.cursorRight"),
        ("cursorWordLeft", "tui.editor.cursorWordLeft"),
        ("cursorWordRight", "tui.editor.cursorWordRight"),
        ("cursorLineStart", "tui.editor.cursorLineStart"),
        ("cursorLineEnd", "tui.editor.cursorLineEnd"),
        ("jumpForward", "tui.editor.jumpForward"),
        ("jumpBackward", "tui.editor.jumpBackward"),
        ("pageUp", "tui.editor.pageUp"),
        ("pageDown", "tui.editor.pageDown"),
        ("deleteCharBackward", "tui.editor.deleteCharBackward"),
        ("deleteCharForward", "tui.editor.deleteCharForward"),
        ("deleteWordBackward", "tui.editor.deleteWordBackward"),
        ("deleteWordForward", "tui.editor.deleteWordForward"),
        ("deleteToLineStart", "tui.editor.deleteToLineStart"),
        ("deleteToLineEnd", "tui.editor.deleteToLineEnd"),
        ("yank", "tui.editor.yank"),
        ("yankPop", "tui.editor.yankPop"),
        ("undo", "tui.editor.undo"),
        ("newLine", "tui.input.newLine"),
        ("submit", "tui.input.submit"),
        ("tab", "tui.input.tab"),
        ("copy", "tui.input.copy"),
        ("selectUp", "tui.select.up"),
        ("selectDown", "tui.select.down"),
        ("selectPageUp", "tui.select.pageUp"),
        ("selectPageDown", "tui.select.pageDown"),
        ("selectConfirm", "tui.select.confirm"),
        ("selectCancel", "tui.select.cancel"),
        ("interrupt", "app.interrupt"),
        ("clear", "app.clear"),
        ("exit", "app.exit"),
        ("suspend", "app.suspend"),
        ("cycleThinkingLevel", "app.thinking.cycle"),
        ("cycleModelForward", "app.model.cycleForward"),
        ("cycleModelBackward", "app.model.cycleBackward"),
        ("selectModel", "app.model.select"),
        ("expandTools", "app.tools.expand"),
        ("backgroundTool", "app.tool.background"),
        ("toggleThinking", "app.thinking.toggle"),
        ("toggleSessionNamedFilter", "app.session.toggleNamedFilter"),
        ("externalEditor", "app.editor.external"),
        ("followUp", "app.message.followUp"),
        ("dequeue", "app.message.dequeue"),
        ("pasteImage", "app.clipboard.pasteImage"),
        ("newSession", "app.session.new"),
        ("tree", "app.session.tree"),
        ("fork", "app.session.fork"),
        ("resume", "app.session.resume"),
        ("treeFoldOrUp", "app.tree.foldOrUp"),
        ("treeUnfoldOrDown", "app.tree.unfoldOrDown"),
        ("treeEditLabel", "app.tree.editLabel"),
        ("treeToggleLabelTimestamp", "app.tree.toggleLabelTimestamp"),
        ("toggleSessionPath", "app.session.togglePath"),
        ("toggleSessionSort", "app.session.toggleSort"),
        ("renameSession", "app.session.rename"),
        ("deleteSession", "app.session.delete"),
        ("deleteSessionNoninvasive", "app.session.deleteNoninvasive"),
        ("archiveSession", "app.session.archive"),
    ]
}

fn legacy_keybinding_name(key: &JsString) -> Option<&'static str> {
    keybinding_name_migrations()
        .into_iter()
        .find(|(legacy, _)| key == *legacy)
        .map(|(_, current)| current)
}

/// Result of migrating a raw keybindings file object.
#[derive(Debug, Clone, PartialEq)]
pub struct KeybindingsMigration {
    pub config: JsObject,
    pub migrated: bool,
}

/// Renames legacy ids and orders the object like the TypeScript loader.
pub fn migrate_keybindings_config(raw_config: &JsObject) -> KeybindingsMigration {
    let mut config = JsObject::new();
    let mut migrated = false;

    for (key, value) in js_json::object_entries_lossless(raw_config) {
        let next_key = legacy_keybinding_name(key)
            .map(JsString::from)
            .unwrap_or_else(|| key.clone());
        if &next_key != key {
            migrated = true;
        }
        if key != &next_key && raw_config.contains_key(&next_key) {
            migrated = true;
            continue;
        }
        config.insert(next_key, value.clone());
    }

    KeybindingsMigration {
        config: order_keybindings_config(config),
        migrated,
    }
}

fn order_keybindings_config(config: JsObject) -> JsObject {
    let mut ordered = JsObject::new();
    for (keybinding, _) in keybindings() {
        if let Some(value) = config.get(&keybinding) {
            ordered.insert(keybinding, value.clone());
        }
    }

    let mut extras: Vec<&JsString> = config.keys().filter(|key| !ordered.contains_key(*key)).collect();
    extras.sort_by(|left, right| compare_utf16(left, right));
    for key in extras {
        ordered.insert(key.clone(), config[key].clone());
    }
    with_javascript_key_order(ordered)
}

/// JavaScript objects enumerate array-index keys first, in ascending numeric order.
fn with_javascript_key_order(config: JsObject) -> JsObject {
    let mut ordered = JsObject::new();
    for (key, value) in js_json::object_entries_lossless(&config) {
        ordered.insert(key.clone(), value.clone());
    }
    ordered
}

/// JavaScript's default `Array.prototype.sort` compares UTF-16 code units.
fn compare_utf16(left: &JsString, right: &JsString) -> std::cmp::Ordering {
    left.units().cmp(right.units())
}

fn load_raw_config(path: &JsString) -> Option<JsObject> {
    let encoded_path = js_json::encode_node_utf8(path);
    if !std::path::Path::new(&encoded_path).exists() {
        return None;
    }
    let content = std::fs::read_to_string(&encoded_path).ok()?;
    match js_json::parse_lossless(content).ok()? {
        JsValue::Object(object) => Some(object),
        _ => None,
    }
}

fn to_keybindings_config(value: &JsObject) -> KeybindingsConfig {
    let mut config = KeybindingsConfig::new();
    for (key, binding) in value {
        match binding {
            JsValue::String(text) => config.push((key.clone(), vec![text.clone()])),
            JsValue::Array(entries) => {
                let keys: Option<Vec<KeyId>> = entries.iter().map(|entry| entry.as_string().cloned()).collect();
                if let Some(keys) = keys {
                    config.push((key.clone(), keys));
                }
            }
            _ => {}
        }
    }
    config
}

/// Coding-agent keybindings loaded from `<agentDir>/keybindings.json`.
#[derive(Debug, Clone)]
pub struct KeybindingsManager {
    config_path: Option<JsString>,
    inner: TuiKeybindingsManager,
}

impl KeybindingsManager {
    pub fn new(user_bindings: KeybindingsConfig, config_path: Option<JsString>) -> Self {
        Self {
            config_path,
            inner: TuiKeybindingsManager::new(keybindings(), user_bindings),
        }
    }

    /// Loads `keybindings.json` from the agent directory.
    pub fn create(agent_dir: Option<&JsString>) -> Result<Self, crate::config::ConfigError> {
        let agent_dir = match agent_dir {
            Some(directory) => directory.clone(),
            None => get_agent_dir()?,
        };
        let config_path = crate::utils::paths::node_join(&agent_dir, "keybindings.json");
        let user_bindings = Self::load_from_file(&config_path);
        Ok(Self::new(user_bindings, Some(config_path)))
    }

    /// Install this manager as the process-wide TUI keybinding registry
    /// (`setKeybindings` in `interactive-mode.ts`).
    pub fn install(self) {
        pi_tui::keybindings::set_keybindings(self.inner);
    }

    pub fn reload(&mut self) {
        let Some(config_path) = &self.config_path else {
            return;
        };
        let user_bindings = Self::load_from_file(config_path);
        self.inner.set_user_bindings(user_bindings);
    }

    pub fn get_effective_config(&self) -> KeybindingsConfig {
        self.inner.get_resolved_bindings()
    }

    pub fn get_user_bindings(&self) -> KeybindingsConfig {
        self.inner.get_user_bindings()
    }

    pub fn get_keys(&self, keybinding: impl Into<JsString>) -> Vec<KeyId> {
        self.inner.get_keys(keybinding)
    }

    /// Whether `data` matches the keys bound to `keybinding`.
    pub fn matches(&self, data: impl Into<JsString>, keybinding: impl Into<JsString>) -> bool {
        self.inner.matches(data, keybinding)
    }

    pub fn get_conflicts(&self) -> Vec<pi_tui::keybindings::KeybindingConflict> {
        self.inner.get_conflicts()
    }

    fn load_from_file(path: &JsString) -> KeybindingsConfig {
        let Some(raw_config) = load_raw_config(path) else {
            return KeybindingsConfig::new();
        };
        to_keybindings_config(&migrate_keybindings_config(&raw_config).config)
    }
}
