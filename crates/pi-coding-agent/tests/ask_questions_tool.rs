//! Port of `packages/coding-agent/test/ask-questions-tool.test.ts`.

#[path = "support/tool_context.rs"]
mod tool_context;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use pi_agent_core::AgentHarnessTool;
use pi_ai::request::BoxFuture;
use pi_coding_agent::core::desktop_notification::{
    DesktopNotification, DesktopNotificationHandle, DesktopNotifier, PERSISTENT_DESKTOP_NOTIFICATION_EXPIRE_TIME_MS,
};
use pi_coding_agent::core::tools::ask_questions::{AskQuestionsTool, parameters};
use pi_coding_agent::core::tools::context::{ExtensionMode, ToolContext, ToolUi};
use serde_json::{Value, json};
use tool_context::StaticSession;

struct ScriptedUi {
    selects: Mutex<VecDeque<Option<String>>>,
    inputs: Mutex<VecDeque<Option<String>>>,
    seen_titles: Mutex<Vec<String>>,
    seen_options: Mutex<Vec<Vec<String>>>,
}

impl ScriptedUi {
    fn new(selects: Vec<Option<&str>>, inputs: Vec<Option<&str>>) -> Arc<Self> {
        Arc::new(Self {
            selects: Mutex::new(selects.into_iter().map(|value| value.map(str::to_owned)).collect()),
            inputs: Mutex::new(inputs.into_iter().map(|value| value.map(str::to_owned)).collect()),
            seen_titles: Mutex::new(Vec::new()),
            seen_options: Mutex::new(Vec::new()),
        })
    }
}

impl ToolUi for ScriptedUi {
    fn select<'a>(&'a self, title: &'a str, options: &'a [String]) -> BoxFuture<'a, Option<String>> {
        self.seen_titles.lock().expect("titles lock").push(title.to_owned());
        self.seen_options.lock().expect("options lock").push(options.to_vec());
        let choice = self.selects.lock().expect("selects lock").pop_front().flatten();
        Box::pin(async move { choice })
    }

    fn input<'a>(&'a self, _title: &'a str, _placeholder: Option<&'a str>) -> BoxFuture<'a, Option<String>> {
        let answer = self.inputs.lock().expect("inputs lock").pop_front().flatten();
        Box::pin(async move { answer })
    }
}

#[derive(Default)]
struct MockNotifier {
    notifications: Mutex<Vec<DesktopNotification>>,
    closes: Arc<AtomicUsize>,
}

impl DesktopNotifier for MockNotifier {
    fn notify(&self, notification: &DesktopNotification) -> Option<Box<dyn DesktopNotificationHandle>> {
        self.notifications
            .lock()
            .expect("notifications lock")
            .push(notification.clone());
        Some(Box::new(CloseCounter(Arc::clone(&self.closes))))
    }
}

struct CloseCounter(Arc<AtomicUsize>);

