//! Behavioral tests for interactive components owned by ca-interactive-c.

use std::sync::{Arc, OnceLock};

use pi_agent_core::agent::AgentMessage;
use pi_agent_core::harness::messages::{BranchSummaryMessage, BranchSummaryRole};
use pi_ai::types::{
    AssistantContent, AssistantMessage, AssistantRole, StopReason, TextContent, ThinkingContent, ToolCall, Usage,
};
use pi_ai::types::{UserContent, UserMessage, UserRole};
use pi_coding_agent::core::extensions::types::{
    CustomMessageContent, CustomMessageValue, EntryRenderOptions, MessageRenderOptions,
};
use pi_coding_agent::core::keybindings::KeybindingsManager;
use pi_coding_agent::core::messages::{CompactionSummaryMessage, CompactionSummaryRole};
use pi_coding_agent::core::session_manager::{
    CompactionEntry, CustomEntry, KnownSessionEntry, ModelChangeEntry, SessionEntry, SessionMessageEntry,
    SessionTreeNode, ThinkingLevelChangeEntry,
};
use pi_coding_agent::modes::interactive::components::armin::ArminComponent;
use pi_coding_agent::modes::interactive::components::assistant_message::AssistantMessageComponent;
use pi_coding_agent::modes::interactive::components::bash_execution::{BashExecutionComponent, BashExecutionOptions};
use pi_coding_agent::modes::interactive::components::bordered_loader::{BorderedLoader, BorderedLoaderOptions};
use pi_coding_agent::modes::interactive::components::branch_summary_message::BranchSummaryMessageComponent;
use pi_coding_agent::modes::interactive::components::compaction_summary_message::CompactionSummaryMessageComponent;
use pi_coding_agent::modes::interactive::components::custom_editor::{CustomEditor, CustomEditorOptions};
use pi_coding_agent::modes::interactive::components::custom_entry::CustomEntryComponent;
use pi_coding_agent::modes::interactive::components::custom_message::CustomMessageComponent;
use pi_coding_agent::modes::interactive::components::daxnuts::DaxnutsComponent;
use pi_coding_agent::modes::interactive::components::extension_editor::{
    ExtensionEditorComponent, ExtensionEditorHost,
};
use pi_coding_agent::modes::interactive::components::extension_input::{
    ExtensionInputComponent, ExtensionInputOptions,
};
use pi_coding_agent::modes::interactive::components::first_time_setup::{
    FirstTimeSetupComponent, FirstTimeSetupOptions, FirstTimeSetupResult,
};
#[path = "support/agent_session_fixture.rs"]
mod agent_session_fixture;

use pi_coding_agent::core::footer_data_provider::{FooterDataProvider, FooterSessionOverride};
use pi_coding_agent::modes::interactive::components::footer::{FooterComponent, format_cwd_for_footer, format_tokens};
use pi_coding_agent::modes::interactive::components::session_selector::{SessionListLoader, SessionSelectorComponent};
use pi_coding_agent::modes::interactive::components::status_indicator::{
    CompactionStatusIndicator, CompactionStatusReason, IdleStatus, RetryStatusIndicator, WorkingStatusIndicator,
};
use pi_coding_agent::modes::interactive::components::tree_selector::{FilterMode, TreeSelectorComponent};
use pi_coding_agent::modes::interactive::components::user_message::UserMessageComponent;
use pi_coding_agent::modes::interactive::theme::{ColorMode, ColorValue, REQUIRED_COLORS, Theme, set_theme_instance};
use pi_coding_agent::modes::interactive::theme::{TerminalTheme, get_editor_theme};
use pi_tui::components::editor::{EditorFixedCell, EditorHost, EditorOptions};
use pi_tui::components::text::Text;
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

#[test]
fn custom_entry_renders_renderer_output_and_tracks_expansion() {
    use std::sync::Mutex;

    theme();
    let seen: Arc<Mutex<Vec<bool>>> = Arc::new(Mutex::new(Vec::new()));
    let record = seen.clone();
    let renderer: pi_coding_agent::core::extensions::types::EntryRenderer =
        Arc::new(move |entry: &CustomEntry, options: &EntryRenderOptions, _theme| {
            record.lock().unwrap().push(options.expanded);
            Some(std::rc::Rc::new(std::cell::RefCell::new(Text::new(
                format!("custom {}", entry.custom_type),
                0,
                0,
            ))))
        });
    let entry = CustomEntry {
        custom_type: "test".to_owned(),
        data: None,
        id: "entry".to_owned(),
        parent_id: None,
        timestamp: "2026-01-01T00:00:00.000Z".to_owned(),
    };
    let mut component = CustomEntryComponent::new(entry, renderer);
    assert!(component.has_content());
    let rendered = plain(&component.render(40));
    assert!(rendered.contains("custom test"), "{rendered}");
    assert_eq!(*seen.lock().unwrap(), vec![false]);

    component.set_expanded(true);
    assert_eq!(*seen.lock().unwrap(), vec![false, true]);
}

#[test]
fn custom_entry_without_renderer_output_has_no_content() {
    theme();
    let renderer: pi_coding_agent::core::extensions::types::EntryRenderer = Arc::new(|_entry, _options, _theme| None);
    let entry = CustomEntry {
        custom_type: "test".to_owned(),
        data: None,
        id: "entry".to_owned(),
        parent_id: None,
        timestamp: "2026-01-01T00:00:00.000Z".to_owned(),
    };
    let mut component = CustomEntryComponent::new(entry, renderer);
    assert!(!component.has_content());
    assert!(component.render(40).iter().all(|line| line.trim().is_empty()));
}

fn custom_message(content: CustomMessageContent) -> CustomMessageValue {
    CustomMessageValue {
        custom_type: "test".to_owned(),
        content,
        display: true,
        details: None,
        timestamp: 0.0,
    }
}

