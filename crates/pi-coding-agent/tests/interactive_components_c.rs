//! Behavioral tests for interactive components owned by ca-interactive-c.

use std::sync::{Arc, OnceLock};

use pi_agent_core::harness::messages::{BranchSummaryMessage, BranchSummaryRole};
use pi_ai::types::{
    AssistantContent, AssistantMessage, AssistantRole, StopReason, TextContent, ThinkingContent, ToolCall, Usage,
};
use pi_coding_agent::core::keybindings::KeybindingsManager;
use pi_coding_agent::core::messages::{CompactionSummaryMessage, CompactionSummaryRole};
use pi_coding_agent::modes::interactive::components::assistant_message::AssistantMessageComponent;
use pi_coding_agent::modes::interactive::components::bordered_loader::{BorderedLoader, BorderedLoaderOptions};
use pi_coding_agent::modes::interactive::components::branch_summary_message::BranchSummaryMessageComponent;
use pi_coding_agent::modes::interactive::components::compaction_summary_message::CompactionSummaryMessageComponent;
use pi_coding_agent::modes::interactive::components::custom_editor::{CustomEditor, CustomEditorOptions};
use pi_coding_agent::modes::interactive::components::extension_editor::{
    ExtensionEditorComponent, ExtensionEditorHost,
};
use pi_coding_agent::modes::interactive::components::extension_input::{
    ExtensionInputComponent, ExtensionInputOptions,
};
use pi_coding_agent::modes::interactive::components::first_time_setup::{
    FirstTimeSetupComponent, FirstTimeSetupOptions, FirstTimeSetupResult,
};
use pi_coding_agent::modes::interactive::components::status_indicator::{
    CompactionStatusIndicator, CompactionStatusReason, IdleStatus, RetryStatusIndicator, WorkingStatusIndicator,
};
use pi_coding_agent::modes::interactive::components::user_message::UserMessageComponent;
use pi_coding_agent::modes::interactive::theme::{ColorMode, ColorValue, REQUIRED_COLORS, Theme, set_theme_instance};
use pi_coding_agent::modes::interactive::theme::{TerminalTheme, get_editor_theme};
use pi_tui::components::editor::{EditorFixedCell, EditorHost, EditorOptions};
use pi_tui::{Component, MarkdownTheme, strip_terminal_sequences};

const OSC133_ZONE_START: &str = "\u{1b}]133;A\u{7}";
const OSC133_ZONE_END: &str = "\u{1b}]133;B\u{7}";
const OSC133_ZONE_FINAL: &str = "\u{1b}]133;C\u{7}";

