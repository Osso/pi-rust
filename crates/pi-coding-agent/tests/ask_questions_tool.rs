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
use pi_coding_agent::core::extensions::{
    ExtensionContext, ExtensionMode, ExtensionUiContext, ExtensionUiDialogOptions,
};
use pi_coding_agent::core::tools::ask_questions::{
    AskQuestionsTool, close_ask_questions_notification, notify_ask_questions_waiting, parameters,
};
use serde_json::{Value, json};

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

impl ExtensionUiContext for ScriptedUi {
    fn select<'a>(
        &'a self,
        title: &str,
        options: &[String],
        _opts: Option<&ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<Option<String>, String>> {
        self.seen_titles.lock().expect("titles lock").push(title.to_owned());
        self.seen_options.lock().expect("options lock").push(options.to_vec());
        let choice = self.selects.lock().expect("selects lock").pop_front().flatten();
        Box::pin(async move { Ok(choice) })
    }

    fn input<'a>(
        &'a self,
        _title: &str,
        _placeholder: Option<&str>,
        _opts: Option<&ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<Option<String>, String>> {
        let answer = self.inputs.lock().expect("inputs lock").pop_front().flatten();
        Box::pin(async move { Ok(answer) })
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

async fn context(ui: Arc<ScriptedUi>) -> ExtensionContext {
    tool_context::TestContextBuilder::new("/repo", tool_context::in_memory_session("/repo", "session"))
        .mode(ExtensionMode::Tui)
        .ui(ui)
        .build()
        .await
        .1
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
            context(Arc::clone(&ui)).await,
            None,
        )
        .await
        .expect("ask questions");

    assert_eq!(
        result.details.as_ref().expect("details")["answers"],
        json!({ "Which approach should we use?": "Direct API" })
    );
    assert_eq!(
        *ui.seen_titles.lock().expect("titles"),
        vec!["Which approach should we use?".to_owned()]
    );
    assert_eq!(
        *ui.seen_options.lock().expect("options"),
        vec![vec![
            "1. Direct API — Keep code simple".to_owned(),
            "2. Adapter — Add indirection".to_owned(),
            "Other".to_owned(),
            "Cancel".to_owned(),
        ]]
    );
}

#[tokio::test]
async fn custom_answers_trim_the_typed_text() {
    let ui = ScriptedUi::new(vec![Some("Other")], vec![Some("  Use a plugin  ")]);
    let result = AskQuestionsTool::new()
        .execute(
            "call-2".to_owned(),
            json!({ "questions": [{ "question": "What should we build?", "options": [{ "label": "Tool" }, { "label": "Extension" }] }] }),
            None,
            None,
            context(ui).await,
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
async fn multi_select_questions_collect_every_selected_label() {
    let ui = ScriptedUi::new(vec![Some("[ ] 1. Tests"), Some("[ ] 2. Docs"), Some("Done")], vec![]);
    let result = AskQuestionsTool::new()
        .execute(
            "call-3".to_owned(),
            json!({ "questions": [{ "question": "Which follow-ups should be included?", "multiSelect": true, "options": [{ "label": "Tests" }, { "label": "Docs" }] }] }),
            None,
            None,
            context(ui).await,
            None,
        )
        .await
        .expect("ask questions");

    assert_eq!(
        result.details.as_ref().expect("details")["answers"],
        json!({ "Which follow-ups should be included?": "Tests, Docs" })
    );
}

#[test]
fn sends_a_persistent_desktop_notification_while_waiting() {
    let notifier = MockNotifier::default();
    let notification = notify_ask_questions_waiting(&notifier, 1).expect("notification");

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
    drop(notifications);

    close_ask_questions_notification(Some(notification));
    assert_eq!(notifier.closes.load(Ordering::SeqCst), 1);
}

#[test]
fn uses_the_plural_body_for_multiple_questions() {
    let notifier = MockNotifier::default();
    let notification = notify_ask_questions_waiting(&notifier, 2).expect("notification");

    let notifications = notifier.notifications.lock().expect("notifications");
    assert_eq!(notifications[0].body, "Pi is waiting for your answers.");
    drop(notifications);

    close_ask_questions_notification(Some(notification));
    assert_eq!(notifier.closes.load(Ordering::SeqCst), 1);
}

#[test]
fn notification_body_never_includes_question_or_option_text() {
    let notifier = MockNotifier::default();
    let notification = notify_ask_questions_waiting(&notifier, 1).expect("notification");

    let notifications = notifier.notifications.lock().expect("notifications");
    assert!(!notifications[0].body.contains("secret-question-token"));
    assert!(!notifications[0].body.contains("secret option"));
    drop(notifications);

    close_ask_questions_notification(Some(notification));
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
            context(ui).await,
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
            context(ui).await,
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
            context(ui).await,
            None,
        )
        .await
        .expect_err("duplicate questions must fail");

    assert_eq!(error.to_string(), "Duplicate question text: Duplicate?");
}

#[tokio::test]
async fn returns_an_error_outside_interactive_tui_mode() {
    let context = tool_context::TestContextBuilder::new("/repo", tool_context::in_memory_session("/repo", "session"))
        .mode(ExtensionMode::Print)
        .build()
        .await
        .1;
    let result = AskQuestionsTool::new()
        .execute(
            "call-5".to_owned(),
            json!({ "questions": [{ "question": "Proceed?", "options": [{ "label": "Yes" }, { "label": "No" }] }] }),
            None,
            None,
            context,
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
            context(ui).await,
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
