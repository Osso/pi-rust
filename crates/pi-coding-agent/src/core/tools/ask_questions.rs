//! `ask_questions` tool (`core/tools/ask-questions.ts`).

use std::collections::HashSet;
use std::sync::{Arc, OnceLock};

use pi_agent_core::{
    AgentHarnessTool, AgentToolExecutionContext, AgentToolResult, AgentToolUpdateCallback, ToolError, ToolExecutionMode,
};
use pi_ai::request::BoxFuture;
use pi_js_compat::string::js_trim;
use pi_tui::tui::ComponentRef;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use tokio_util::sync::CancellationToken;

use super::result::{error_text_result, text_result};
use crate::core::desktop_notification::{
    DesktopNotification, DesktopNotificationHandle, DesktopNotificationUrgency, DesktopNotifier,
    PERSISTENT_DESKTOP_NOTIFICATION_EXPIRE_TIME_MS, send_desktop_notification,
};
use crate::core::extensions::types::{
    ExtensionContext, ExtensionMode, ExtensionUiContext, ToolDefinition, ToolRenderContext, ToolRenderResultOptions,
    infallible_render_call, infallible_render_result,
};
use crate::modes::interactive::theme::{Theme, ThemeColor};

const MIN_OPTIONS: usize = 2;
const MAX_OPTIONS: usize = 4;
const MAX_QUESTIONS: usize = 4;
const OTHER_LABEL: &str = "Other";
const DONE_LABEL: &str = "Done";
const CANCEL_LABEL: &str = "Cancel";
const ASK_QUESTIONS_NOTIFICATION_TITLE: &str = "Pi question needs input";

pub const NAME: &str = "ask_questions";
pub const LABEL: &str = "ask_questions";
pub const DESCRIPTION: &str = "Ask the user multiple-choice questions to clarify requirements, gather preferences, or choose between approaches. Similar to Claude Code's AskUserQuestion tool.";
pub const PROMPT_SNIPPET: &str = "Ask the user structured multiple-choice clarifying questions";
pub const PROMPT_GUIDELINES: [&str; 4] = [
    "Use ask_questions only in interactive TUI sessions when you need user preferences, requirement clarification, or a decision between approaches.",
    "Do not ask free-form clarifying questions in chat when ask_questions can present clear options.",
    "Provide 2-4 distinct options. Do not include an Other option; Pi adds it automatically.",
    "If you recommend an option, make it first and add '(Recommended)' to its label.",
];

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuestionOption {
    label: String,
    description: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Question {
    question: String,
    options: Vec<QuestionOption>,
    multi_select: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct AskQuestionsParams {
    questions: Vec<Question>,
    metadata: Option<Map<String, Value>>,
}

#[derive(Debug, Clone)]
struct SelectionOption {
    label: String,
    value: String,
    custom: bool,
}

/// `createAskQuestionsToolDefinition()`.
#[derive(Debug, Default, Clone, Copy)]
pub struct AskQuestionsTool;

impl AskQuestionsTool {
    pub fn new() -> Self {
        Self
    }
}

pub fn parameters() -> &'static Value {
    static PARAMETERS: OnceLock<Value> = OnceLock::new();
    PARAMETERS.get_or_init(|| {
        json!({
            "type": "object",
            "properties": {
                "questions": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": MAX_QUESTIONS,
                    "description": "Questions to ask the user (1-4 questions).",
                    "items": {
                        "type": "object",
                        "properties": {
                            "question": {
                                "type": "string",
                                "description": "The complete question to ask the user. Should be clear, specific, and end with a question mark."
                            },
                            "header": {
                                "type": "string",
                                "description": "Very short label displayed in summaries. Examples: 'Auth', 'Library', 'Approach'."
                            },
                            "options": {
                                "type": "array",
                                "minItems": MIN_OPTIONS,
                                "maxItems": MAX_OPTIONS,
                                "description": "Available choices for this question. Provide 2-4 distinct choices; the UI adds an Other option automatically.",
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "label": {
                                            "type": "string",
                                            "description": "Concise display text for this option. Should be unique within the question."
                                        },
                                        "description": {
                                            "type": "string",
                                            "description": "Optional explanation of this option's meaning or trade-offs."
                                        },
                                        "preview": {
                                            "type": "string",
                                            "description": "Optional preview content for compatibility with AskUserQuestion-style callers. Pi currently returns it in details but does not render a dedicated preview pane."
                                        }
                                    },
                                    "required": ["label"]
                                }
                            },
                            "multiSelect": {
                                "type": "boolean",
                                "description": "Set true to allow multiple answers instead of one mutually exclusive answer."
                            }
                        },
                        "required": ["question", "options"]
                    }
                },
                "metadata": {
                    "type": "object",
                    "description": "Optional metadata for callers. Not displayed to the user."
                }
            },
            "required": ["questions"]
        })
    })
}