fn theme() -> Arc<Theme> {
    static INSTALLED: OnceLock<()> = OnceLock::new();
    INSTALLED.get_or_init(|| {
        pi_tui::set_keybindings(pi_tui::KeybindingsManager::new(
            pi_coding_agent::core::keybindings::keybindings(),
            Vec::new(),
        ));
    });
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

fn plain(lines: &[String]) -> String {
    lines
        .iter()
        .map(|line| strip_terminal_sequences(line))
        .collect::<Vec<_>>()
        .join("\n")
}

struct PlainMarkdownTheme;

impl MarkdownTheme for PlainMarkdownTheme {
    fn heading(&self, text: &str) -> String {
        text.to_string()
    }
    fn link(&self, text: &str) -> String {
        text.to_string()
    }
    fn link_url(&self, text: &str) -> String {
        text.to_string()
    }
    fn code(&self, text: &str) -> String {
        text.to_string()
    }
    fn code_block(&self, text: &str) -> String {
        text.to_string()
    }
    fn code_block_border(&self, text: &str) -> String {
        text.to_string()
    }
    fn quote(&self, text: &str) -> String {
        text.to_string()
    }
    fn quote_border(&self, text: &str) -> String {
        text.to_string()
    }
    fn hr(&self, text: &str) -> String {
        text.to_string()
    }
    fn list_bullet(&self, text: &str) -> String {
        text.to_string()
    }
    fn bold(&self, text: &str) -> String {
        text.to_string()
    }
    fn italic(&self, text: &str) -> String {
        text.to_string()
    }
    fn strikethrough(&self, text: &str) -> String {
        text.to_string()
    }
    fn underline(&self, text: &str) -> String {
        text.to_string()
    }
}

fn markdown_theme() -> std::rc::Rc<dyn MarkdownTheme> {
    std::rc::Rc::new(PlainMarkdownTheme)
}

fn assistant_message(content: Vec<AssistantContent>, stop_reason: StopReason) -> AssistantMessage {
    AssistantMessage {
        role: AssistantRole::Tag,
        content,
        api: "openai-responses".to_owned(),
        provider: "openai".to_owned(),
        model: "gpt-4o-mini".to_owned(),
        usage: Usage::default(),
        stop_reason,
        timestamp: 0,
        response_model: None,
        response_id: None,
        image_generation_result: None,
        diagnostics: None,
        deferred: None,
        error_message: None,
        raw_stop_reason: None,
        end_turn: None,
    }
}

fn text(value: &str) -> AssistantContent {
    AssistantContent::Text(TextContent::new(value))
}

fn thinking(value: &str) -> AssistantContent {
    AssistantContent::Thinking(ThinkingContent::new(value))
}

#[test]
fn branch_summary_collapsed_and_expanded() {
    theme();
    let message = BranchSummaryMessage {
        role: BranchSummaryRole::BranchSummary,
        summary: "the summary".to_owned(),
        from_id: "from".to_owned(),
        timestamp: 0.0,
    };
    let mut component = BranchSummaryMessageComponent::new(message, Some(markdown_theme()));
    let collapsed = plain(&component.render(60));
    assert!(collapsed.contains("[branch]"));
    assert!(collapsed.contains("Branch summary ("));
    assert!(collapsed.contains("to expand)"));

    component.set_expanded(true);
    let expanded = plain(&component.render(60));
    assert!(expanded.contains("Branch Summary"));
    assert!(expanded.contains("the summary"));
    assert!(!expanded.contains("to expand)"));
}

fn compaction(tokens_before: f64) -> CompactionSummaryMessage {
    CompactionSummaryMessage {
        role: CompactionSummaryRole::CompactionSummary,
        summary: "summary".to_owned(),
        tokens_before,
        tokens_after: None,
        kept_from_previous_context_tokens: None,
        compacted_result_tokens: None,
        duration_ms: None,
        provider_native: None,
        timestamp: 0.0,
    }
}

#[test]
fn compaction_summary_shows_duration() {
    theme();
    let mut message = compaction(1234.0);
    message.duration_ms = Some(4567.0);
    let mut component = CompactionSummaryMessageComponent::new(message, Some(markdown_theme()));
    let rendered = plain(&component.render(120));
    assert!(rendered.contains("Compacted from 1.2k tokens in 4.6s"), "{rendered}");
}

#[test]
fn compaction_summary_shows_after_kept_and_remote_result() {
    theme();
    let mut message = compaction(286_705.0);
    message.tokens_after = Some(31_234.0);
    message.kept_from_previous_context_tokens = Some(26_667.0);
    message.compacted_result_tokens = Some(4567.0);
    let mut component = CompactionSummaryMessageComponent::new(message, Some(markdown_theme()));
    let rendered = plain(&component.render(140));
    assert!(rendered.contains("Compacted from 286.7k to 31.2k tokens"), "{rendered}");
    assert!(rendered.contains("kept 26.7k tokens"), "{rendered}");
    assert!(rendered.contains("remote result 4.6k tokens"), "{rendered}");
    assert!(!rendered.contains("saved"), "{rendered}");
}

#[test]
fn assistant_adds_osc_zone_markers_without_tool_calls() {
    theme();
    let component = AssistantMessageComponent::new(
        Some(assistant_message(vec![text("hello")], StopReason::Stop)),
        false,
        Some(markdown_theme()),
        None,
        1,
        Vec::new(),
    );
    let mut component = component;
    let lines = component.render(40);
    assert!(!lines.is_empty());
    assert!(lines[0].contains(OSC133_ZONE_START));
    assert!(
        lines[lines.len() - 1].starts_with(&format!("{OSC133_ZONE_END}{OSC133_ZONE_FINAL}")),
        "{:?}",
        lines[lines.len() - 1]
    );
}

#[test]
fn assistant_skips_osc_markers_with_tool_calls() {
    theme();
    let message = assistant_message(
        vec![
            text("calling tool"),
            AssistantContent::ToolCall(ToolCall::new(
                "tool-1",
                "read",
                serde_json::json!({"path": "file.txt"})
                    .as_object()
                    .cloned()
                    .expect("object"),
            )),
        ],
        StopReason::ToolUse,
    );
    let mut component =
        AssistantMessageComponent::new(Some(message), false, Some(markdown_theme()), None, 1, Vec::new());
    let rendered = component.render(60).join("\n");
    assert!(!rendered.contains(OSC133_ZONE_START));
    assert!(!rendered.contains(OSC133_ZONE_END));
    assert!(!rendered.contains(OSC133_ZONE_FINAL));
}

#[test]
fn assistant_renders_length_stop_with_hidden_thinking_label() {
    theme();
    let message = assistant_message(vec![thinking("private reasoning")], StopReason::Length);
    let mut component =
        AssistantMessageComponent::new(Some(message), true, Some(markdown_theme()), None, 1, Vec::new());
    let rendered = component.render(80).join("\n");
    assert!(rendered.contains("Thinking..."));
    assert!(rendered.contains("Response was truncated before completion."));
}

#[test]
fn assistant_coalesces_adjacent_thinking_blocks_into_one_label() {
    theme();
    let message = assistant_message(
        vec![
            thinking("first thought"),
            thinking(""),
            thinking("second thought"),
            text("answer"),
        ],
        StopReason::Stop,
    );
    let mut component =
        AssistantMessageComponent::new(Some(message), true, Some(markdown_theme()), None, 1, Vec::new());
    let rendered = plain(&component.render(80));
    assert_eq!(rendered.matches("Thinking...").count(), 1);
    assert!(rendered.contains("answer"));
}

#[test]
fn assistant_uses_configured_output_padding() {
    theme();
    let message = assistant_message(vec![text("hello"), thinking("reasoning")], StopReason::Stop);
    let mut component =
        AssistantMessageComponent::new(Some(message), false, Some(markdown_theme()), None, 1, Vec::new());
    let lines: Vec<String> = component
        .render(80)
        .iter()
        .map(|line| strip_terminal_sequences(line))
        .collect();
    assert!(lines.iter().any(|line| line.contains(" hello")), "{lines:?}");
    assert!(lines.iter().any(|line| line.contains(" reasoning")), "{lines:?}");

    component.set_output_pad(0);
    let lines: Vec<String> = component
        .render(80)
        .iter()
        .map(|line| strip_terminal_sequences(line))
        .collect();
    assert!(lines.iter().any(|line| line.starts_with("hello")), "{lines:?}");
    assert!(lines.iter().any(|line| line.starts_with("reasoning")), "{lines:?}");
}

#[test]
fn assistant_chains_markdown_transformers_in_registration_order() {
    use std::sync::Mutex;

    theme();
    let calls: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));
    let first = calls.clone();
    let second = calls.clone();
    let transformers: Vec<pi_coding_agent::core::extensions::types::MarkdownTransformer> = vec![
        Arc::new(move |markdown: &str, context| {
            assert_eq!(
                context.message_type,
                pi_coding_agent::core::extensions::types::MarkdownMessageType::Assistant
            );
            assert!(!context.is_streaming);
            assert_eq!(context.available_width, 78.0);
            first.lock().unwrap().push("formula");
            markdown.replace("$x^2$", "x\u{b2}")
        }),
        Arc::new(move |markdown: &str, _context| {
            second.lock().unwrap().push("suffix");
            format!("{markdown} Done.")
        }),
    ];
    let message = assistant_message(vec![text("The result is $x^2$.")], StopReason::Stop);
    let mut component =
        AssistantMessageComponent::new(Some(message), false, Some(markdown_theme()), None, 1, transformers);
    let rendered = plain(&component.render(80));
    assert!(rendered.contains("The result is x\u{b2}. Done."), "{rendered}");
    assert_eq!(*calls.lock().unwrap(), vec!["formula", "suffix"]);
}

