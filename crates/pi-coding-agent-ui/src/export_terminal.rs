//! Read-only, compaction-aware ANSI history (`core/export-terminal.ts`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use pi_agent_core::AgentMessage;
use pi_ai::request::CallbackError;
use pi_ai::types::{AssistantContent, AssistantMessage, StopReason, TextContent, TextOrImageContent, UserContent};
use pi_js_compat::js_json::{self, JsString, JsValue};
use pi_js_compat::js_regex::JsRegex;
use pi_tui::Spacer;
use pi_tui::tui::{Component, Container, component};

use crate::core::extensions::types::CustomMessageValue;
use crate::core::messages::{BranchSummaryMessage, BranchSummaryRole, CompactionSummaryMessage};
use crate::core::parse_skill_block;
use crate::core::session_manager::{load_session_context_entries, session_entry_to_context_messages};
use crate::core::tools::truncate::{TruncationOptions, truncate_tail};
use crate::modes::interactive::components::assistant_message::AssistantMessageComponent;
use crate::modes::interactive::components::bash_execution::{BashExecutionComponent, BashExecutionOptions};
use crate::modes::interactive::components::branch_summary_message::BranchSummaryMessageComponent;
use crate::modes::interactive::components::compaction_summary_message::CompactionSummaryMessageComponent;
use crate::modes::interactive::components::custom_message::CustomMessageComponent;
use crate::modes::interactive::components::skill_invocation_message::SkillInvocationMessageComponent;
use crate::modes::interactive::components::tool_execution::{
    ToolExecutionComponent, ToolExecutionOptions, ToolExecutionResult,
};
use crate::modes::interactive::components::user_message::UserMessageComponent;
use crate::modes::interactive::theme::{current_theme, get_markdown_theme};
use crate::utils::ansi::strip_ansi;
use crate::utils::shell::sanitize_binary_output;

type PendingTools = HashMap<String, Rc<RefCell<ToolExecutionComponent>>>;

fn sanitize_saved_text(text: &JsString) -> JsString {
    let text = sanitize_binary_output(&strip_ansi(text))
        .replace("\r\n", "\n")
        .replace("\r", "\n");
    JsString::from_utf16(
        text.units()
            .iter()
            .copied()
            .filter(|unit| !(0x7f..=0x9f).contains(unit))
            .collect(),
    )
}

fn sanitize_value(value: &mut JsValue) {
    match value {
        JsValue::String(text) => *text = sanitize_saved_text(text),
        JsValue::Array(values) => values.iter_mut().for_each(sanitize_value),
        JsValue::Object(values) => values.values_mut().for_each(sanitize_value),
        _ => {}
    }
}

fn render_line(line: &JsString, styles: &JsRegex) -> JsString {
    let mut output = JsString::default();
    let mut start = 0;
    for style in styles.find_iter(line) {
        output.push_str(sanitize_saved_text(&line.slice(start, style.start())));
        output.push_str(style.text());
        start = style.end();
    }
    output.push_str(sanitize_saved_text(&line.slice(start, line.len())));
    output.push_str("\u{1b}[0m");
    output
}

fn add_user_message(chat: &mut Container, content: UserContent) {
    let text = match content {
        UserContent::Text(text) => text,
        UserContent::Blocks(blocks) => JsString::join(
            blocks.into_iter().filter_map(|part| match part {
                TextOrImageContent::Text(text) => Some(text.text),
                _ => None,
            }),
            "",
        ),
    };
    if text.is_empty() {
        return;
    }
    if !chat.children().is_empty() {
        chat.add_child(component(Spacer::new(1)));
    }
    if let Some(skill) = parse_skill_block(&text) {
        let user_message = skill.user_message.clone();
        chat.add_child(component(SkillInvocationMessageComponent::new(
            skill,
            Rc::from(get_markdown_theme()),
            current_theme(),
        )));
        if let Some(user_message) = user_message.filter(|message| !message.is_empty()) {
            chat.add_child(component(Spacer::new(1)));
            chat.add_child(component(UserMessageComponent::new(user_message, None, 1)));
        }
    } else {
        chat.add_child(component(UserMessageComponent::new(text, None, 1)));
    }
}

fn add_assistant_message(chat: &mut Container, message: AssistantMessage, pending: &mut PendingTools, cwd: &JsString) {
    chat.add_child(component(AssistantMessageComponent::new(
        Some(message.clone()),
        false,
        None,
        None,
        1,
    )));
    for content in message.content {
        let AssistantContent::ToolCall(call) = content else {
            continue;
        };
        let tool = ToolExecutionComponent::new(
            call.name,
            call.id.clone(),
            JsValue::Object(call.arguments),
            ToolExecutionOptions {
                show_images: Some(false),
                ..Default::default()
            },
            None,
            Arc::new(|| {}),
            cwd,
        );
        chat.add_child(tool.clone());
        if matches!(message.stop_reason, StopReason::Aborted | StopReason::Error) {
            let text = message
                .error_message
                .as_ref()
                .filter(|text| !text.is_empty())
                .cloned()
                .unwrap_or_else(|| {
                    if message.stop_reason == StopReason::Aborted {
                        "Operation aborted".into()
                    } else {
                        "Error".into()
                    }
                });
            tool.borrow_mut().update_result(
                ToolExecutionResult {
                    content: vec![TextOrImageContent::Text(TextContent::new(text))],
                    details: None,
                    is_error: true,
                },
                false,
                Some(message.timestamp),
            );
        } else {
            pending.insert(call.id, tool);
        }
    }
}