fn validate_questions(questions: &[Question]) -> Result<(), ToolError> {
    if questions.is_empty() || questions.len() > MAX_QUESTIONS {
        return Err(ToolError::new(format!(
            "ask_questions requires 1-{MAX_QUESTIONS} questions"
        )));
    }
    let mut question_texts = HashSet::new();
    for question in questions {
        if !question_texts.insert(question.question.as_str()) {
            return Err(ToolError::new(format!(
                "Duplicate question text: {}",
                question.question
            )));
        }
        if question.options.len() < MIN_OPTIONS || question.options.len() > MAX_OPTIONS {
            return Err(ToolError::new(format!(
                "Question \"{}\" requires {MIN_OPTIONS}-{MAX_OPTIONS} options",
                question.question
            )));
        }
        let mut option_labels = HashSet::new();
        for option in &question.options {
            if !option_labels.insert(option.label.as_str()) {
                return Err(ToolError::new(format!(
                    "Duplicate option label \"{}\" in question \"{}\"",
                    option.label, question.question
                )));
            }
        }
    }
    Ok(())
}

fn format_option(option: &QuestionOption, index: usize) -> String {
    let suffix = option
        .description
        .as_ref()
        .map(|description| format!(" — {description}"))
        .unwrap_or_default();
    format!("{}. {}{suffix}", index + 1, option.label)
}

fn build_selection_options(question: &Question) -> Vec<SelectionOption> {
    let mut options: Vec<SelectionOption> = question
        .options
        .iter()
        .enumerate()
        .map(|(index, option)| SelectionOption {
            label: format_option(option, index),
            value: option.label.clone(),
            custom: false,
        })
        .collect();
    options.push(SelectionOption {
        label: OTHER_LABEL.to_owned(),
        value: OTHER_LABEL.to_owned(),
        custom: true,
    });
    options
}

async fn ask_custom_answer(ui: &dyn ExtensionUiContext, title: &str) -> Result<Option<String>, ToolError> {
    let answer = ui
        .input(title, Some("Type your answer"), None)
        .await
        .map_err(ToolError::new)?;
    let trimmed = answer.as_deref().map(js_trim).unwrap_or_default();
    Ok((!trimmed.is_empty()).then(|| trimmed.to_owned()))
}

async fn ask_single_question(ui: &dyn ExtensionUiContext, question: &Question) -> Result<Option<String>, ToolError> {
    let options = build_selection_options(question);
    let mut labels: Vec<String> = options.iter().map(|option| option.label.clone()).collect();
    labels.push(CANCEL_LABEL.to_owned());
    let Some(choice) = ui
        .select(&question.question, &labels, None)
        .await
        .map_err(ToolError::new)?
    else {
        return Ok(None);
    };
    if choice == CANCEL_LABEL {
        return Ok(None);
    }
    let Some(selected) = options.iter().find(|option| option.label == choice) else {
        return Ok(None);
    };
    if selected.custom {
        return ask_custom_answer(ui, &question.question).await;
    }
    Ok(Some(selected.value.clone()))
}

fn format_multi_select_option(option: &SelectionOption, selected: &[String]) -> String {
    if option.custom {
        return option.label.clone();
    }
    let marker = if selected.contains(&option.value) { "[x]" } else { "[ ]" };
    format!("{marker} {}", option.label)
}

fn read_multi_select_choice<'a>(
    options: &'a [SelectionOption],
    choice: &str,
    selected: &[String],
) -> Option<&'a SelectionOption> {
    options
        .iter()
        .find(|option| format_multi_select_option(option, selected) == choice)
}

async fn apply_multi_select_choice(
    ui: &dyn ExtensionUiContext,
    question: &Question,
    option: &SelectionOption,
    selected: &mut Vec<String>,
) -> Result<(), ToolError> {
    if !option.custom {
        if let Some(index) = selected.iter().position(|value| value == &option.value) {
            selected.remove(index);
        } else {
            selected.push(option.value.clone());
        }
        return Ok(());
    }
    if let Some(custom_answer) = ask_custom_answer(ui, &question.question).await? {
        selected.push(custom_answer);
    }
    Ok(())
}