#[test]
fn assistant_identifies_partial_markdown_as_streaming() {
    use std::sync::Mutex;

    theme();
    let streaming_states: Arc<Mutex<Vec<bool>>> = Arc::new(Mutex::new(Vec::new()));
    let states = streaming_states.clone();
    let transformers: Vec<pi_coding_agent::core::extensions::types::MarkdownTransformer> =
        vec![Arc::new(move |markdown: &str, context| {
            states.lock().unwrap().push(context.is_streaming);
            if context.is_streaming {
                markdown.to_owned()
            } else {
                format!("{markdown} transformed")
            }
        })];
    let message = assistant_message(vec![text("partial")], StopReason::Stop);
    let mut component = AssistantMessageComponent::new(None, false, Some(markdown_theme()), None, 1, transformers);

    component.update_content(message.clone(), true);
    assert!(!plain(&component.render(80)).contains("transformed"));

    component.update_content(message, false);
    assert!(plain(&component.render(80)).contains("partial transformed"));
    assert_eq!(*streaming_states.lock().unwrap(), vec![true, false]);
}

#[test]
fn assistant_reapplies_transformers_when_width_changes() {
    use std::sync::Mutex;

    theme();
    let widths: Arc<Mutex<Vec<f64>>> = Arc::new(Mutex::new(Vec::new()));
    let recorded = widths.clone();
    let transformers: Vec<pi_coding_agent::core::extensions::types::MarkdownTransformer> =
        vec![Arc::new(move |markdown: &str, context| {
            recorded.lock().unwrap().push(context.available_width);
            format!("{markdown} ({})", context.available_width)
        })];
    let message = assistant_message(vec![text("answer")], StopReason::Stop);
    let mut component =
        AssistantMessageComponent::new(Some(message), false, Some(markdown_theme()), None, 1, transformers);

    assert!(plain(&component.render(80)).contains("answer (78)"));
    component.render(80);
    assert!(plain(&component.render(60)).contains("answer (58)"));
    assert_eq!(*widths.lock().unwrap(), vec![78.0, 58.0]);
}