impl DesktopNotificationHandle for CloseCounter {
    fn close(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

fn context(ui: Arc<ScriptedUi>, notifier: Arc<MockNotifier>) -> ToolContext {
    ToolContext::new("/repo", StaticSession::new("/repo", "/repo/sessions", "session", None))
        .with_mode(ExtensionMode::Tui, true)
        .with_ui(ui)
        .with_desktop_notifier(notifier)
}

#[test]
fn exposes_min_max_schema_constraints() {
    let schema = parameters();
    assert_eq!(schema["properties"]["questions"]["minItems"], 1);
    assert_eq!(schema["properties"]["questions"]["maxItems"], 4);
    assert_eq!(
        schema["properties"]["questions"]["items"]["properties"]["options"]["minItems"],
        2
    );
    assert_eq!(
        schema["properties"]["questions"]["items"]["properties"]["options"]["maxItems"],
        4
    );
}

#[tokio::test]
async fn asks_a_single_choice_question_and_returns_the_selected_label() {
    let ui = ScriptedUi::new(vec![Some("1. Direct API — Keep code simple")], vec![]);
    let notifier = Arc::new(MockNotifier::default());
    let result = AskQuestionsTool::new()
        .execute(
            "call-1".to_owned(),
            json!({
                "questions": [{
                    "question": "Which approach should we use?",
                    "header": "Approach",
                    "options": [
                        { "label": "Direct API", "description": "Keep code simple" },
                        { "label": "Adapter", "description": "Add indirection" }
                    ]
                }]
            }),
            None,
            None,
            context(Arc::clone(&ui), notifier),
            None,
        )
        .await
        .expect("ask questions");

    assert_eq!(result.details.as_ref().expect("details")["cancelled"], false);
    assert_eq!(
        result.details.as_ref().expect("details")["answers"],
        json!({ "Which approach should we use?": "Direct API" })
    );
    assert!(ui.seen_options.lock().expect("options")[0].contains(&"1. Direct API — Keep code simple".to_owned()));
    assert!(ui.seen_options.lock().expect("options")[0].contains(&"Cancel".to_owned()));
}

#[tokio::test]
async fn adds_an_automatic_other_option_for_custom_answers() {
    let ui = ScriptedUi::new(vec![Some("Other")], vec![Some("Use a plugin")]);
    let notifier = Arc::new(MockNotifier::default());
    let result = AskQuestionsTool::new()
        .execute(
            "call-2".to_owned(),
            json!({
                "questions": [{
                    "question": "What should we build?",
                    "options": [{ "label": "Tool" }, { "label": "Extension" }]
                }]
            }),
            None,
            None,
            context(Arc::clone(&ui), notifier),
            None,
        )
        .await
        .expect("ask questions");

    assert_eq!(
        result.details.as_ref().expect("details")["answers"],
        json!({ "What should we build?": "Use a plugin" })
    );
}

#[tokio::test]
async fn supports_multi_select_questions() {
    let ui = ScriptedUi::new(vec![Some("[ ] 1. Tests"), Some("[ ] 2. Docs"), Some("Done")], vec![]);
    let notifier = Arc::new(MockNotifier::default());
    let result = AskQuestionsTool::new()
        .execute(
            "call-3".to_owned(),
            json!({
                "questions": [{
                    "question": "Which follow-ups should be included?",
                    "multiSelect": true,
                    "options": [{ "label": "Tests" }, { "label": "Docs" }]
                }]
            }),
            None,
            None,
            context(Arc::clone(&ui), notifier),
            None,
        )
        .await
        .expect("ask questions");

    assert_eq!(
        result.details.as_ref().expect("details")["answers"],
        json!({ "Which follow-ups should be included?": "Tests, Docs" })
    );
}

#[tokio::test]
async fn sends_a_persistent_desktop_notification_while_waiting() {
    let ui = ScriptedUi::new(vec![Some("1. Yes")], vec![]);
    let notifier = Arc::new(MockNotifier::default());
    let result = AskQuestionsTool::new()
        .execute(
            "call-notify".to_owned(),
            json!({ "questions": [{ "question": "Proceed?", "options": [{ "label": "Yes" }, { "label": "No" }] }] }),
            None,
            None,
            context(Arc::clone(&ui), Arc::clone(&notifier)),
            None,
        )
        .await
        .expect("ask questions");

    assert_eq!(
        result
            .details
            .as_ref()
            .and_then(|details| details.get("cancelled"))
            .and_then(serde_json::Value::as_bool),
        Some(false)
    );
    let notifications = notifier.notifications.lock().expect("notifications");
    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0].body, "Pi is waiting for your answer.");
    assert_eq!(
        notifications[0].expire_time_ms,
        Some(PERSISTENT_DESKTOP_NOTIFICATION_EXPIRE_TIME_MS)
    );
    assert_eq!(notifications[0].title, "Pi question needs input");
    assert_eq!(
        notifications[0].urgency,
        Some(pi_coding_agent::core::desktop_notification::DesktopNotificationUrgency::Normal)
    );
    assert_eq!(notifier.closes.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn closes_the_desktop_notification_when_questions_are_cancelled() {
    let ui = ScriptedUi::new(vec![Some("Cancel")], vec![]);
    let notifier = Arc::new(MockNotifier::default());
    let result = AskQuestionsTool::new()
        .execute(
            "call-notify-cancel".to_owned(),
            json!({ "questions": [{ "question": "Proceed?", "options": [{ "label": "Yes" }, { "label": "No" }] }] }),
            None,
            None,
            context(Arc::clone(&ui), Arc::clone(&notifier)),
            None,
        )
        .await
        .expect("ask questions");

    assert_eq!(
        result
            .details
            .as_ref()
            .and_then(|details| details.get("cancelled"))
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert_eq!(notifier.notifications.lock().expect("notifications").len(), 1);
    assert_eq!(notifier.closes.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn does_not_expose_question_or_option_text_in_the_desktop_notification() {
    let ui = ScriptedUi::new(vec![Some("1. Ship secret option")], vec![]);
    let notifier = Arc::new(MockNotifier::default());
    AskQuestionsTool::new()
        .execute(
            "call-notify-redacted".to_owned(),
            json!({
                "questions": [{
                    "question": "Should we use token secret-question-token?",
                    "options": [{ "label": "Ship secret option" }, { "label": "Hide secret option" }]
                }]
            }),
            None,
            None,
            context(Arc::clone(&ui), Arc::clone(&notifier)),
            None,
        )
        .await
        .expect("ask questions");

    let notifications = notifier.notifications.lock().expect("notifications");
    assert!(!notifications[0].body.contains("secret-question-token"));
    assert!(!notifications[0].body.contains("secret option"));
}

#[tokio::test]
async fn rejects_duplicate_option_labels() {
    let ui = ScriptedUi::new(vec![], vec![]);
    let error = AskQuestionsTool::new()
        .execute(
            "call-duplicate-options".to_owned(),
            json!({
                "questions": [{ "question": "Pick one?", "options": [{ "label": "A" }, { "label": "A" }] }]
            }),
            None,
            None,
            context(ui, Arc::new(MockNotifier::default())),
            None,
        )
        .await
        .expect_err("duplicate options must fail");

    assert_eq!(
        error.to_string(),
        "Duplicate option label \"A\" in question \"Pick one?\""
    );
}

#[tokio::test]
async fn returns_partial_answers_when_cancelled_after_earlier_questions() {
    let ui = ScriptedUi::new(vec![Some("1. First"), Some("Cancel")], vec![]);
    let result = AskQuestionsTool::new()
        .execute(
            "call-cancel-partial".to_owned(),
            json!({
                "questions": [
                    { "question": "First?", "options": [{ "label": "First" }, { "label": "Second" }] },
                    { "question": "Second?", "options": [{ "label": "Third" }, { "label": "Fourth" }] }
                ]
            }),
            None,
            None,
            context(ui, Arc::new(MockNotifier::default())),
            None,
        )
        .await
        .expect("ask questions");

    let details = result.details.as_ref().expect("details");
    assert_eq!(details["cancelled"], true);
    assert_eq!(details["answers"], json!({ "First?": "First" }));
}

#[tokio::test]
async fn rejects_duplicate_questions() {
    let ui = ScriptedUi::new(vec![], vec![]);
    let error = AskQuestionsTool::new()
        .execute(
            "call-4".to_owned(),
            json!({
                "questions": [
                    { "question": "Duplicate?", "options": [{ "label": "A" }, { "label": "B" }] },
                    { "question": "Duplicate?", "options": [{ "label": "C" }, { "label": "D" }] }
                ]
            }),
            None,
            None,
            context(ui, Arc::new(MockNotifier::default())),
            None,
        )
        .await
        .expect_err("duplicate questions must fail");

    assert_eq!(error.to_string(), "Duplicate question text: Duplicate?");
}

#[tokio::test]
async fn returns_an_error_outside_interactive_tui_mode() {
    let ui = ScriptedUi::new(vec![], vec![]);
    let result = AskQuestionsTool::new()
        .execute(
            "call-5".to_owned(),
            json!({ "questions": [{ "question": "Proceed?", "options": [{ "label": "Yes" }, { "label": "No" }] }] }),
            None,
            None,
            ToolContext::new("/repo", StaticSession::new("/repo", "/repo/sessions", "session", None))
                .with_mode(ExtensionMode::Print, false)
                .with_ui(ui),
            None,
        )
        .await
        .expect("unavailable result");

    assert_eq!(result.is_error, Some(true));
    assert_eq!(
        result
            .details
            .as_ref()
            .and_then(|details| details.get("cancelled"))
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert_eq!(
        serde_json::to_value(&result.content).expect("content"),
        json!([{ "type": "text", "text": "Error: ask_questions requires an interactive TUI session" }])
    );
}

#[tokio::test]
async fn result_text_formats_answers() {
    let ui = ScriptedUi::new(vec![Some("1. Yes")], vec![]);
    let result = AskQuestionsTool::new()
        .execute(
            "call-text".to_owned(),
            json!({ "questions": [{ "question": "Proceed?", "options": [{ "label": "Yes" }, { "label": "No" }] }] }),
            None,
            None,
            context(ui, Arc::new(MockNotifier::default())),
            None,
        )
        .await
        .expect("ask questions");

    let content: Value = serde_json::to_value(&result.content).expect("content");
    assert_eq!(
        content,
        json!([{ "type": "text", "text": "User answered questions:\n- Proceed? → Yes" }])
    );
}