#[test]
fn custom_message_default_rendering_uses_label_and_markdown() {
    theme();
    let mut component = CustomMessageComponent::new(
        custom_message(CustomMessageContent::Text("hello **world**".to_owned())),
        None,
        Some(markdown_theme()),
        1,
    );
    let rendered = plain(&component.render(40));
    assert!(rendered.contains("[test]"), "{rendered}");
    assert!(rendered.contains("hello world"), "{rendered}");
}

#[test]
fn custom_message_renderer_receives_output_padding_and_updates() {
    use std::sync::Mutex;

    theme();
    let options_seen: Arc<Mutex<Vec<MessageRenderOptions>>> = Arc::new(Mutex::new(Vec::new()));
    let record = options_seen.clone();
    let renderer: pi_coding_agent::core::extensions::types::MessageRenderer =
        Arc::new(move |_message, options, _theme| {
            record.lock().unwrap().push(*options);
            Some(std::rc::Rc::new(std::cell::RefCell::new(Text::new(
                "custom",
                options.output_pad as usize,
                0,
            ))))
        });
    let mut component = CustomMessageComponent::new(
        custom_message(CustomMessageContent::Text("custom".to_owned())),
        Some(renderer),
        Some(markdown_theme()),
        1,
    );
    let rendered: Vec<String> = component
        .render(40)
        .iter()
        .map(|line| strip_terminal_sequences(line))
        .collect();
    assert!(rendered.iter().any(|line| line.starts_with(" custom")), "{rendered:?}");
    assert_eq!(options_seen.lock().unwrap()[0].output_pad, 1.0);

    component.set_output_pad(0);
    let rendered: Vec<String> = component
        .render(40)
        .iter()
        .map(|line| strip_terminal_sequences(line))
        .collect();
    assert!(rendered.iter().any(|line| line.starts_with("custom")), "{rendered:?}");
    assert_eq!(options_seen.lock().unwrap().last().unwrap().output_pad, 0.0);
}

#[test]
fn bash_execution_streams_output_and_formats_completion() {
    theme();
    let mut component = BashExecutionComponent::new(
        "echo hi",
        std::rc::Rc::new(|| {}),
        false,
        BashExecutionOptions::default(),
    );
    component.append_output("hel");
    component.append_output("lo\r\nsecond\rthird\n");
    assert_eq!(component.get_output(), "hello\nsecond\nthird\n");
    assert_eq!(component.get_command(), "echo hi");

    let rendered = plain(&component.render(60));
    assert!(rendered.contains("$ echo hi"), "{rendered}");
    assert!(rendered.contains("hello"), "{rendered}");
    assert!(rendered.contains("Running..."), "{rendered}");

    component.set_complete(Some(0), false, None, None);
    let rendered = plain(&component.render(60));
    assert!(!rendered.contains("Running..."), "{rendered}");
    assert!(!rendered.contains("(exit"), "{rendered}");
}

#[test]
fn bash_execution_reports_error_exit_and_cancellation() {
    theme();
    let mut component =
        BashExecutionComponent::new("false", std::rc::Rc::new(|| {}), false, BashExecutionOptions::default());
    component.set_complete(Some(3), false, None, None);
    assert!(plain(&component.render(60)).contains("(exit 3)"));

    let mut cancelled = BashExecutionComponent::new(
        "sleep 10",
        std::rc::Rc::new(|| {}),
        false,
        BashExecutionOptions::default(),
    );
    cancelled.set_complete(None, true, None, None);
    assert!(plain(&cancelled.render(60)).contains("(cancelled)"));
}

#[test]
fn bash_execution_preview_hides_extra_lines_and_warns_on_truncation() {
    theme();
    let mut component = BashExecutionComponent::new(
        "seq 1 30",
        std::rc::Rc::new(|| {}),
        false,
        BashExecutionOptions::default(),
    );
    let output: String = (1..=30).map(|value| format!("line {value}\n")).collect();
    component.append_output(&output);
    component.set_complete(Some(0), false, None, None);
    let rendered = plain(&component.render(80));
    assert!(rendered.contains("... 11 more lines ("), "{rendered}");
    assert!(!rendered.contains("line 10\n"), "{rendered}");
    assert!(rendered.contains("line 30"), "{rendered}");

    component.set_expanded(true);
    let rendered = plain(&component.render(80));
    assert!(rendered.contains("to collapse"), "{rendered}");
    assert!(rendered.contains("line 1"), "{rendered}");
}

#[test]
fn bash_execution_strips_ansi_and_tracks_elapsed_timer() {
    use std::cell::Cell;

    theme();
    let renders = std::rc::Rc::new(Cell::new(0));
    let counter = renders.clone();
    let mut component = BashExecutionComponent::new(
        "echo",
        std::rc::Rc::new(move || counter.set(counter.get() + 1)),
        true,
        BashExecutionOptions {
            show_elapsed: Some(true),
        },
    );
    component.append_output("\u{1b}[31mred\u{1b}[0m");
    assert_eq!(component.get_output(), "red");
    let deadline = component.next_timer_deadline().expect("timer");
    let renders_before = renders.get();
    assert!(component.poll_timer(deadline));
    assert!(renders.get() > renders_before);
    component.dispose();
    assert!(component.next_timer_deadline().is_none());
    assert!(!component.poll_timer(deadline + std::time::Duration::from_secs(5)));
}

#[test]
fn armin_animates_and_renders_message() {
    theme();
    let mut component = ArminComponent::new(std::rc::Rc::new(|| {}));
    assert!(["typewriter", "scanline", "rain", "fade", "crt", "glitch", "dissolve"].contains(&component.effect()));
    let mut now = std::time::Instant::now();
    for _ in 0..2000 {
        now += std::time::Duration::from_millis(50);
        component.poll(now);
        if !component.is_running() {
            break;
        }
    }
    // Source discrepancy: `rain` never settles the two all-background columns,
    // so that effect animates forever. Every other effect reaches the final grid.
    if component.effect() == "rain" {
        assert!(component.is_running());
    } else {
        assert!(!component.is_running(), "effect {} did not finish", component.effect());
    }
    let rendered = plain(&component.render(40));
    assert!(rendered.contains("ARMIN SAYS HI"), "{rendered}");
    assert_eq!(component.current_grid().len(), 18);
    component.dispose();
}