#[test]
fn user_message_keeps_height_and_zone_markers() {
    theme();
    let mut component = UserMessageComponent::new("hello", Some(markdown_theme()), 1, Vec::new());
    let lines = component.render(20);
    assert_eq!(lines.len(), 3, "{lines:?}");
    assert!(lines[0].contains(OSC133_ZONE_START));
    assert!(lines[0].ends_with("\u{1b}[49m"), "{:?}", lines[0]);
    assert!(!lines[0].contains(OSC133_ZONE_END));
    assert!(lines[1].contains("hello"));
    assert!(lines[2].starts_with(&format!("{OSC133_ZONE_END}{OSC133_ZONE_FINAL}")));
    assert!(lines[2].ends_with("\u{1b}[49m"), "{:?}", lines[2]);
}

#[test]
fn user_message_chains_transformers_with_user_context() {
    theme();
    let transformers: Vec<pi_coding_agent::core::extensions::types::MarkdownTransformer> = vec![
        Arc::new(|markdown: &str, context| {
            assert_eq!(
                context.message_type,
                pi_coding_agent::core::extensions::types::MarkdownMessageType::User
            );
            assert!(!context.is_streaming);
            assert_eq!(context.available_width, 78.0);
            markdown.replace("$x^2$", "x\u{b2}")
        }),
        Arc::new(|markdown: &str, _context| format!("{markdown} Done.")),
    ];
    let mut component = UserMessageComponent::new("The input is $x^2$.", Some(markdown_theme()), 1, transformers);
    let rendered = plain(&component.render(80));
    assert!(rendered.contains("The input is x\u{b2}. Done."), "{rendered}");
}