async fn ask_multi_select_question(
    ui: &dyn ExtensionUiContext,
    question: &Question,
) -> Result<Option<String>, ToolError> {
    let options = build_selection_options(question);
    let mut selected: Vec<String> = Vec::new();
    loop {
        let mut labels: Vec<String> = options
            .iter()
            .map(|option| format_multi_select_option(option, &selected))
            .collect();
        labels.push(DONE_LABEL.to_owned());
        labels.push(CANCEL_LABEL.to_owned());
        let Some(choice) = ui
            .select(&question.question, &labels, None)
            .await
            .map_err(ToolError::new)?
        else {
            return Ok(None);
        };
        if choice == CANCEL_LABEL {
            return Ok(None);
        }
        if choice == DONE_LABEL {
            return Ok(Some(selected.join(", ")));
        }
        let Some(option) = read_multi_select_choice(&options, &choice, &selected) else {
            return Ok(None);
        };
        apply_multi_select_choice(ui, question, option, &mut selected).await?;
    }
}

async fn ask_question(ui: &dyn ExtensionUiContext, question: &Question) -> Result<Option<String>, ToolError> {
    if question.multi_select == Some(true) {
        ask_multi_select_question(ui, question).await
    } else {
        ask_single_question(ui, question).await
    }
}

fn format_answers(answers: &[(String, String)]) -> String {
    answers
        .iter()
        .map(|(question, answer)| {
            let answer = if answer.is_empty() { "(no selection)" } else { answer };
            format!("- {question} → {answer}")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn answers_json(answers: &[(String, String)]) -> Value {
    let mut map = Map::new();
    for (question, answer) in answers {
        map.insert(question.clone(), Value::String(answer.clone()));
    }
    Value::Object(map)
}

fn details_json(
    questions: &Value,
    answers: &[(String, String)],
    cancelled: bool,
    metadata: Option<&Map<String, Value>>,
) -> Value {
    let mut details = Map::new();
    details.insert("questions".to_owned(), questions.clone());
    details.insert("answers".to_owned(), answers_json(answers));
    details.insert("cancelled".to_owned(), Value::Bool(cancelled));
    if let Some(metadata) = metadata {
        details.insert("metadata".to_owned(), Value::Object(metadata.clone()));
    }
    Value::Object(details)
}

fn unavailable_result(params: &AskQuestionsParams, questions: &Value) -> AgentToolResult {
    error_text_result(
        "Error: ask_questions requires an interactive TUI session",
        details_json(questions, &[], true, params.metadata.as_ref()),
    )
}

fn cancelled_result(params: &AskQuestionsParams, questions: &Value, answers: &[(String, String)]) -> AgentToolResult {
    text_result(
        "Questions cancelled",
        details_json(questions, answers, true, params.metadata.as_ref()),
    )
}

fn answered_result(params: &AskQuestionsParams, questions: &Value, answers: &[(String, String)]) -> AgentToolResult {
    text_result(
        format!("User answered questions:\n{}", format_answers(answers)),
        details_json(questions, answers, false, params.metadata.as_ref()),
    )
}

fn format_ask_questions_notification_body(question_count: usize) -> &'static str {
    if question_count == 1 {
        "Pi is waiting for your answer."
    } else {
        "Pi is waiting for your answers."
    }
}

fn system_notifier() -> Arc<dyn DesktopNotifier> {
    Arc::new(SystemDesktopNotifier)
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
        body: format_ask_questions_notification_body(question_count).to_owned(),
        expire_time_ms: Some(PERSISTENT_DESKTOP_NOTIFICATION_EXPIRE_TIME_MS),
        title: ASK_QUESTIONS_NOTIFICATION_TITLE.to_owned(),
        urgency: Some(DesktopNotificationUrgency::Normal),
    })
}

pub fn close_ask_questions_notification(notification: Option<Box<dyn DesktopNotificationHandle>>) {
    if let Some(notification) = notification {
        notification.close();
    }
}

impl AgentHarnessTool<ExtensionContext> for AskQuestionsTool {
    fn name(&self) -> &str {
        NAME
    }

    fn description(&self) -> &str {
        DESCRIPTION
    }