#[test]
fn daxnuts_reveals_image_and_text_over_time() {
    theme();
    let mut component = DaxnutsComponent::new(std::rc::Rc::new(|| {}));
    let initial = plain(&component.render(40));
    assert!(!initial.contains("Free Kimi K2.5"), "{initial}");

    let mut now = std::time::Instant::now();
    for _ in 0..40 {
        now += std::time::Duration::from_millis(80);
        component.poll(now);
    }
    assert!(!component.is_running());
    assert_eq!(component.tick(), 25);
    let rendered = plain(&component.render(40));
    assert!(rendered.contains("Free Kimi K2.5 via OpenCode Zen"), "{rendered}");
    assert!(rendered.contains("Powered by daxnuts"), "{rendered}");
    assert!(rendered.contains("Try OpenCode"), "{rendered}");
    component.dispose();
}

fn user_entry(id: &str, parent_id: Option<&str>, content: &str) -> SessionEntry {
    SessionEntry::Known(KnownSessionEntry::Message(SessionMessageEntry {
        id: id.to_owned(),
        parent_id: parent_id.map(str::to_owned),
        timestamp: "2026-01-01T00:00:00.000Z".to_owned(),
        message: Box::new(AgentMessage::User(UserMessage {
            role: UserRole::Tag,
            content: UserContent::Text(content.to_owned()),
            input_source: None,
            provider_native: None,
            timestamp: 0,
        })),
    }))
}

fn assistant_entry(id: &str, parent_id: Option<&str>, text: &str) -> SessionEntry {
    let mut message = AssistantMessage::empty("anthropic-messages", "anthropic", "claude-sonnet-4", 0);
    message.content = vec![pi_ai::types::AssistantContent::Text(pi_ai::types::TextContent::new(
        text,
    ))];
    message.stop_reason = StopReason::Stop;
    SessionEntry::Known(KnownSessionEntry::Message(SessionMessageEntry {
        id: id.to_owned(),
        parent_id: parent_id.map(str::to_owned),
        timestamp: "2026-01-01T00:00:00.000Z".to_owned(),
        message: Box::new(AgentMessage::Assistant(Box::new(message))),
    }))
}

fn tool_only_assistant_entry(id: &str, parent_id: Option<&str>) -> SessionEntry {
    let mut message = AssistantMessage::empty("anthropic-messages", "anthropic", "claude-sonnet-4", 0);
    message.content = vec![pi_ai::types::AssistantContent::ToolCall(pi_ai::types::ToolCall::new(
        format!("tc-{id}"),
        "read",
        serde_json::json!({"path": "test.ts"})
            .as_object()
            .cloned()
            .expect("object"),
    ))];
    message.stop_reason = StopReason::ToolUse;
    SessionEntry::Known(KnownSessionEntry::Message(SessionMessageEntry {
        id: id.to_owned(),
        parent_id: parent_id.map(str::to_owned),
        timestamp: "2026-01-01T00:00:00.000Z".to_owned(),
        message: Box::new(AgentMessage::Assistant(Box::new(message))),
    }))
}

fn tool_result_entry(id: &str, parent_id: Option<&str>, tool_call_id: &str, text: &str) -> SessionEntry {
    SessionEntry::Known(KnownSessionEntry::Message(SessionMessageEntry {
        id: id.to_owned(),
        parent_id: parent_id.map(str::to_owned),
        timestamp: "2026-01-01T00:00:00.000Z".to_owned(),
        message: Box::new(AgentMessage::ToolResult(pi_ai::types::ToolResultMessage {
            role: pi_ai::types::ToolResultRole::Tag,
            tool_call_id: tool_call_id.to_owned(),
            tool_name: "read".to_owned(),
            content: vec![pi_ai::types::TextOrImageContent::Text(pi_ai::types::TextContent::new(
                text,
            ))],
            details: None,
            usage: None,
            added_tool_names: None,
            is_error: false,
            timestamp: 0,
        })),
    }))
}

fn model_change_entry(id: &str, parent_id: Option<&str>) -> SessionEntry {
    SessionEntry::Known(KnownSessionEntry::ModelChange(ModelChangeEntry {
        id: id.to_owned(),
        parent_id: parent_id.map(str::to_owned),
        timestamp: "2026-01-01T00:00:00.000Z".to_owned(),
        provider: "anthropic".to_owned(),
        model_id: "claude-sonnet-4".to_owned(),
    }))
}

fn thinking_change_entry(id: &str, parent_id: Option<&str>) -> SessionEntry {
    SessionEntry::Known(KnownSessionEntry::ThinkingLevelChange(ThinkingLevelChangeEntry {
        id: id.to_owned(),
        parent_id: parent_id.map(str::to_owned),
        timestamp: "2026-01-01T00:00:00.000Z".to_owned(),
        thinking_level: "high".to_owned(),
    }))
}

fn compaction_entry(id: &str, parent_id: &str, first_kept_entry_id: &str, summary: &str) -> SessionEntry {
    SessionEntry::Known(KnownSessionEntry::Compaction(CompactionEntry {
        id: id.to_owned(),
        parent_id: Some(parent_id.to_owned()),
        timestamp: "2026-01-01T00:00:00.000Z".to_owned(),
        summary: summary.to_owned(),
        first_kept_entry_id: first_kept_entry_id.to_owned(),
        tokens_before: 12_345.0,
        usage: None,
        duration_ms: None,
        details: None,
        provider_native: None,
        from_hook: None,
        first_kept_entry_index: None,
    }))
}