#[test]
fn user_message_reapplies_transformers_when_invalidated() {
    use std::sync::Mutex;

    theme();
    let suffix = Arc::new(Mutex::new("before"));
    let captured = suffix.clone();
    let transformers: Vec<pi_coding_agent::core::extensions::types::MarkdownTransformer> =
        vec![Arc::new(move |markdown: &str, _context| {
            format!("{markdown} {}", captured.lock().unwrap())
        })];
    let mut component = UserMessageComponent::new("Message", Some(markdown_theme()), 1, transformers);
    assert!(plain(&component.render(80)).contains("Message before"));

    *suffix.lock().unwrap() = "after";
    component.invalidate();
    assert!(plain(&component.render(80)).contains("Message after"));
}

#[test]
fn user_message_uses_configured_output_padding() {
    theme();
    let mut padded = UserMessageComponent::new("hello", Some(markdown_theme()), 1, Vec::new());
    let lines: Vec<String> = padded
        .render(40)
        .iter()
        .map(|line| strip_terminal_sequences(line))
        .collect();
    assert!(lines.iter().any(|line| line.starts_with(" hello")), "{lines:?}");

    let mut unpadded = UserMessageComponent::new("hello", Some(markdown_theme()), 0, Vec::new());
    let lines: Vec<String> = unpadded
        .render(40)
        .iter()
        .map(|line| strip_terminal_sequences(line))
        .collect();
    assert!(lines.iter().any(|line| line.starts_with("hello")), "{lines:?}");
}

struct NoopCell;

impl EditorFixedCell for NoopCell {
    fn place(&self, _row: i64, _col: i64) {}
    fn clear(&self) {}
    fn update(&self, _value: &str) -> bool {
        true
    }
}

struct TestHost;

impl EditorHost for TestHost {
    fn terminal_rows(&self) -> usize {
        24
    }

    fn request_render(&self) {}

    fn create_fixed_cell(&self) -> std::rc::Rc<dyn EditorFixedCell> {
        std::rc::Rc::new(NoopCell)
    }
}

impl ExtensionEditorHost for TestHost {
    fn suspend(&self) {}
    fn resume(&self) {}
    fn request_forced_render(&self) {}
}

fn app_keybindings() -> KeybindingsManager {
    KeybindingsManager::new(Default::default(), None)
}

#[test]
fn bordered_loader_renders_message_without_animation() {
    use std::cell::Cell;

    theme();
    let renders = std::rc::Rc::new(Cell::new(0));
    let counter = renders.clone();
    let mut loader = BorderedLoader::new(
        std::rc::Rc::new(move || counter.set(counter.get() + 1)),
        "Working...",
        BorderedLoaderOptions::default(),
    );
    let calls_after_construction = renders.get();
    assert_eq!(calls_after_construction, 1);
    assert!(plain(&loader.render(80)).contains("Working..."));
    assert!(plain(&loader.render(80)).contains("cancel"));
    assert_eq!(renders.get(), calls_after_construction);
    loader.dispose();
}

#[test]
fn bordered_loader_cancels_on_cancel_key() {
    use std::cell::Cell;

    theme();
    let aborted = std::rc::Rc::new(Cell::new(false));
    let flag = aborted.clone();
    let mut loader = BorderedLoader::new(std::rc::Rc::new(|| {}), "Working...", BorderedLoaderOptions::default());
    loader.set_on_abort(Some(std::rc::Rc::new(move || flag.set(true))));
    loader.handle_input("\u{1b}");
    assert!(aborted.get());
    assert!(loader.signal().is_cancelled());
    loader.dispose();
}

