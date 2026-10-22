#[path = "support/oracle.rs"]
mod oracle;

use std::{cell::RefCell, rc::Rc};

use pi_coding_agent::modes::interactive::components::{
    extension_editor::{ExtensionEditorComponent, ExtensionEditorHost},
    extension_input::{ExtensionInputComponent, ExtensionInputOptions},
    extension_selector::{ExtensionSelectorComponent, ExtensionSelectorOptions},
};
use pi_js_compat::js_json::{JsString, parse_lossless};
use pi_tui::tui::Component;
use serde_json::json;

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
    fn suspend(&self) {
        panic!("external editor not requested")
    }
    fn resume(&self) {
        panic!("external editor not requested")
    }
    fn request_forced_render(&self) {}
}

#[test]
fn extension_editor_preserves_raw_prefill_and_submission() {
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
            Default::default(),
            Some("unused".into()),
        );
        assert!(editor.render(40).iter().any(|line| line.contains(&raw)));
        editor.handle_input(&"\r".into());
        assert_eq!(*submitted.borrow(), vec![raw]);
    }
}

#[test]
fn native_extension_dialog_rendering_and_callbacks_match_frozen_source() {
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