fn build_tree(entries: Vec<SessionEntry>) -> Vec<SessionTreeNode> {
    let mut nodes: Vec<SessionTreeNode> = entries
        .into_iter()
        .map(|entry| SessionTreeNode {
            entry,
            children: Vec::new(),
            label: None,
            label_timestamp: None,
        })
        .collect();
    let ids: Vec<(String, usize)> = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.entry.id().unwrap_or_default().to_owned(), index))
        .collect();
    let mut roots = Vec::new();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
    for (index, node) in nodes.iter().enumerate() {
        match node.entry.parent_id() {
            None => roots.push(index),
            Some(parent_id) => {
                if let Some((_, parent_index)) = ids.iter().find(|(id, _)| id == parent_id) {
                    children[*parent_index].push(index);
                } else {
                    roots.push(index);
                }
            }
        }
    }
    fn build(index: usize, nodes: &mut Vec<SessionTreeNode>, children: &[Vec<usize>]) -> SessionTreeNode {
        let mut node = nodes[index].clone();
        node.children = children[index]
            .iter()
            .map(|child| build(*child, nodes, children))
            .collect();
        node
    }
    let result: Vec<SessionTreeNode> = roots.iter().map(|index| build(*index, &mut nodes, &children)).collect();
    result
}

fn tree_selector(entries: Vec<SessionEntry>, leaf: &str, filter: Option<FilterMode>) -> TreeSelectorComponent {
    TreeSelectorComponent::new(
        build_tree(entries),
        Some(leaf.to_owned()),
        24,
        std::rc::Rc::new(|_| {}),
        std::rc::Rc::new(|| {}),
        None,
        None,
        filter,
    )
}

fn selected_id(selector: &TreeSelectorComponent) -> Option<String> {
    selector.tree_list().borrow().selected_entry_id().map(str::to_owned)
}

#[test]
fn tree_selector_focuses_nearest_visible_ancestor_for_metadata_leaf() {
    theme();
    let entries = vec![
        user_entry("user-1", None, "hello"),
        assistant_entry("asst-1", Some("user-1"), "hi"),
        user_entry("user-2", Some("asst-1"), "active branch"),
        model_change_entry("model-1", Some("user-2")),
        user_entry("user-3", Some("asst-1"), "sibling branch"),
    ];
    let selector = tree_selector(entries, "model-1", None);
    assert_eq!(selected_id(&selector).as_deref(), Some("user-2"));

    let entries = vec![
        user_entry("user-1", None, "hello"),
        assistant_entry("asst-1", Some("user-1"), "hi"),
        user_entry("user-2", Some("asst-1"), "active branch"),
        thinking_change_entry("thinking-1", Some("user-2")),
        user_entry("user-3", Some("asst-1"), "sibling branch"),
    ];
    let selector = tree_selector(entries, "thinking-1", None);
    assert_eq!(selected_id(&selector).as_deref(), Some("user-2"));
}

#[test]
fn tree_selector_user_only_filter_starts_on_nearest_user() {
    theme();
    let entries = vec![
        user_entry("user-1", None, "hello"),
        assistant_entry("asst-1", Some("user-1"), "hi"),
        user_entry("user-2", Some("asst-1"), "active branch"),
        assistant_entry("asst-2", Some("user-2"), "response"),
    ];
    let selector = tree_selector(entries, "asst-2", Some(FilterMode::UserOnly));
    assert_eq!(selected_id(&selector).as_deref(), Some("user-2"));

    let rendered = plain(&selector.tree_list().borrow_mut().render(200));
    assert!(rendered.contains("user: hello"), "{rendered}");
    assert!(rendered.contains("user: active branch"), "{rendered}");
    assert!(!rendered.contains("assistant:"), "{rendered}");
    assert!(rendered.contains("[user]"), "{rendered}");
}

#[test]
fn tree_selector_switches_filters_and_preserves_ancestor() {
    theme();
    let entries = vec![
        user_entry("user-1", None, "hello"),
        assistant_entry("asst-1", Some("user-1"), "hi"),
        user_entry("user-2", Some("asst-1"), "active branch"),
        assistant_entry("asst-2", Some("user-2"), "response"),
        user_entry("user-3", Some("asst-1"), "sibling branch"),
    ];
    let mut selector = tree_selector(entries, "asst-2", None);
    assert_eq!(selected_id(&selector).as_deref(), Some("asst-2"));

    selector.handle_input("\u{15}");
    assert_eq!(selected_id(&selector).as_deref(), Some("user-2"));

    selector.handle_input("\u{4}");
    assert_eq!(selected_id(&selector).as_deref(), Some("user-2"));
}

#[test]
fn tree_selector_help_fits_narrow_width_without_ellipsis() {
    theme();
    let entries = vec![
        user_entry("user-1", None, "hello"),
        assistant_entry("asst-1", Some("user-1"), "hi"),
    ];
    let mut selector = tree_selector(entries, "asst-1", None);
    let lines = selector.render(30);
    let rendered = plain(&lines);
    for expected in ["branch", "copy", "filters", "cycle", "label time"] {
        assert!(rendered.contains(expected), "missing {expected}: {rendered}");
    }
    assert!(!rendered.contains("..."), "{rendered}");
    for line in &lines {
        assert!(pi_tui::visible_width(line) <= 30, "{line:?}");
    }
}

#[test]
fn tree_selector_copies_full_selected_message() {
    use std::cell::RefCell;

    theme();
    let message = format!("{}\nsecond line", "long message ".repeat(30));
    let entries = vec![
        user_entry("user-1", None, "hello"),
        assistant_entry("asst-1", Some("user-1"), &message),
    ];
    let mut selector = tree_selector(entries, "asst-1", None);
    let copied: std::rc::Rc<RefCell<Option<String>>> = std::rc::Rc::new(RefCell::new(None));
    let sink = copied.clone();
    selector.set_on_copy(Some(std::rc::Rc::new(move |text| {
        *sink.borrow_mut() = text;
    })));
    selector.handle_input("\u{18}");
    assert_eq!(copied.borrow().as_deref(), Some(message.as_str()));
}