    fn parameters(&self) -> &Value {
        parameters()
    }

    fn label(&self) -> &str {
        LABEL
    }

    fn execution_mode(&self) -> Option<ToolExecutionMode> {
        Some(ToolExecutionMode::Sequential)
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: Value,
        _signal: Option<CancellationToken>,
        _on_update: Option<AgentToolUpdateCallback>,
        context: ExtensionContext,
        _execution: Option<AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        Box::pin(async move {
            let questions = params
                .get("questions")
                .cloned()
                .unwrap_or_else(|| Value::Array(Vec::new()));
            let parsed: AskQuestionsParams =
                serde_json::from_value(params).map_err(|error| ToolError::new(error.to_string()))?;
            let has_ui = context.has_ui().map_err(|error| ToolError::new(error.message))?;
            let mode = context.mode().map_err(|error| ToolError::new(error.message))?;
            if !has_ui || mode != ExtensionMode::Tui {
                return Ok(unavailable_result(&parsed, &questions));
            }
            let ui = context.ui().map_err(|error| ToolError::new(error.message))?;
            validate_questions(&parsed.questions)?;
            let notifier = system_notifier();
            let notification = notify_ask_questions_waiting(notifier.as_ref(), parsed.questions.len());
            let result = ask_all_questions(ui.as_ref(), &parsed, &questions).await;
            close_ask_questions_notification(notification);
            result
        })
    }
}

async fn ask_all_questions(
    ui: &dyn ExtensionUiContext,
    params: &AskQuestionsParams,
    questions: &Value,
) -> Result<AgentToolResult, ToolError> {
    let mut answers: Vec<(String, String)> = Vec::new();
    for question in &params.questions {
        let Some(answer) = ask_question(ui, question).await? else {
            return Ok(cancelled_result(params, questions, &answers));
        };
        answers.push((question.question.clone(), answer));
    }
    Ok(answered_result(params, questions, &answers))
}

fn format_ask_questions_call(args: &Value, theme: &Theme) -> String {
    let count = args.get("questions").and_then(Value::as_array).map_or(0, Vec::len);
    let label = if count == 1 {
        "1 question".to_owned()
    } else {
        format!("{count} questions")
    };
    format!(
        "{} {}",
        theme.fg(ThemeColor::ToolTitle, &theme.bold("ask_questions")),
        theme.fg(ThemeColor::Accent, &label)
    )
}

fn format_details_answers(answers: &Map<String, Value>) -> String {
    answers
        .iter()
        .map(|(question, answer)| {
            let answer = answer.as_str().unwrap_or("");
            let answer = if answer.is_empty() { "(no selection)" } else { answer };
            format!("- {question} → {answer}")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_ask_questions_result(details: Option<&Value>, theme: &Theme) -> String {
    let Some(details) = details.filter(|details| !details.is_null()) else {
        return String::new();
    };
    if details.get("cancelled").and_then(Value::as_bool) == Some(true) {
        return theme.fg(ThemeColor::Warning, "Questions cancelled");
    }
    let answers = details
        .get("answers")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    theme.fg(
        ThemeColor::ToolOutput,
        &format!("User answered:\n{}", format_details_answers(&answers)),
    )
}

/// `renderCall` for the ask_questions tool.
pub fn render_call(args: &Value, theme: &Theme, context: &ToolRenderContext) -> ComponentRef {
    super::render_text_component(context, format_ask_questions_call(args, theme))
}

/// `renderResult` for the ask_questions tool.
pub fn render_result(
    result: &AgentToolResult,
    _options: &ToolRenderResultOptions,
    theme: &Theme,
    context: &ToolRenderContext,
) -> ComponentRef {
    super::render_text_component(context, format_ask_questions_result(result.details.as_ref(), theme))
}

/// `createAskQuestionsToolDefinition()`.
pub fn create_ask_questions_tool_definition() -> ToolDefinition {
    let mut definition = crate::core::extensions::wrapper::harness_tool_definition(Arc::new(AskQuestionsTool::new()));
    definition.prompt_snippet = Some(PROMPT_SNIPPET.to_owned());
    definition.prompt_guidelines = Some(PROMPT_GUIDELINES.iter().map(|line| (*line).to_owned()).collect());
    definition.approval_required = Some(false);
    definition.render_call = Some(infallible_render_call(render_call));
    definition.render_result = Some(infallible_render_result(render_result));
    definition
}
