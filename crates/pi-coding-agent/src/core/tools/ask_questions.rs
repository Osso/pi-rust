//! Frozen `core/tools/ask-questions.ts`: explicit blocking, per-UI FIFO and inbox answers.

use super::result::{error_text_result, text_result};
use crate::core::desktop_notification::{
    DesktopNotification, DesktopNotificationHandle, DesktopNotificationUrgency, DesktopNotifier,
    PERSISTENT_DESKTOP_NOTIFICATION_EXPIRE_TIME_MS, send_desktop_notification,
};
use crate::core::extensions::types::{
    ExtensionContext, ExtensionMode, ExtensionUiContext, NotifyType, ToolDefinition, ToolRenderContext,
    ToolRenderResultOptions, infallible_render_call, infallible_render_result,
};
use crate::core::session_control_db::{
    EnqueueStoredRuntimeMailboxMessageInput, RuntimeMailboxAddress, RuntimeMailboxMessageKind, RuntimeMailboxStoreRef,
    enqueue_stored_runtime_mailbox_message,
};
use crate::core::session_manager::lock_session_manager;
use crate::modes::interactive::theme::{Theme, ThemeColor};
use pi_agent_core::{
    AgentHarnessTool, AgentToolExecutionContext, AgentToolResult, AgentToolUpdateCallback, ToolError, ToolExecutionMode,
};
use pi_ai::request::BoxFuture;
use pi_js_compat::js_format;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsValue as Value;
use pi_js_compat::js_json::{JsString, JsValue, encode_node_utf8};
use pi_tui::tui::ComponentRef;
use serde::{Deserialize, Serialize};
use serde_json::Value as SchemaValue;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