fn branching_tree() -> Vec<SessionEntry> {
    vec![
        user_entry("user-1", None, "first message"),
        assistant_entry("asst-1", Some("user-1"), "response 1"),
        user_entry("user-2", Some("asst-1"), "second message"),
        assistant_entry("asst-2", Some("user-2"), "response 2"),
        user_entry("user-3a", Some("asst-2"), "branch A start"),
        assistant_entry("asst-3a", Some("user-3a"), "branch A response"),
        user_entry("user-4a", Some("asst-3a"), "branch A deep"),
        assistant_entry("asst-4a", Some("user-4a"), "branch A leaf"),
        user_entry("user-3b", Some("asst-2"), "branch B start"),
        assistant_entry("asst-3b", Some("user-3b"), "branch B response"),
        user_entry("user-4b", Some("asst-3b"), "branch B deep"),
    ]
}

const CTRL_LEFT: &str = "\u{1b}[1;5D";
const CTRL_RIGHT: &str = "\u{1b}[1;5C";
const ALT_LEFT: &str = "\u{1b}[1;3D";
const ALT_RIGHT: &str = "\u{1b}[1;3C";
const UP: &str = "\u{1b}[A";
const DOWN: &str = "\u{1b}[B";

#[test]
fn tree_selector_ctrl_arrows_fold_and_jump_between_segments() {
    theme();
    let mut selector = tree_selector(branching_tree(), "asst-4a", None);

    selector.handle_input(CTRL_LEFT);
    assert_eq!(selected_id(&selector).as_deref(), Some("user-3a"));

    selector.handle_input(CTRL_LEFT);
    assert_eq!(selected_id(&selector).as_deref(), Some("user-3a"));

    selector.handle_input(DOWN);
    assert_eq!(selected_id(&selector).as_deref(), Some("user-3b"));

    selector.handle_input(UP);
    assert_eq!(selected_id(&selector).as_deref(), Some("user-3a"));

    selector.handle_input(CTRL_RIGHT);
    assert_eq!(selected_id(&selector).as_deref(), Some("user-3a"));

    selector.handle_input(DOWN);
    assert_eq!(selected_id(&selector).as_deref(), Some("asst-3a"));

    selector.handle_input(CTRL_LEFT);
    assert_eq!(selected_id(&selector).as_deref(), Some("user-3a"));

    selector.handle_input(CTRL_RIGHT);
    assert_eq!(selected_id(&selector).as_deref(), Some("asst-4a"));
}

#[test]
fn tree_selector_alt_arrows_alias_fold_and_unfold() {
    theme();
    let mut selector = tree_selector(branching_tree(), "asst-4a", None);

    selector.handle_input(ALT_LEFT);
    assert_eq!(selected_id(&selector).as_deref(), Some("user-3a"));
    selector.handle_input(ALT_LEFT);
    assert_eq!(selected_id(&selector).as_deref(), Some("user-3a"));
    selector.handle_input(ALT_RIGHT);
    assert_eq!(selected_id(&selector).as_deref(), Some("user-3a"));
    selector.handle_input(ALT_RIGHT);
    assert_eq!(selected_id(&selector).as_deref(), Some("asst-4a"));
}

#[test]
fn tree_selector_folding_root_hides_subtree_and_preserves_nested_fold() {
    theme();
    let mut selector = tree_selector(branching_tree(), "asst-4a", None);

    selector.handle_input(CTRL_LEFT); // asst-4a -> user-3a
    selector.handle_input(CTRL_LEFT); // fold user-3a
    selector.handle_input(CTRL_LEFT); // user-3a -> user-2
    selector.handle_input(CTRL_LEFT); // user-2 -> user-1
    selector.handle_input(CTRL_LEFT); // fold user-1 (root)

    let rendered = plain(&selector.tree_list().borrow_mut().render(120));
    assert!(rendered.contains("first message"), "{rendered}");
    assert!(!rendered.contains("response 1"), "{rendered}");
    assert!(!rendered.contains("branch A"), "{rendered}");

    selector.handle_input(CTRL_RIGHT); // unfold root
    let rendered = plain(&selector.tree_list().borrow_mut().render(120));
    assert!(rendered.contains("response 1"), "{rendered}");
    // Nested fold on user-3a survived the root fold/unfold.
    assert!(!rendered.contains("branch A response"), "{rendered}");
}

#[test]
fn tree_selector_search_and_filter_changes_reset_folds() {
    theme();
    let mut selector = tree_selector(branching_tree(), "asst-4a", None);

    selector.handle_input(CTRL_LEFT);
    selector.handle_input(CTRL_LEFT); // fold user-3a
    assert!(!plain(&selector.tree_list().borrow_mut().render(160)).contains("branch A response"));

    selector.handle_input("b");
    let rendered = plain(&selector.tree_list().borrow_mut().render(160));
    assert!(rendered.contains("branch A response"), "{rendered}");

    selector.handle_input("\u{1b}");
    selector.handle_input(CTRL_LEFT);
    selector.handle_input(CTRL_LEFT);
    assert!(!plain(&selector.tree_list().borrow_mut().render(160)).contains("branch A response"));

    selector.handle_input("\u{14}"); // ctrl+t: no-tools filter
    let rendered = plain(&selector.tree_list().borrow_mut().render(160));
    assert!(rendered.contains("branch A response"), "{rendered}");
}

#[test]
fn tree_selector_user_only_compaction_view_orders_summary_before_retained_prompts() {
    theme();
    let entries = vec![
        user_entry("user-old", None, "compacted-away prompt"),
        assistant_entry("assistant-old", Some("user-old"), "compacted-away response"),
        user_entry("user-kept", Some("assistant-old"), "retained prompt"),
        assistant_entry("assistant-kept", Some("user-kept"), "retained response"),
        compaction_entry(
            "compaction-1",
            "assistant-kept",
            "user-kept",
            "generated compacted summary",
        ),
        user_entry("user-later", Some("compaction-1"), "later prompt"),
        assistant_entry("assistant-later", Some("user-later"), "later response"),
    ];
    let mut selector = tree_selector(entries, "assistant-later", Some(FilterMode::UserOnly));
    let rendered = plain(&selector.tree_list().borrow_mut().render(200));
    assert!(!rendered.contains("compacted-away prompt"), "{rendered}");
    let summary_index = rendered.find("generated compacted summary").expect("summary");
    let retained_index = rendered.find("retained prompt").expect("retained");
    let later_index = rendered.find("later prompt").expect("later");
    assert!(
        summary_index < retained_index && retained_index < later_index,
        "{rendered}"
    );
    assert!(rendered.contains("(3/3) [user]"), "{rendered}");

    selector.handle_input("\u{4}");
    let rendered = plain(&selector.tree_list().borrow_mut().render(200));
    assert!(rendered.contains("compacted-away prompt"), "{rendered}");
}