#[test]
fn idle_status_keeps_two_blank_lines() {
    let mut idle = IdleStatus;
    assert_eq!(idle.render(20), vec![" ".repeat(20), " ".repeat(20)]);
}

#[test]
fn working_status_keeps_configured_frames_static() {
    use std::cell::Cell;

    theme();
    let renders = std::rc::Rc::new(Cell::new(0));
    let counter = renders.clone();
    let mut indicator = WorkingStatusIndicator::new(
        std::rc::Rc::new(move || counter.set(counter.get() + 1)),
        "Working...",
        Some(
            pi_coding_agent::modes::interactive::components::status_indicator::WorkingIndicatorOptions {
                frames: Some(vec!["a".to_owned(), "b".to_owned()]),
                interval_ms: Some(250),
            },
        ),
    );
    let calls_after_construction = renders.get();
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert_eq!(renders.get(), calls_after_construction);
    assert!(plain(&indicator.render(40)).contains("Working..."));
    indicator.dispose();
}

#[test]
fn retry_status_formats_countdown_and_disposes() {
    use std::cell::Cell;

    theme();
    let renders = std::rc::Rc::new(Cell::new(0));
    let counter = renders.clone();
    let mut indicator = RetryStatusIndicator::new(std::rc::Rc::new(move || counter.set(counter.get() + 1)), 1, 3, 2500);
    let rendered = plain(&indicator.render(80));
    assert!(rendered.contains("Retrying (1/3) in 3s..."), "{rendered}");
    let calls_before_dispose = renders.get();
    indicator.dispose();
    let deadline = indicator.next_countdown_deadline();
    assert!(deadline.is_none());
    assert_eq!(renders.get(), calls_before_dispose);
}

#[test]
fn compaction_status_uses_reason_label() {
    theme();
    let mut manual = CompactionStatusIndicator::new(std::rc::Rc::new(|| {}), CompactionStatusReason::Manual);
    assert!(plain(&manual.render(100)).contains("Compacting context..."));
    let mut overflow = CompactionStatusIndicator::new(std::rc::Rc::new(|| {}), CompactionStatusReason::Overflow);
    let rendered = plain(&overflow.render(100));
    assert!(
        rendered.contains("Context overflow detected, Auto-compacting..."),
        "{rendered}"
    );
    manual.dispose();
    overflow.dispose();
}

#[test]
fn extension_input_submits_and_cancels() {
    use std::cell::RefCell;

    theme();
    let submitted: std::rc::Rc<RefCell<Vec<String>>> = std::rc::Rc::new(RefCell::new(Vec::new()));
    let cancelled = std::rc::Rc::new(std::cell::Cell::new(false));
    let submit = submitted.clone();
    let cancel = cancelled.clone();
    let mut input = ExtensionInputComponent::new(
        "Name",
        None,
        std::rc::Rc::new(move |value| submit.borrow_mut().push(value)),
        std::rc::Rc::new(move || cancel.set(true)),
        ExtensionInputOptions::default(),
    );
    for character in ["a", "b", "c"] {
        input.handle_input(character);
    }
    assert!(plain(&input.render(60)).contains("Name"));
    input.handle_input("\r");
    assert_eq!(*submitted.borrow(), vec!["abc"]);

    input.handle_input("\u{1b}");
    assert!(cancelled.get());
}

#[test]
fn extension_input_masks_secret_values() {
    theme();
    let mut input = ExtensionInputComponent::new(
        "Secret",
        None,
        std::rc::Rc::new(|_| {}),
        std::rc::Rc::new(|| {}),
        ExtensionInputOptions {
            secret: true,
            ..Default::default()
        },
    );
    for character in ["s", "e", "c"] {
        input.handle_input(character);
    }
    let rendered = plain(&input.render(60));
    assert!(!rendered.contains("sec"), "{rendered}");
}