fn add_custom_message(chat: &mut Container, value: JsValue) -> Result<(), CallbackError> {
    let role = value["role"].as_string().cloned().unwrap_or_default();
    if role == "custom" {
        let message: CustomMessageValue =
            js_json::from_value(&value).map_err(|error| CallbackError::from(error.to_string()))?;
        if message.display {
            chat.add_child(component(CustomMessageComponent::new(message, None, None)));
        }
    } else if role == "branchSummary" {
        let message = BranchSummaryMessage {
            role: BranchSummaryRole::BranchSummary,
            summary: text_field(&value, "summary"),
            from_id: text_field(&value, "fromId"),
            timestamp: value["timestamp"].as_f64().unwrap_or_default(),
        };
        chat.add_child(component(Spacer::new(1)));
        chat.add_child(component(BranchSummaryMessageComponent::new(message, None)));
    } else if role == "compactionSummary" {
        let message: CompactionSummaryMessage =
            js_json::from_value(&value).map_err(|error| CallbackError::from(error.to_string()))?;
        chat.add_child(component(Spacer::new(1)));
        chat.add_child(component(CompactionSummaryMessageComponent::new(message, None)));
    } else if role == "bashExecution" {
        let mut bash = BashExecutionComponent::new(
            text_field(&value, "command"),
            Rc::new(|| {}),
            value["excludeFromContext"].as_bool().unwrap_or(false),
            BashExecutionOptions {
                show_elapsed: Some(false),
            },
        );
        bash.append_output(text_field(&value, "output"));
        let truncation = value["truncated"].as_bool().unwrap_or(false).then(|| {
            let mut result = truncate_tail("", TruncationOptions::default());
            result.truncated = true;
            result
        });
        bash.set_complete(
            value["exitCode"].as_f64(),
            value["cancelled"].as_bool().unwrap_or(false),
            truncation,
            value["fullOutputPath"].as_string().cloned(),
        );
        chat.add_child(component(bash));
    }
    Ok(())
}

fn text_field(value: &JsValue, key: &str) -> JsString {
    value[key].as_string().cloned().unwrap_or_default()
}

fn add_message(
    chat: &mut Container,
    message: AgentMessage,
    pending: &mut PendingTools,
    cwd: &JsString,
) -> Result<(), CallbackError> {
    match message {
        AgentMessage::User(user) => add_user_message(chat, user.content),
        AgentMessage::Assistant(message) => add_assistant_message(chat, *message, pending, cwd),
        AgentMessage::ToolResult(result) => {
            if let Some(tool) = pending.remove(&result.tool_call_id) {
                tool.borrow_mut().update_result(
                    ToolExecutionResult {
                        content: result.content,
                        details: result.details,
                        is_error: result.is_error,
                    },
                    false,
                    Some(result.timestamp),
                );
            }
        }
        AgentMessage::Custom(object) => add_custom_message(chat, JsValue::Object(object))?,
    }
    Ok(())
}

/// Render saved history without starting a terminal, settings, extensions or an agent.
pub fn export_session_to_terminal(session_path: &JsString, columns: f64) -> Result<JsString, CallbackError> {
    if !columns.is_finite() || columns.fract() != 0.0 || columns <= 0.0 || columns > 9_007_199_254_740_991.0 {
        return Err(CallbackError::from("Terminal columns must be a positive safe integer"));
    }
    let loaded =
        load_session_context_entries(session_path).map_err(|error| CallbackError::from(error.message().clone()))?;
    let cwd = loaded
        .header
        .cwd()
        .ok_or_else(|| CallbackError::from("Session header has no working directory"))?;
    let mut chat = Container::new();
    let mut pending = PendingTools::new();
    for entry in loaded.entries {
        for message in session_entry_to_context_messages(&entry) {
            let mut value = js_json::to_value(&message).map_err(|error| CallbackError::from(error.to_string()))?;
            sanitize_value(&mut value);
            let message = js_json::from_value(&value).map_err(|error| CallbackError::from(error.to_string()))?;
            add_message(&mut chat, message, &mut pending, cwd)?;
        }
    }
    let styles = JsRegex::new("\u{1b}\\[[0-9;:]*m").map_err(|error| CallbackError::from(error.to_string()))?;
    Ok(JsString::join(
        chat.render(columns as usize)
            .iter()
            .map(|line| render_line(line, &styles)),
        "\n",
    ) + "\n")
}