#[test]
fn tree_selector_hides_tool_only_assistants_and_maps_tool_results() {
    theme();
    let entries = vec![
        user_entry("user-1", None, "hello"),
        tool_only_assistant_entry("asst-tool", Some("user-1")),
        tool_result_entry("result-1", Some("asst-tool"), "tc-asst-tool", "file contents"),
        assistant_entry("asst-1", Some("result-1"), "visible"),
    ];
    let selector = tree_selector(entries, "asst-1", None);
    let rendered = plain(&selector.tree_list().borrow_mut().render(160));
    assert!(!rendered.contains("assistant: (no content)"), "{rendered}");
    assert!(rendered.contains("[read: test.ts]"), "{rendered}");
    assert!(rendered.contains("assistant: visible"), "{rendered}");

    // A tool-only assistant is visible when it is the current leaf.
    let entries = vec![
        user_entry("user-1", None, "hello"),
        tool_only_assistant_entry("asst-tool", Some("user-1")),
    ];
    let leaf_selector = tree_selector(entries, "asst-tool", None);
    let rendered = plain(&leaf_selector.tree_list().borrow_mut().render(160));
    assert!(rendered.contains("assistant: (no content)"), "{rendered}");
}

fn session_info(
    path: &str,
    name: Option<&str>,
    first_message: &str,
    modified_offset_ms: i64,
    archived: bool,
) -> pi_coding_agent::core::session_manager::SessionInfo {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or_default())
        .unwrap_or_default();
    pi_coding_agent::core::session_manager::SessionInfo {
        path: path.to_owned(),
        id: format!("id-{path}"),
        cwd: "/repo".to_owned(),
        name: name.map(str::to_owned),
        is_archived: archived.then_some(true),
        parent_session_path: None,
        created: now - modified_offset_ms,
        modified: now - modified_offset_ms,
        message_count: 3,
        first_message: first_message.to_owned(),
        all_messages_text: first_message.to_owned(),
    }
}

fn session_loader(sessions: Vec<pi_coding_agent::core::session_manager::SessionInfo>) -> SessionListLoader {
    std::rc::Rc::new(move |_progress| {
        let sessions = sessions.clone();
        Box::pin(async move { Ok(sessions) })
    })
}

fn build_session_selector(
    current: Vec<pi_coding_agent::core::session_manager::SessionInfo>,
    all: Vec<pi_coding_agent::core::session_manager::SessionInfo>,
    current_file: Option<&str>,
) -> SessionSelectorComponent {
    SessionSelectorComponent::new(
        session_loader(current),
        session_loader(all),
        std::rc::Rc::new(|_| {}),
        std::rc::Rc::new(|| {}),
        std::rc::Rc::new(|| {}),
        std::rc::Rc::new(|| {}),
        None,
        None,
        None,
        None,
        None,
        current_file,
    )
}

#[tokio::test]
async fn session_selector_loads_current_scope_and_renders_sessions() {
    theme();
    let current = vec![
        session_info(
            "/sessions/a.jsonl",
            Some("alpha"),
            "first alpha message",
            7_200_000,
            false,
        ),
        session_info("/sessions/b.jsonl", None, "beta first message", 3 * 86_400_000, false),
    ];
    let mut selector = build_session_selector(current, Vec::new(), Some("/sessions/b.jsonl"));
    selector.load_current_sessions().await;
    let rendered = plain(&selector.render(120));
    assert!(rendered.contains("Resume Session (Current Folder)"), "{rendered}");
    assert!(rendered.contains("alpha"), "{rendered}");
    assert!(rendered.contains("beta first message"), "{rendered}");
    assert!(rendered.contains("2h"), "{rendered}");
    assert!(rendered.contains("3d"), "{rendered}");
    assert!(rendered.contains("3 2h"), "{rendered}");
}

#[tokio::test]
async fn session_selector_scope_toggle_loads_all_sessions() {
    theme();
    let current = vec![session_info("/sessions/a.jsonl", None, "current only", 60_000, false)];
    let all = vec![
        session_info("/sessions/a.jsonl", None, "current only", 60_000, false),
        session_info("/other/c.jsonl", Some("other"), "other folder", 120_000, false),
    ];
    let mut selector = build_session_selector(current, all, None);
    selector.load_current_sessions().await;

    selector.handle_input("\t");
    assert!(selector.has_pending_action());
    selector.run_pending_action().await;
    let rendered = plain(&selector.render(120));
    assert!(rendered.contains("Resume Session (All)"), "{rendered}");
    assert!(rendered.contains("› current only"), "{rendered}");
    assert!(rendered.contains("other"), "{rendered}");
    assert!(rendered.contains("/repo 3 2m"), "{rendered}");
}

#[tokio::test]
async fn session_selector_sort_name_and_path_toggles() {
    theme();
    let current = vec![
        session_info("/sessions/a.jsonl", None, "un-named", 60_000, false),
        session_info("/sessions/b.jsonl", Some("named"), "named session", 120_000, false),
    ];
    let mut selector = build_session_selector(current, Vec::new(), None);
    selector.load_current_sessions().await;

    selector.handle_input("\u{13}");
    assert_eq!(
        selector.sort_mode(),
        pi_coding_agent::modes::interactive::components::session_selector_search::SortMode::Relevance
    );
    assert!(plain(&selector.render(120)).contains("Sort: Fuzzy"));

    selector.handle_input("\u{e}");
    assert_eq!(
        selector.name_filter(),
        pi_coding_agent::modes::interactive::components::session_selector_search::NameFilter::Named
    );
    assert!(plain(&selector.render(120)).contains("Name: Named"));

    selector.handle_input("\u{10}");
    let rendered = plain(&selector.render(120));
    assert!(rendered.contains("path (on)"), "{rendered}");
    assert!(rendered.contains("/sessions/b.jsonl"), "{rendered}");
}

