//! Behavioral tests for `modes/interactive/components/login-dialog.ts`.
//! Ports `test/suite/regressions/5433-extension-oauth-prompt-input.test.ts`.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use pi_coding_agent::modes::interactive::components::login_dialog::{
    LoginDialogComponent, LoginDialogUi, LoginError, LoginInputResultFn,
};
use pi_coding_agent::modes::interactive::theme::{ColorMode, ColorValue, REQUIRED_COLORS, Theme, set_theme_instance};
use pi_tui::{Component, KeybindingsManager, set_keybindings, strip_terminal_sequences};

struct TestUi {
    renders: RefCell<usize>,
}

impl LoginDialogUi for TestUi {
    fn request_render(&self) {
        *self.renders.borrow_mut() += 1;
    }
}

fn install_theme() -> Arc<Theme> {
    let colors: Vec<(String, ColorValue)> = REQUIRED_COLORS
        .iter()
        .enumerate()
        .map(|(index, key)| {
            (
                key.to_string(),
                ColorValue::Text(format!(
                    "#{:02x}{:02x}{:02x}",
                    (16 + 7 * index) % 256,
                    (32 + 11 * index) % 256,
                    (48 + 13 * index) % 256
                )),
            )
        })
        .collect();
    let theme = Arc::new(Theme::new(&colors, &colors, ColorMode::Truecolor, None, None).expect("test theme"));
    set_theme_instance(theme.clone());
    theme
}

fn create_dialog(
    opened: Rc<RefCell<Vec<String>>>,
    completed: Rc<RefCell<Vec<(bool, Option<String>)>>>,
) -> (LoginDialogComponent, Rc<TestUi>) {
    let ui = Rc::new(TestUi {
        renders: RefCell::new(0),
    });
    set_keybindings(KeybindingsManager::new(
        pi_coding_agent::core::keybindings::keybindings(),
        Vec::new(),
    ));
    let dialog = LoginDialogComponent::new(
        ui.clone(),
        "prompt-repro",
        Rc::new(move |success, message| {
            completed.borrow_mut().push((success, message.map(str::to_string)));
        }),
        Some("Prompt Repro"),
        None,
        Rc::new(move |url| opened.borrow_mut().push(url.to_string())),
        install_theme(),
    );
    (dialog, ui)
}

fn render_dialog(dialog: &mut LoginDialogComponent) -> Vec<String> {
    dialog
        .render(120)
        .iter()
        .map(|line| strip_terminal_sequences(line).trim_end().to_string())
        .collect()
}

fn count_rendered_value(lines: &[String], value: &str) -> usize {
    lines.iter().filter(|line| line.trim() == format!("> {value}")).count()
}

fn resolver() -> (LoginInputResultFn, Rc<RefCell<Vec<Result<String, LoginError>>>>) {
    let results = Rc::new(RefCell::new(Vec::new()));
    let captured = results.clone();
    (Rc::new(move |result| captured.borrow_mut().push(result)), results)
}

#[test]
fn keeps_previous_prompt_input_stable_when_a_later_prompt_is_active() {
    let (mut dialog, ui) = create_dialog(Rc::default(), Rc::default());
    let (first_resolver, first_results) = resolver();
    dialog.show_prompt("First prompt:", Some("first-value"), first_resolver);
    dialog.handle_input("first-value");
    dialog.handle_input("\n");
    assert_eq!(*first_results.borrow(), vec![Ok("first-value".to_string())]);

    let (second_resolver, second_results) = resolver();
    dialog.show_prompt("Second prompt:", None, second_resolver);
    dialog.handle_input("second-secret-demo");

    let lines = render_dialog(&mut dialog);
    let output = lines.join("\n");
    assert!(output.contains("First prompt:"), "{output}");
    assert!(output.contains("Second prompt:"), "{output}");
    assert_eq!(count_rendered_value(&lines, "first-value"), 1, "{lines:?}");
    assert_eq!(count_rendered_value(&lines, "second-secret-demo"), 1, "{lines:?}");
    assert!(*ui.renders.borrow() >= 2);

    dialog.handle_input("\n");
    assert_eq!(*second_results.borrow(), vec![Ok("second-secret-demo".to_string())]);
}