#[test]
fn extension_editor_submits_typed_text() {
    use std::cell::RefCell;

    theme();
    let submitted: std::rc::Rc<RefCell<Vec<String>>> = std::rc::Rc::new(RefCell::new(Vec::new()));
    let submit = submitted.clone();
    let host: std::rc::Rc<dyn ExtensionEditorHost> = std::rc::Rc::new(TestHost);
    let mut editor = ExtensionEditorComponent::new(
        host,
        app_keybindings(),
        "Compose",
        Some("prefill"),
        std::rc::Rc::new(move |value| submit.borrow_mut().push(value)),
        std::rc::Rc::new(|| {}),
        EditorOptions::default(),
        None,
    );
    let rendered = plain(&editor.render(80));
    assert!(rendered.contains("Compose"));
    assert!(rendered.contains("prefill"));
    assert!(rendered.contains("submit"));
    editor.handle_input("\r");
    assert_eq!(*submitted.borrow(), vec!["prefill"]);
}

#[test]
fn extension_editor_cancels_on_escape() {
    use std::cell::Cell;

    theme();
    let cancelled = std::rc::Rc::new(Cell::new(false));
    let flag = cancelled.clone();
    let host: std::rc::Rc<dyn ExtensionEditorHost> = std::rc::Rc::new(TestHost);
    let mut editor = ExtensionEditorComponent::new(
        host,
        app_keybindings(),
        "Compose",
        None,
        std::rc::Rc::new(|_| {}),
        std::rc::Rc::new(move || flag.set(true)),
        EditorOptions::default(),
        None,
    );
    editor.handle_input("\u{1b}");
    assert!(cancelled.get());
}

#[test]
fn first_time_setup_walks_theme_then_analytics() {
    use std::cell::RefCell;

    theme();
    let previews: std::rc::Rc<RefCell<Vec<TerminalTheme>>> = std::rc::Rc::new(RefCell::new(Vec::new()));
    let submitted: std::rc::Rc<RefCell<Vec<FirstTimeSetupResult>>> = std::rc::Rc::new(RefCell::new(Vec::new()));
    let preview = previews.clone();
    let submit = submitted.clone();
    let mut setup = FirstTimeSetupComponent::new(FirstTimeSetupOptions {
        detected_theme: TerminalTheme::Dark,
        on_theme_preview: std::rc::Rc::new(move |theme| preview.borrow_mut().push(theme)),
        on_submit: std::rc::Rc::new(move |result| submit.borrow_mut().push(result)),
        on_cancel: std::rc::Rc::new(|| {}),
    });
    let first = plain(&setup.render(100));
    assert!(first.contains("Welcome to pi, the minimal coding agent."), "{first}");
    assert!(first.contains("Pick a theme."), "{first}");
    assert!(first.contains("Detected system appearance: dark"), "{first}");

    setup.handle_input("j");
    assert_eq!(*previews.borrow(), vec![TerminalTheme::Light]);
    let second = plain(&setup.render(100));
    assert!(second.contains("Detected system appearance: dark"), "{second}");

    setup.handle_input("\r");
    let analytics = plain(&setup.render(100));
    assert!(
        analytics.contains("Opt-in to anonymous usage data sharing?"),
        "{analytics}"
    );

    setup.handle_input("j");
    setup.handle_input("\r");
    assert_eq!(
        *submitted.borrow(),
        vec![FirstTimeSetupResult {
            theme: TerminalTheme::Light,
            share_analytics: false,
        }]
    );
}

#[test]
fn first_time_setup_cancel_skips() {
    use std::cell::Cell;

    theme();
    let cancelled = std::rc::Rc::new(Cell::new(false));
    let flag = cancelled.clone();
    let mut setup = FirstTimeSetupComponent::new(FirstTimeSetupOptions {
        detected_theme: TerminalTheme::Light,
        on_theme_preview: std::rc::Rc::new(|_| {}),
        on_submit: std::rc::Rc::new(|_| {}),
        on_cancel: std::rc::Rc::new(move || flag.set(true)),
    });
    setup.handle_input("\u{1b}");
    assert!(cancelled.get());
}