#[tokio::test]
async fn session_selector_search_filters_and_current_session_cannot_be_deleted() {
    theme();
    let current = vec![
        session_info("/sessions/a.jsonl", None, "alpha prompt", 60_000, false),
        session_info("/sessions/b.jsonl", None, "beta prompt", 120_000, false),
    ];
    let mut selector = build_session_selector(current, Vec::new(), Some("/sessions/b.jsonl"));
    selector.load_current_sessions().await;

    for character in ["a", "l", "p"] {
        selector.handle_input(character);
    }
    let rendered = plain(&selector.render(120));
    assert!(rendered.contains("alpha prompt"), "{rendered}");
    assert!(!rendered.contains("beta prompt"), "{rendered}");

    // Clear the search with backspaces.
    for _ in 0..3 {
        selector.handle_input("\u{7f}");
    }
    // Select the current session (beta is second).
    selector.handle_input("\u{1b}[B");
    selector.handle_input("\u{4}");
    let rendered = plain(&selector.render(120));
    assert!(
        rendered.contains("Cannot delete the currently active session"),
        "{rendered}"
    );
}

#[tokio::test]
async fn session_selector_deletes_session_after_confirmation() {
    theme();
    let directory = tempfile::tempdir().expect("temp dir");
    let target = directory.path().join("delete-me.jsonl");
    std::fs::write(&target, "session").expect("session file");
    let target_path = target.to_string_lossy().into_owned();
    let current = vec![
        session_info("/sessions/a.jsonl", None, "keep", 60_000, false),
        session_info(&target_path, None, "delete me", 120_000, false),
    ];
    let mut selector = build_session_selector(current, Vec::new(), Some("/sessions/a.jsonl"));
    selector.load_current_sessions().await;

    selector.handle_input("\u{1b}[B");
    selector.handle_input("\u{4}");
    assert!(
        plain(&selector.render(120)).contains("Delete session?"),
        "confirmation hint"
    );
    // Other keys are ignored while confirming.
    selector.handle_input("x");
    assert!(plain(&selector.render(120)).contains("Delete session?"));

    selector.handle_input("\r");
    assert!(selector.has_pending_action());
    selector.run_pending_action().await;
    assert!(!target.exists());
    let rendered = plain(&selector.render(120));
    assert!(
        rendered.contains("Session deleted") || rendered.contains("Session moved to trash"),
        "{rendered}"
    );
}

#[tokio::test]
async fn session_selector_archive_requires_control_db_and_archives_with_one() {
    theme();
    let directory = tempfile::tempdir().expect("temp dir");
    let control_db = directory.path().join("control.db");
    let control_db_path = control_db.to_string_lossy().into_owned();
    let session_path = directory.path().join("session.jsonl");
    std::fs::write(&session_path, "session").expect("session file");
    let session_path = session_path.to_string_lossy().into_owned();
    let current = vec![session_info(&session_path, None, "archive me", 60_000, false)];

    let mut no_db = SessionSelectorComponent::new(
        session_loader(current.clone()),
        session_loader(Vec::new()),
        std::rc::Rc::new(|_| {}),
        std::rc::Rc::new(|| {}),
        std::rc::Rc::new(|| {}),
        std::rc::Rc::new(|| {}),
        None,
        None,
        None,
        None,
        None,
        None,
    );
    no_db.load_current_sessions().await;
    no_db.handle_input("\u{1}");
    no_db.run_pending_action().await;
    assert!(plain(&no_db.render(120)).contains("Session archive requires a control database."));

    let mut with_db = SessionSelectorComponent::new(
        session_loader(current),
        session_loader(Vec::new()),
        std::rc::Rc::new(|_| {}),
        std::rc::Rc::new(|| {}),
        std::rc::Rc::new(|| {}),
        std::rc::Rc::new(|| {}),
        None,
        None,
        None,
        Some(control_db_path),
        None,
        None,
    );
    with_db.load_current_sessions().await;
    with_db.handle_input("\u{1}");
    with_db.run_pending_action().await;
    assert!(plain(&with_db.render(120)).contains("Session archived"));
}

#[tokio::test]
async fn session_selector_rename_mode_trims_and_submits() {
    theme();
    let current = vec![session_info(
        "/sessions/a.jsonl",
        Some("old name"),
        "alpha",
        60_000,
        false,
    )];
    type RenameCalls = Arc<std::sync::Mutex<Vec<(String, Option<String>)>>>;
    let renamed: RenameCalls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = renamed.clone();
    let rename: pi_coding_agent::modes::interactive::components::session_selector::RenameSessionFn =
        std::rc::Rc::new(move |path, name| {
            let sink = sink.clone();
            Box::pin(async move {
                sink.lock().unwrap().push((path, name));
                Ok(())
            })
        });
    let mut selector = SessionSelectorComponent::new(
        session_loader(current.clone()),
        session_loader(Vec::new()),
        std::rc::Rc::new(|_| {}),
        std::rc::Rc::new(|| {}),
        std::rc::Rc::new(|| {}),
        std::rc::Rc::new(|| {}),
        Some(rename),
        None,
        None,
        None,
        None,
        None,
    );
    selector.load_current_sessions().await;
    selector.handle_input("\u{12}");
    let rendered = plain(&selector.render(120));
    assert!(rendered.contains("Rename Session"), "{rendered}");
    assert!(rendered.contains("old name"), "{rendered}");

    // Typing inserts at the initial cursor (start), matching Input.setValue.
    for character in ["n", "e", "w"] {
        selector.handle_input(character);
    }
    selector.handle_input("\r");
    assert!(selector.has_pending_action());
    selector.run_pending_action().await;
    assert_eq!(
        *renamed.lock().unwrap(),
        vec![("/sessions/a.jsonl".to_owned(), Some("newold name".to_owned()))]
    );
    assert!(plain(&selector.render(120)).contains("Resume Session"));

    // Clear with ctrl+k and submit an empty name (None).
    selector.handle_input("\u{12}");
    selector.handle_input("\u{1}");
    selector.handle_input("\u{b}");
    selector.handle_input("\r");
    selector.run_pending_action().await;
    assert_eq!(renamed.lock().unwrap().last().unwrap().1, None);
}

