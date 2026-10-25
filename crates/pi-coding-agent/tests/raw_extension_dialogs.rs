use crate::shared_oracle as oracle;

use std::{cell::RefCell, rc::Rc};

use pi_coding_agent::modes::interactive::components::{
    custom_editor::{CustomEditor, CustomEditorOptions},
    extension_editor::{ExtensionEditorComponent, ExtensionEditorHost},
    extension_input::{ExtensionInputComponent, ExtensionInputOptions},
    extension_selector::{ExtensionSelectorComponent, ExtensionSelectorOptions},
    settings_selector::SelectSubmenu,
};
use pi_js_compat::js_json::{JsString, parse_lossless};
use pi_tui::tui::Component;
use serde_json::json;

fn initialize_theme() {
    use pi_coding_agent::modes::interactive::theme::{
        ColorMode, ColorValue, REQUIRED_COLORS, Theme, set_theme_instance,
    };
    let colors: Vec<_> = REQUIRED_COLORS
        .iter()
        .map(|name| (JsString::from(*name), ColorValue::Index(0.0)))
        .collect();
    let theme = Theme::new(&colors, &colors, ColorMode::Color256, None, None).unwrap();
    set_theme_instance(std::sync::Arc::new(theme));
}

struct EditorHost;

impl pi_tui::components::editor::EditorHost for EditorHost {
    fn terminal_rows(&self) -> usize {
        24
    }
    fn request_render(&self) {}
    fn create_fixed_cell(&self) -> Rc<dyn pi_tui::components::editor::EditorFixedCell> {
        Rc::new(Self)
    }
}
impl pi_tui::components::editor::EditorFixedCell for EditorHost {
    fn place(&self, _: i64, _: i64) {}
    fn clear(&self) {}
    fn update(&self, _: &JsString) -> bool {
        false
    }
}
impl ExtensionEditorHost for EditorHost {
    fn suspend(&self) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        panic!("external editor not requested")
    }
    fn resume(&self) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        panic!("external editor not requested")
    }
    fn request_forced_render(&self) {}
}

#[test]
fn settings_submenu_retains_raw_title_options_and_selection_callback() {
    initialize_theme();
    for raw in [
        JsString::from_utf16(vec![0xd800]),
        JsString::from_utf16(vec![0xdc00]),
        JsString::from("😀"),
    ] {
        let values = Rc::new(RefCell::new(Vec::new()));
        let selected = Rc::clone(&values);
        let mut submenu = SelectSubmenu::new(
            &raw,
            &raw,
            vec![pi_tui::SelectItem::new(&raw, &raw)],
            &raw,
            Rc::new(move |value| selected.borrow_mut().push(value.clone())),
            Rc::new(|| {}),
            None,
        );
        assert!(submenu.render(40).iter().any(|line| line.contains(&raw)));
        submenu.handle_input(&"\r".into());
        assert_eq!(*values.borrow(), vec![raw]);
    }
}

#[test]
fn custom_editor_native_history_input_and_shortcuts_match_frozen_source() {
    initialize_theme();
    let mut oracle = oracle::TsOracle::spawn("raw-coding-tui-oracle.ts", "packages/coding-agent/src");
    for raw in [
        JsString::from_utf16(vec![0xd800]),
        JsString::from_utf16(vec![0xdc00]),
        JsString::from("😀"),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let expected =
            oracle.request(&json!({"operation":"custom-editor","units":raw.units(),"directory":directory.path()}));
        assert_eq!(expected["ok"], true, "{expected}");
        let mut editor = CustomEditor::new(
            Rc::new(EditorHost),
            pi_tui::components::editor::EditorTheme::plain(),
            pi_coding_agent::core::keybindings::KeybindingsManager::new(Default::default(), None),
            CustomEditorOptions::default(),
        )
        .unwrap();
        editor.set_text(&raw);
        editor.add_to_history_without_persistence(&raw);
        let seen = Rc::new(RefCell::new(Vec::new()));
        let shortcut_seen = Rc::clone(&seen);
        editor.on_extension_shortcut = Some(Rc::new(move |data| {
            shortcut_seen.borrow_mut().push(data.clone());
            false
        }));
        editor.handle_input(&raw);
        let actual = serde_json::to_string(&(editor.get_text(), editor.get_history(), seen.borrow().clone())).unwrap();
        assert_eq!(
            parse_lossless(actual).unwrap(),
            parse_lossless(expected["json"].as_str().unwrap()).unwrap()
        );
    }
}