#[test]
fn custom_editor_routes_app_actions_and_interrupt() {
    use std::cell::Cell;

    theme();
    let interrupted = std::rc::Rc::new(Cell::new(false));
    let flag = interrupted.clone();
    let mut editor = CustomEditor::new(
        std::rc::Rc::new(TestHost),
        get_editor_theme(),
        app_keybindings(),
        CustomEditorOptions::default(),
    )
    .expect("editor");
    editor.on_action(
        "app.interrupt",
        std::rc::Rc::new(move || {
            flag.set(true);
            true
        }),
    );
    editor.handle_input("\u{1b}");
    assert!(interrupted.get());
}

#[test]
fn custom_editor_exits_only_when_empty() {
    use std::cell::Cell;

    theme();
    let exited = std::rc::Rc::new(Cell::new(0));
    let counter = exited.clone();
    let mut editor = CustomEditor::new(
        std::rc::Rc::new(TestHost),
        get_editor_theme(),
        app_keybindings(),
        CustomEditorOptions::default(),
    )
    .expect("editor");
    editor.on_action(
        "app.exit",
        std::rc::Rc::new(move || {
            counter.set(counter.get() + 1);
            true
        }),
    );
    editor.set_text("content");
    editor.handle_input("\u{4}");
    assert_eq!(exited.get(), 0);

    editor.set_text("");
    editor.handle_input("\u{4}");
    assert_eq!(exited.get(), 1);
}

#[test]
fn custom_editor_persists_history_through_the_control_db() {
    theme();
    let directory = tempfile::tempdir().expect("temp dir");
    let control_db = directory.path().join("control.db");
    let control_db = control_db.to_string_lossy().into_owned();
    let legacy = directory.path().join("prompt-history.json");
    std::fs::write(&legacy, r#"["legacy one", "legacy two", 7]"#).expect("legacy history");

    let mut editor = CustomEditor::new(
        std::rc::Rc::new(TestHost),
        get_editor_theme(),
        app_keybindings(),
        CustomEditorOptions {
            legacy_prompt_history_path: Some(legacy.to_string_lossy().into_owned()),
            prompt_history_control_db_path: Some(control_db.clone()),
            ..Default::default()
        },
    )
    .expect("editor");
    assert_eq!(editor.get_history(), vec!["legacy one", "legacy two"]);

    editor.add_to_history("newest").expect("record history");
    assert_eq!(editor.get_history(), vec!["newest", "legacy one", "legacy two"]);

    let reopened = CustomEditor::new(
        std::rc::Rc::new(TestHost),
        get_editor_theme(),
        app_keybindings(),
        CustomEditorOptions {
            prompt_history_control_db_path: Some(control_db),
            ..Default::default()
        },
    )
    .expect("reopened editor");
    assert_eq!(reopened.get_history().first().map(String::as_str), Some("newest"));
}

#[test]
fn custom_editor_filters_legacy_history() {
    theme();
    let directory = tempfile::tempdir().expect("temp dir");
    let control_db = directory.path().join("control.db");
    let legacy = directory.path().join("prompt-history.json");
    std::fs::write(&legacy, r#"["keep", "drop"]"#).expect("legacy history");

    let editor = CustomEditor::new(
        std::rc::Rc::new(TestHost),
        get_editor_theme(),
        app_keybindings(),
        CustomEditorOptions {
            legacy_prompt_history_path: Some(legacy.to_string_lossy().into_owned()),
            prompt_history_control_db_path: Some(control_db.to_string_lossy().into_owned()),
            prompt_history_filter: Some(std::rc::Rc::new(|entry: &str| entry == "keep")),
            ..Default::default()
        },
    )
    .expect("editor");
    assert_eq!(editor.get_history(), vec!["keep"]);
}