#[test]
fn preserves_auth_instructions_when_showing_a_prompt() {
    let opened = Rc::new(RefCell::new(Vec::new()));
    let (mut dialog, _ui) = create_dialog(opened.clone(), Rc::default());
    dialog.show_auth("https://example.invalid/login", Some("Authorize the extension"));
    assert_eq!(*opened.borrow(), vec!["https://example.invalid/login"]);
    let (resolver, _results) = resolver();
    dialog.show_prompt("First prompt:", None, resolver);

    let output = render_dialog(&mut dialog).join("\n");
    assert!(output.contains("https://example.invalid/login"), "{output}");
    assert!(output.contains("Authorize the extension"), "{output}");
    assert!(output.contains("First prompt:"), "{output}");
}

#[test]
fn preserves_neutral_information_and_links_when_showing_a_prompt() {
    let (mut dialog, _ui) = create_dialog(Rc::default(), Rc::default());
    dialog.show_info(
        "Configure credentials outside pi.",
        &[pi_ai::auth::types::AuthInfoLink {
            url: "https://example.invalid/docs".to_string(),
            label: Some("Provider documentation".to_string()),
        }],
        false,
    );
    let (resolver, _results) = resolver();
    dialog.show_prompt("Press Enter to continue:", None, resolver);

    let output = render_dialog(&mut dialog).join("\n");
    assert!(output.contains("Configure credentials outside pi."), "{output}");
    assert!(
        output.contains("Provider documentation: https://example.invalid/docs"),
        "{output}"
    );
    assert!(output.contains("Press Enter to continue:"), "{output}");
}

#[test]
fn preserves_setup_details_when_showing_a_prompt() {
    let (mut dialog, _ui) = create_dialog(Rc::default(), Rc::default());
    dialog.show_details(&["AWS credential setup:".to_string(), "providers.md".to_string()]);
    let (resolver, _results) = resolver();
    dialog.show_prompt("Enter API key:", None, resolver);

    let output = render_dialog(&mut dialog).join("\n");
    assert!(output.contains("AWS credential setup:"), "{output}");
    assert!(output.contains("providers.md"), "{output}");
    assert!(output.contains("Enter API key:"), "{output}");
}

#[test]
fn keeps_previous_manual_input_stable_when_a_later_prompt_is_active() {
    let (mut dialog, _ui) = create_dialog(Rc::default(), Rc::default());
    let (manual_resolver, manual_results) = resolver();
    dialog.show_manual_input("Paste callback URL:", manual_resolver);
    dialog.handle_input("callback-value");
    dialog.handle_input("\n");
    assert_eq!(*manual_results.borrow(), vec![Ok("callback-value".to_string())]);

    let (prompt_resolver, prompt_results) = resolver();
    dialog.show_prompt("Second prompt:", None, prompt_resolver);
    dialog.handle_input("second-secret-demo");

    let lines = render_dialog(&mut dialog);
    let output = lines.join("\n");
    assert!(output.contains("Paste callback URL:"), "{output}");
    assert!(output.contains("Second prompt:"), "{output}");
    assert_eq!(count_rendered_value(&lines, "callback-value"), 1, "{lines:?}");
    assert_eq!(count_rendered_value(&lines, "second-secret-demo"), 1, "{lines:?}");

    dialog.handle_input("\n");
    assert_eq!(*prompt_results.borrow(), vec![Ok("second-secret-demo".to_string())]);
}

#[test]
fn cancel_rejects_the_pending_prompt_and_reports_completion() {
    let completed = Rc::new(RefCell::new(Vec::new()));
    let (mut dialog, _ui) = create_dialog(Rc::default(), completed.clone());
    let (resolver, results) = resolver();
    dialog.show_prompt("Token:", None, resolver);
    dialog.handle_input("abc");
    dialog.handle_input("\u{1b}");

    assert_eq!(
        *results.borrow(),
        vec![Err(LoginError {
            message: "Login cancelled".to_string()
        })]
    );
    assert_eq!(*completed.borrow(), vec![(false, Some("Login cancelled".to_string()))]);
    assert!(dialog.signal().is_cancelled());
}