#[test]
fn extension_editor_preserves_raw_prefill_and_submission() {
    initialize_theme();
    for raw in [
        JsString::from_utf16(vec![0xd800]),
        JsString::from_utf16(vec![0xdc00]),
        JsString::from("😀"),
    ] {
        let submitted = Rc::new(RefCell::new(Vec::new()));
        let output = Rc::clone(&submitted);
        let mut editor = ExtensionEditorComponent::new(
            Rc::new(EditorHost),
            pi_coding_agent::core::keybindings::KeybindingsManager::new(Default::default(), None),
            &raw,
            Some(&raw),
            Rc::new(move |value| output.borrow_mut().push(value)),
            Rc::new(|| {}),
            pi_coding_agent::modes::interactive::components::extension_editor::ExtensionEditorOptions {
                editor: Default::default(),
                external_editor_command: Some("unused".into()),
            },
        );
        assert!(editor.render(40).iter().any(|line| line.contains(&raw)));
        editor.handle_input(&"\r".into());
        assert_eq!(*submitted.borrow(), vec![raw]);
    }
}

#[test]
fn native_extension_dialog_rendering_and_callbacks_match_frozen_source() {
    initialize_theme();
    let mut oracle = oracle::TsOracle::spawn("raw-coding-tui-oracle.ts", "packages/coding-agent/src");
    for raw in [
        JsString::from_utf16(vec![0xd800]),
        JsString::from_utf16(vec![0xdc00]),
        JsString::from("😀"),
    ] {
        let expected = oracle.request(&json!({"operation":"dialogs", "units":raw.units()}));
        assert_eq!(expected["ok"], true, "{expected}");
        let submitted = Rc::new(RefCell::new(Vec::new()));
        let selected = Rc::clone(&submitted);
        let mut selector = ExtensionSelectorComponent::new(
            &raw,
            vec![raw.clone(), raw.clone() + "next"],
            Rc::new(move |value| selected.borrow_mut().push(value)),
            Rc::new(|| {}),
            ExtensionSelectorOptions::default(),
        );
        let input_result = Rc::clone(&submitted);
        let mut input = ExtensionInputComponent::new(
            &raw,
            None,
            Rc::new(move |value| input_result.borrow_mut().push(value)),
            Rc::new(|| {}),
            ExtensionInputOptions::default(),
        );
        input.handle_input(&raw);
        let plain = |lines: Vec<JsString>| {
            lines
                .into_iter()
                .map(pi_tui::utils::strip_terminal_sequences)
                .collect::<Vec<_>>()
        };
        let mut snapshots = Vec::new();
        for width in [4, 12, 40] {
            snapshots.push(vec![plain(selector.render(width)), plain(input.render(width))]);
            selector.invalidate();
            input.invalidate();
            snapshots.push(vec![plain(selector.render(width)), plain(input.render(width))]);
        }
        selector.handle_input(&JsString::from("j"));
        selector.handle_input(&JsString::from("\r"));
        input.handle_input(&JsString::from("\r"));
        let actual = serde_json::to_string(&(snapshots, submitted.borrow().clone())).unwrap();
        assert_eq!(
            parse_lossless(actual).unwrap(),
            parse_lossless(expected["json"].as_str().unwrap()).unwrap()
        );
    }
}

#[test]
fn extension_dialogs_preserve_native_titles_options_and_submitted_values() {
    initialize_theme();
    for raw in [
        JsString::from_utf16(vec![0xd800]),
        JsString::from_utf16(vec![0xdc00]),
        JsString::from("😀"),
    ] {
        let submitted = Rc::new(RefCell::new(Vec::new()));
        let selected = Rc::clone(&submitted);
        let mut selector = ExtensionSelectorComponent::new(
            &raw,
            vec![raw.clone()],
            Rc::new(move |value| selected.borrow_mut().push(value)),
            Rc::new(|| {}),
            ExtensionSelectorOptions::default(),
        );
        assert_eq!(selector.base_title(), &raw);
        assert!(selector.render(40).iter().any(|line| line.contains(&raw)));
        selector.handle_input(&JsString::from("\r"));
        assert_eq!(*submitted.borrow(), vec![raw.clone()]);
        selector.invalidate();
        assert!(selector.render(40).iter().any(|line| line.contains(&raw)));

        let input_result = Rc::clone(&submitted);
        let mut input = ExtensionInputComponent::new(
            &raw,
            None,
            Rc::new(move |value| input_result.borrow_mut().push(value)),
            Rc::new(|| {}),
            ExtensionInputOptions::default(),
        );
        assert_eq!(input.base_title(), &raw);
        input.handle_input(&raw);
        input.handle_input(&JsString::from("\r"));
        assert_eq!(*submitted.borrow(), vec![raw.clone(), raw.clone()]);
        assert!(input.render(40).iter().any(|line| line.contains(&raw)));
    }
}