#[test]
fn footer_formats_tokens_like_the_source() {
    let cases = [
        (0.0, "0"),
        (999.0, "999"),
        (1000.0, "1.0k"),
        (1500.0, "1.5k"),
        (9999.0, "10.0k"),
        (10_000.0, "10k"),
        (12_345.0, "12k"),
        (999_999.0, "1000k"),
        (1_000_000.0, "1.0M"),
        (1_500_000.0, "1.5M"),
        (9_999_999.0, "10.0M"),
        (10_000_000.0, "10M"),
        (12_345_678.0, "12M"),
    ];
    for (count, expected) in cases {
        assert_eq!(format_tokens(count), expected, "count {count}");
    }
}

#[test]
fn footer_formats_cwd_relative_to_home() {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/tester".to_owned());
    assert_eq!(format_cwd_for_footer(&home, Some(&home)), "~");
    assert_eq!(
        format_cwd_for_footer(&format!("{home}/projects/pi"), Some(&home)),
        format!("~{}projects{}pi", std::path::MAIN_SEPARATOR, std::path::MAIN_SEPARATOR)
    );
    assert_eq!(format_cwd_for_footer("/opt/other", Some(&home)), "/opt/other");
    assert_eq!(format_cwd_for_footer("/opt/other", None), "/opt/other");
}

fn assistant_with_usage(
    input: f64,
    output: f64,
    cache_read: f64,
    cache_write: f64,
    cost: f64,
) -> pi_ai::types::AssistantMessage {
    let mut message = pi_ai::types::AssistantMessage::empty("faux", "faux", "faux-1", 0);
    message.usage.input = input;
    message.usage.output = output;
    message.usage.cache_read = cache_read;
    message.usage.cache_write = cache_write;
    message.usage.cost.total = cost;
    message.stop_reason = StopReason::Stop;
    message
}

#[tokio::test]
async fn footer_renders_override_stats_context_and_model() {
    theme();
    let directory = tempfile::tempdir().expect("temp dir");
    let fixture =
        agent_session_fixture::fixture(directory.path(), pi_agent_core::harness::types::ThinkingLevel::High).await;
    let session = std::sync::Arc::new(fixture.session);
    {
        let mut manager = session.session_manager();
        manager.append_session_info("Test Session").expect("session name");
        manager
            .append_message(AgentMessage::Assistant(Box::new(assistant_with_usage(
                10_000.0, 2000.0, 3000.0, 4000.0, 0.1234,
            ))))
            .expect("append assistant");
        manager
            .append_message(AgentMessage::ToolResult(pi_ai::types::ToolResultMessage {
                role: pi_ai::types::ToolResultRole::Tag,
                tool_call_id: "tool".to_owned(),
                tool_name: "read".to_owned(),
                content: vec![],
                details: None,
                usage: Some(pi_ai::types::Usage {
                    input: 500.0,
                    output: 100.0,
                    ..Default::default()
                }),
                added_tool_names: None,
                is_error: false,
                timestamp: 0,
            }))
            .expect("append tool result");
    }

    let footer_data = std::sync::Arc::new(FooterDataProvider::new(directory.path()));
    let mut footer = FooterComponent::new(std::sync::Arc::clone(&session), footer_data);
    let model = pi_ai::types::Model {
        id: "gpt-x".to_owned(),
        provider: "openai".to_owned(),
        reasoning: true,
        context_window: 200_000.0,
        ..session.model()
    };
    footer.set_session_override(FooterSessionOverride {
        cwd: "/repo".to_owned(),
        session_manager: None,
        model: Some(model),
        thinking_level: "high".to_owned(),
        context_usage: Some(pi_coding_agent::core::footer_data_provider::ContextUsage {
            tokens: Some(50_000.0),
            context_window: 200_000.0,
            percent: Some(25.0),
        }),
    });
    let rendered = plain(&footer.render(120));
    assert!(rendered.contains("/repo • Test Session"), "{rendered}");
    assert!(rendered.contains("↑11k"), "{rendered}");
    assert!(rendered.contains("↓2.1k"), "{rendered}");
    assert!(rendered.contains("R3.0k"), "{rendered}");
    assert!(rendered.contains("W4.0k"), "{rendered}");
    assert!(rendered.contains("CH17.6%"), "{rendered}");
    assert!(rendered.contains("$0.123"), "{rendered}");
    assert!(rendered.contains("25.0%/200k (auto)"), "{rendered}");
    assert!(rendered.contains("gpt-x • effort high"), "{rendered}");

    footer.set_auto_compact_enabled(false);
    let rendered = plain(&footer.render(120));
    assert!(!rendered.contains("(auto)"), "{rendered}");
}

#[tokio::test]
async fn footer_truncates_to_width_and_appends_extension_status() {
    theme();
    let directory = tempfile::tempdir().expect("temp dir");
    let fixture =
        agent_session_fixture::fixture(directory.path(), pi_agent_core::harness::types::ThinkingLevel::Off).await;
    let session = std::sync::Arc::new(fixture.session);
    let footer_data = std::sync::Arc::new(FooterDataProvider::new(directory.path()));
    footer_data.set_extension_status("goal", Some("working on the port"));
    let mut footer = FooterComponent::new(session, footer_data);
    let lines = footer.render(16);
    assert!(lines.iter().all(|line| pi_tui::visible_width(line) <= 16), "{lines:?}");
    assert!(lines.len() >= 3, "{lines:?}");
    let rendered = plain(&lines);
    assert!(rendered.contains("working on th"), "{rendered}");
}