pub const NAME: &str = "ask_questions";
pub const LABEL: &str = "ask_questions";
pub const DESCRIPTION: &str = "Ask the user questions with choices or free-text answers. Explicitly choose blocking to wait, or nonblocking to queue questions and receive answers through your inbox while work continues.";
pub const PROMPT_SNIPPET: &str = "Ask all user questions through this tool; explicitly choose blocking or nonblocking";
pub const PROMPT_GUIDELINES: [&str; 5] = [
    "Use ask_questions only in interactive TUI sessions when you need user preferences, requirement clarification, or a decision between approaches.",
    "Always use ask_questions for questions to the user; never ask for user input in chat. Omit options for free-text questions.",
    "Always specify blocking explicitly. Use true when progress depends on the answer; use false to queue questions and continue independent authorized work. Nonblocking answers and cancellation arrive as inbox messages correlated by requestId.",
    "For multiple-choice questions, provide 2-4 distinct options. Do not include an Other option; Pi adds it automatically.",
    "If you recommend an option, make it first and add '(Recommended)' to its label.",
];
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QuestionOption {
    pub label: JsString,
    pub description: Option<JsString>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Question {
    pub question: JsString,
    pub header: Option<JsString>,
    pub options: Option<Vec<QuestionOption>>,
    pub multi_select: Option<bool>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct AskQuestionsParams {
    pub blocking: bool,
    pub questions: Vec<Question>,
}

pub fn parameters() -> &'static SchemaValue {
    static PARAMETERS: OnceLock<SchemaValue> = OnceLock::new();
    PARAMETERS.get_or_init(|| serde_json::json!({
        "type":"object", "required":["blocking","questions"], "properties": {
            "blocking":{"type":"boolean","description":"Required. True waits for answers; false queues questions and returns immediately, with answers delivered later to your inbox. Continue only work independent of unanswered questions."},
            "questions":{"type":"array","minItems":1,"maxItems":4,"description":"Questions to ask the user (1-4 questions).","items":{
                "type":"object","required":["question"],"properties":{
                    "question":{"type":"string","description":"The complete question to ask the user. Should be clear, specific, and end with a question mark."},
                    "header":{"type":"string","description":"Very short label displayed in summaries. Examples: 'Auth', 'Library', 'Approach'."},
                    "options":{"type":"array","minItems":2,"maxItems":4,"description":"Provide 2-4 distinct choices; Pi adds Other automatically. Omit options for a free-text question.","items":{
                        "type":"object","required":["label"],"properties":{
                            "label":{"type":"string","description":"Concise display text for this option. Should be unique within the question."},
                            "description":{"type":"string","description":"Optional explanation of this option's meaning or trade-offs."},
                            "preview":{"type":"string","description":"Optional preview content for compatibility with AskUserQuestion-style callers. Pi currently returns it in details but does not render a dedicated preview pane."}
                        }
                    }},
                    "multiSelect":{"type":"boolean","description":"Set true to allow multiple answers instead of one mutually exclusive answer."}
                }
            }},
            "metadata":{"type":"object","patternProperties":{"^.*$":{}},"description":"Optional metadata for callers. Not displayed to the user."}
        }
    }))
}
pub fn parse_params(params: &JsValue) -> Result<AskQuestionsParams, ToolError> {
    if params.get("blocking").and_then(Value::as_bool).is_none() {
        return Err(ToolError::new(
            "ask_questions requires explicit blocking: true or false",
        ));
    }
    pi_js_compat::js_json::from_value(params).map_err(|error| ToolError::new(error.to_string()))
}
pub fn validate_questions(questions: &[Question]) -> Result<(), ToolError> {
    if questions.is_empty() || questions.len() > 4 {
        return Err(ToolError::new("ask_questions requires 1-4 questions"));
    }
    let mut texts = HashSet::new();
    for question in questions {
        if !texts.insert(&question.question) {
            return Err(ToolError::new(
                JsString::from("Duplicate question text: ") + &question.question,
            ));
        }
        let Some(options) = &question.options else {
            if question.multi_select == Some(true) {
                return Err(ToolError::new(
                    JsString::from("Free-text question \"") + &question.question + "\" cannot use multiSelect",
                ));
            }
            continue;
        };
        if !(2..=4).contains(&options.len()) {
            return Err(ToolError::new(
                JsString::from("Question \"") + &question.question + "\" requires 2-4 options",
            ));
        }
        let mut labels = HashSet::new();
        for option in options {
            if !labels.insert(&option.label) {
                return Err(ToolError::new(
                    JsString::from("Duplicate option label \"")
                        + &option.label
                        + "\" in question \""
                        + &question.question
                        + "\"",
                ));
            }
        }
    }
    Ok(())
}
pub fn question_result(
    params: &JsValue,
    text: impl Into<JsString>,
    answers: Value,
    cancelled: bool,
) -> AgentToolResult {
    let mut details =
        json!({"blocking":params["blocking"],"questions":params["questions"],"answers":answers,"cancelled":cancelled});
    if let Some(metadata) = params.get("metadata") {
        details["metadata"] = metadata.clone();
    }
    text_result(text, details)
}
pub fn format_answers(answers: &Value) -> JsString {
    let Some(object) = answers.as_object() else {
        return JsString::default();
    };
    JsString::join(
        pi_js_compat::js_json::object_entries_lossless(object)
            .into_iter()
            .map(|(question, answer)| {
                let answer = answer.as_string().cloned().unwrap_or_default();
                let text = if answer.is_empty() {
                    JsString::from("(no selection)")
                } else {
                    answer
                };
                JsString::from("- ") + question + " → " + text
            }),
        "\n",
    )
}

#[derive(Clone, Deserialize)]
struct PanelResult {
    answers: Value,
    cancelled: bool,
}
#[cfg(test)]
mod raw_queue_tests {
    use super::*;

    #[test]
    fn queued_answer_body_keeps_raw_request_question_and_answer_units() {
        let raw = JsString::from_utf16(vec![0xd800]);
        let request = JsString::from("request-") + &raw;
        let result = PanelResult {
            answers: pi_js_compat::js_json::parse_lossless(r#"{"question-\ud800":"answer-\udfff"}"#).unwrap(),
            cancelled: false,
        };
        assert_eq!(
            format_queued_result(&request, &result),
            JsString::from("User answered questions (request ")
                + &request
                + "):\n- question-"
                + raw
                + " → answer-"
                + JsString::from_utf16(vec![0xdfff])
        );
    }
}

struct QuestionJob {
    // A pending panel owns its serialization boundary even if its caller is aborted.
    _queue: Arc<QuestionQueue>,
    questions: Vec<Question>,
    result: oneshot::Sender<Result<PanelResult, JsString>>,
}
struct QuestionQueue {
    sender: mpsc::UnboundedSender<QuestionJob>,
}
type Queues = Mutex<HashMap<usize, Weak<QuestionQueue>>>;
fn queue_questions(
    ui: Arc<dyn ExtensionUiContext>,
    questions: Vec<Question>,
) -> (Arc<QuestionQueue>, oneshot::Receiver<Result<PanelResult, JsString>>) {
    static QUEUES: OnceLock<Queues> = OnceLock::new();
    let mut queues = QUEUES.get_or_init(Mutex::default).lock().expect("question queues");
    queues.retain(|_, queue| queue.strong_count() > 0);
    let key = Arc::as_ptr(&ui) as *const () as usize;
    let queue = queues.get(&key).and_then(Weak::upgrade).unwrap_or_else(|| {
        let (sender, mut receiver) = mpsc::unbounded_channel::<QuestionJob>();
        tokio::spawn(async move {
            while let Some(job) = receiver.recv().await {
                let notification = notify_ask_questions_waiting(&SystemDesktopNotifier, job.questions.len());
                let result = ui
                    .custom(
                        Arc::new(move |context, done| {
                            crate::core::extensions::types::ready_custom_component(Box::new(
                                super::ask_questions_panel::QuestionPanel::new(job.questions.clone(), context, done),
                            ))
                        }),
                        None,
                    )
                    .await
                    .and_then(|value| {
                        pi_js_compat::js_json::from_value(&value.unwrap_or(Value::Null))
                            .map_err(|error| JsString::from(error.to_string()))
                    });
                close_ask_questions_notification(notification);
                let _ = job.result.send(result);
            }
        });
        let queue = Arc::new(QuestionQueue { sender });
        queues.insert(key, Arc::downgrade(&queue));
        queue
    });
    let (sender, receiver) = oneshot::channel();
    queue
        .sender
        .send(QuestionJob {
            _queue: Arc::clone(&queue),
            questions,
            result: sender,
        })
        .expect("question worker active");
    (queue, receiver)
}
#[derive(Clone)]
struct InboxTarget {
    db: JsString,
    path: JsString,
    recipient: RuntimeMailboxAddress,
}
fn capture_inbox(context: &ExtensionContext) -> Result<InboxTarget, ToolError> {
    let db = context
        .control_db_path()
        .map_err(|error| ToolError::new(error.message))?;
    let session = context
        .session_manager()
        .map_err(|error| ToolError::new(error.message))?;
    let manager = lock_session_manager(&session);
    let (Some(db), Some(path)) = (db, manager.get_session_file()) else {
        return Err(ToolError::new(
            "Nonblocking ask_questions requires a persisted session and control database",
        ));
    };
    Ok(InboxTarget {
        db,
        path: path.to_owned(),
        recipient: RuntimeMailboxAddress {
            session_id: manager.get_session_id().into(),
            agent_id: context
                .multi_agent_agent_id()
                .map_err(|error| ToolError::new(error.message))?,
        },
    })
}
fn format_queued_result(request: &JsString, result: &PanelResult) -> JsString {
    let title = if result.cancelled {
        js_format!("Questions cancelled (request {})", request)
    } else {
        js_format!("User answered questions (request {}):", request)
    };
    title + "\n" + format_answers(&result.answers)
}

async fn deliver_queued(
    ui: Arc<dyn ExtensionUiContext>,
    target: InboxTarget,
    request: JsString,
    pending: oneshot::Receiver<Result<PanelResult, JsString>>,
    _queue: Arc<QuestionQueue>,
) {
    let body = match pending
        .await
        .map_err(|error| JsString::from(error.to_string()))
        .and_then(|result| result)
    {
        Ok(result) => format_queued_result(&request, &result),
        Err(error) => {
            let body = js_format!("Questions failed (request {}): {}", request, error);
            ui.notify(&body, Some(NotifyType::Error));
            body.into()
        }
    };
    let message_id = js_format!("ask_questions:{}", request);
    let input = EnqueueStoredRuntimeMailboxMessageInput {
        recipient_host: None,
        recipient: target.recipient.clone(),
        sender: RuntimeMailboxAddress {
            session_id: target.recipient.session_id.clone(),
            agent_id: None,
        },
        kind: RuntimeMailboxMessageKind::Reply,
        store_ref: RuntimeMailboxStoreRef {
            message_id: message_id.clone().into(),
            session_path: target.path.into(),
        },
        updated_at: None,
        message: json!({"body":body,"id":message_id,"kind":"reply","status":"pending","fromAgentId":"main","toAgentId":target.recipient.agent_id.as_ref().cloned().unwrap_or_else(|| "main".into())}),
    };
    if let Err(error) = enqueue_stored_runtime_mailbox_message(&target.db, &input) {
        let message = js_format!(
            "Failed to deliver question answer (request {}): {}",
            request,
            error.message()
        );
        eprintln!("{}", encode_node_utf8(&message));
        ui.notify(&message, Some(NotifyType::Error));
    }
}
#[derive(Debug, Default, Clone, Copy)]
pub struct AskQuestionsTool;
impl AskQuestionsTool {
    pub fn new() -> Self {
        Self
    }
}
impl AgentHarnessTool<ExtensionContext> for AskQuestionsTool {
    fn name(&self) -> &str {
        NAME
    }
    fn label(&self) -> &str {
        LABEL
    }
    fn description(&self) -> &str {
        DESCRIPTION
    }
    fn parameters(&self) -> &SchemaValue {
        parameters()
    }
    fn execution_mode(&self) -> Option<ToolExecutionMode> {
        Some(ToolExecutionMode::Sequential)
    }
    fn execute(
        &self,
        tool_call_id: String,
        params: JsValue,
        _signal: Option<CancellationToken>,
        _on_update: Option<AgentToolUpdateCallback>,
        context: ExtensionContext,
        _execution: Option<AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        Box::pin(async move {
            let parsed = parse_params(&params)?;
            let has_ui = context.has_ui().map_err(|error| ToolError::new(error.message))?;
            let mode = context.mode().map_err(|error| ToolError::new(error.message))?;
            if !has_ui || mode != ExtensionMode::Tui {
                let result = question_result(
                    &params,
                    "Error: ask_questions requires an interactive TUI session",
                    json!({}),
                    true,
                );
                return Ok(error_text_result(
                    "Error: ask_questions requires an interactive TUI session",
                    result.details.unwrap(),
                ));
            }
            validate_questions(&parsed.questions)?;
            let ui = context.ui().map_err(|error| ToolError::new(error.message))?;
            let target = if parsed.blocking {
                None
            } else {
                Some(capture_inbox(&context)?)
            };
            let (queue, pending) = queue_questions(ui.clone(), parsed.questions);
            if let Some(target) = target {
                tokio::spawn(deliver_queued(ui, target, tool_call_id.clone().into(), pending, queue));
                let mut result = question_result(
                    &params,
                    format!(
                        "Questions queued (request {tool_call_id}). Continue independent work; answers will arrive in your inbox."
                    ),
                    json!({}),
                    false,
                );
                let details = result.details.as_mut().unwrap();
                details["queued"] = json!(true).into();
                details["requestId"] = json!(tool_call_id).into();
                return Ok(result);
            }
            let result = pending
                .await
                .map_err(|error| ToolError::new(error.to_string()))?
                .map_err(ToolError::new)?;
            drop(queue);
            let text = if result.cancelled {
                JsString::from("Questions cancelled")
            } else {
                JsString::from("User answered questions:\n") + format_answers(&result.answers)
            };
            Ok(question_result(&params, text, result.answers, result.cancelled))
        })
    }
}
struct SystemDesktopNotifier;
impl DesktopNotifier for SystemDesktopNotifier {
    fn notify(&self, notification: &DesktopNotification) -> Option<Box<dyn DesktopNotificationHandle>> {
        send_desktop_notification(notification)
    }
}
pub fn notify_ask_questions_waiting(
    notifier: &dyn DesktopNotifier,
    question_count: usize,
) -> Option<Box<dyn DesktopNotificationHandle>> {
    notifier.notify(&DesktopNotification {
        title: "Pi question needs input".into(),
        body: if question_count == 1 {
            "Pi is waiting for your answer."
        } else {
            "Pi is waiting for your answers."
        }
        .into(),
        expire_time_ms: Some(PERSISTENT_DESKTOP_NOTIFICATION_EXPIRE_TIME_MS),
        urgency: Some(DesktopNotificationUrgency::Normal),
    })
}
pub fn close_ask_questions_notification(notification: Option<Box<dyn DesktopNotificationHandle>>) {
    if let Some(notification) = notification {
        notification.close();
    }
}
fn call_text(args: &JsValue, details: &Value, theme: &Theme) -> JsString {
    let questions = args["questions"].as_array().map(Vec::as_slice).unwrap_or_default();
    let label = if questions.len() == 1 {
        JsString::from("1 question")
    } else {
        js_format!("{} questions", questions.len())
    };
    let mut lines = vec![js_format!(
        "{} {}",
        theme.fg(ThemeColor::ToolTitle, &theme.bold("ask_questions")),
        theme.fg(ThemeColor::Accent, &label)
    )];
    for question in questions {
        let question = question["question"].as_string().cloned().unwrap_or_default();
        let mut line = js_format!("  {}", theme.fg(ThemeColor::ToolOutput, &question));
        if !details.is_null() {
            let suffix = if details["queued"].as_bool() == Some(true) {
                theme.fg(ThemeColor::Warning, "→ (queued)")
            } else if let Some(answer) = details["answers"]
                .as_object()
                .and_then(|answers| answers.get(&question))
                .and_then(Value::as_string)
            {
                let answer = if answer.is_empty() {
                    JsString::from("(no selection)")
                } else {
                    answer.clone()
                };
                theme.fg(ThemeColor::Accent, &js_format!("→ {}", answer))
            } else {
                theme.fg(ThemeColor::Warning, "→ (cancelled)")
            };
            line.push_str(js_format!(" {}", suffix));
        }
        lines.push(line);
    }
    JsString::join(&lines, "\n")
}
pub fn render_call(args: &JsValue, theme: &Theme, context: &ToolRenderContext) -> ComponentRef {
    super::render_text_component(context, call_text(args, &context.state.get()["details"], theme))
}
pub fn render_result(
    result: &AgentToolResult,
    _options: &ToolRenderResultOptions,
    theme: &Theme,
    context: &ToolRenderContext,
) -> ComponentRef {
    context
        .state
        .update(|state| state["details"] = result.details.clone().unwrap_or(Value::Null.into()));
    if let Some(component) = &context.call_component
        && let Some(text) = component
            .borrow_mut()
            .as_any_mut()
            .and_then(|component| component.downcast_mut::<pi_tui::Text>())
    {
        text.set_text(call_text(&context.args, &context.state.get()["details"], theme));
    }
    super::render_text_component(context, JsString::default())
}
pub fn create_ask_questions_tool_definition() -> ToolDefinition {
    let mut definition = crate::core::extensions::wrapper::harness_tool_definition(Arc::new(AskQuestionsTool::new()));
    definition.prompt_snippet = Some(PROMPT_SNIPPET.into());
    definition.prompt_guidelines = Some(PROMPT_GUIDELINES.iter().map(|line| (*line).into()).collect());
    definition.approval_required = Some(false);
    definition.render_call = Some(infallible_render_call(render_call));
    definition.render_result = Some(infallible_render_result(render_result));
    definition
}
