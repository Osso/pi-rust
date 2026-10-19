//! Port of `packages/coding-agent/extensions/goal/src/rendering.ts`.

use std::cell::RefCell;
use std::rc::Rc;

use pi_js_compat::date::date_parse_millis;
use pi_tui::components::r#box::BoxComponent;
use pi_tui::components::spacer::Spacer;
use pi_tui::components::text::Text;
use pi_tui::tui::{Component, ComponentRef};
use serde_json::Value;

use crate::core::extensions::types::{
    CustomMessageContent, CustomMessagePayload, CustomMessageValue, EntryRenderOptions, EntryRenderer, ExtensionApi,
    ExtensionContext, HandlerError, MessageRenderOptions, MessageRenderer, SendMessageOptions,
};
use crate::core::session_manager::{CustomEntry, lock_session_manager};
use crate::modes::interactive::theme::{Theme, ThemeBg, ThemeColor};

use super::wait_countdown::WaitCountdownRefresher;

const SUPERVISOR_INSTRUCTION_OPEN: &str = "<supervisor-instruction>";
const SUPERVISOR_INSTRUCTION_CLOSE: &str = "</supervisor-instruction>";
pub const SUPERVISOR_STATUS_TYPE: &str = "supervisor-status";
const REVIEW_DUE_TEXT: &str = "Review due…";

type TextStyle = Rc<dyn Fn(&str) -> String>;

struct SupervisorStatusStyles {
    background: TextStyle,
    countdown: TextStyle,
    label: TextStyle,
    message: TextStyle,
}

struct SupervisorStatusComponent {
    message: String,
    deadline_ms: Option<i64>,
    styles: SupervisorStatusStyles,
}

impl Component for SupervisorStatusComponent {
    fn render(&mut self, width: usize) -> Vec<String> {
        let mut container = BoxComponent::new(1, 1, Some(Rc::clone(&self.styles.background)));
        container
            .add_child(Rc::new(RefCell::new(Text::new((self.styles.label)("[Supervisor]"), 0, 0))) as ComponentRef);
        container.add_child(Rc::new(RefCell::new(Spacer::new(1))) as ComponentRef);
        container
            .add_child(Rc::new(RefCell::new(Text::new((self.styles.message)(&self.message), 0, 0))) as ComponentRef);
        if let Some(deadline_ms) = self.deadline_ms {
            let countdown = format_review_countdown(deadline_ms, now_millis());
            container
                .add_child(Rc::new(RefCell::new(Text::new((self.styles.countdown)(&countdown), 0, 0))) as ComponentRef);
        }
        container.render(width)
    }
}

pub struct SupervisorStatusController {
    pi: ExtensionApi,
    refresher: WaitCountdownRefresher,
}

pub type AppendSupervisorStatus =
    std::sync::Arc<dyn Fn(ExtensionContext, &str, Option<&str>) -> Result<(), HandlerError> + Send + Sync>;

impl SupervisorStatusController {
    /// `append`.
    pub fn append(&self, ctx: ExtensionContext, message: &str, review_at: Option<&str>) -> Result<(), HandlerError> {
        let session = ctx
            .session_manager()
            .map_err(|error| HandlerError::new(error.to_string()))?;
        let session_id = lock_session_manager(&session).get_session_id().to_owned();
        self.refresher.clear_session(&session_id);
        let data = match review_at {
            Some(review_at) => {
                let mut object = serde_json::Map::new();
                object.insert("message".to_owned(), Value::String(message.to_owned()));
                object.insert("reviewAt".to_owned(), Value::String(review_at.to_owned()));
                Some(Value::Object(object))
            }
            None => {
                let mut object = serde_json::Map::new();
                object.insert("message".to_owned(), Value::String(message.to_owned()));
                Some(Value::Object(object))
            }
        };
        self.pi
            .append_entry(SUPERVISOR_STATUS_TYPE, data)
            .map_err(|error| HandlerError::new(error.to_string()))?;
        if let Some(review_at) = review_at {
            self.refresher.start(ctx, review_at);
        }
        Ok(())
    }

    pub fn clear_all(&self) {
        self.refresher.clear_all();
    }

    pub fn clear_session(&self, session_id: &str) {
        self.refresher.clear_session(session_id);
    }

    /// `restore`.
    pub fn restore(&self, ctx: ExtensionContext) -> Result<(), HandlerError> {
        let session = ctx
            .session_manager()
            .map_err(|error| HandlerError::new(error.to_string()))?;
        let session_id = lock_session_manager(&session).get_session_id().to_owned();
        self.refresher.clear_session(&session_id);
        if let Some(review_at) = latest_future_review_at(&ctx)? {
            self.refresher.start(ctx, &review_at);
        }
        Ok(())
    }

    pub fn append_callback(self: &std::sync::Arc<Self>) -> AppendSupervisorStatus {
        let controller = std::sync::Arc::clone(self);
        std::sync::Arc::new(move |ctx, message, review_at| controller.append(ctx, message, review_at))
    }
}

/// `supervisorInstructionContent`.
pub fn supervisor_instruction_content(instructions: &str) -> String {
    format!("{SUPERVISOR_INSTRUCTION_OPEN}\n{instructions}\n{SUPERVISOR_INSTRUCTION_CLOSE}")
}

fn supervisor_message(content: &str) -> CustomMessagePayload {
    CustomMessagePayload {
        custom_type: "supervisor".to_owned(),
        content: CustomMessageContent::Text(supervisor_instruction_content(content)),
        display: true,
        details: None,
    }
}

/// `sendSupervisorInstructions`.
pub fn send_supervisor_instructions(pi: &ExtensionApi, instructions: &str) -> Result<(), HandlerError> {
    pi.send_message(
        supervisor_message(instructions),
        Some(SendMessageOptions {
            trigger_turn: Some(true),
            deliver_as: Some(crate::core::extensions::types::MessageDelivery::FollowUp),
        }),
    )
    .map_err(|error| HandlerError::new(error.to_string()))
}

fn has_supervisor_instruction_wrapper(content: &str) -> bool {
    content.starts_with(SUPERVISOR_INSTRUCTION_OPEN) && content.ends_with(SUPERVISOR_INSTRUCTION_CLOSE)
}

fn supervisor_instruction_body(content: &str) -> String {
    if !has_supervisor_instruction_wrapper(content) {
        return content.to_owned();
    }
    let mut body =
        content[SUPERVISOR_INSTRUCTION_OPEN.len()..content.len() - SUPERVISOR_INSTRUCTION_CLOSE.len()].to_owned();
    if body.starts_with('\n') {
        body.remove(0);
    }
    if body.ends_with('\n') {
        body.pop();
    }
    body
}

fn status_details(data: Option<&Value>) -> (String, Option<String>) {
    let Some(Value::Object(details)) = data else {
        return ("Supervisor status unavailable".to_owned(), None);
    };
    let message = details
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("Supervisor status unavailable")
        .to_owned();
    let review_at = details.get("reviewAt").and_then(Value::as_str).map(str::to_owned);
    (message, review_at)
}

fn format_review_countdown(deadline_ms: i64, now_ms: i64) -> String {
    let remaining_ms = deadline_ms - now_ms;
    if remaining_ms <= 0 {
        return REVIEW_DUE_TEXT.to_owned();
    }
    let remaining_seconds = (remaining_ms + 999) / 1_000;
    let minutes = remaining_seconds / 60;
    let seconds = format!("{:02}", remaining_seconds % 60);
    format!("Next review in {minutes}:{seconds}")
}

fn latest_future_review_at(ctx: &ExtensionContext) -> Result<Option<String>, HandlerError> {
    let session = ctx
        .session_manager()
        .map_err(|error| HandlerError::new(error.to_string()))?;
    let entries = lock_session_manager(&session).get_entries();
    for entry in entries.iter().rev() {
        let crate::core::session_manager::SessionEntry::Known(crate::core::session_manager::KnownSessionEntry::Custom(
            entry,
        )) = entry
        else {
            continue;
        };
        if entry.custom_type != SUPERVISOR_STATUS_TYPE {
            continue;
        }
        let (_, review_at) = status_details(entry.data.as_ref());
        let Some(review_at) = review_at else {
            return Ok(None);
        };
        let deadline_ms = date_parse_millis(&review_at);
        return Ok(deadline_ms
            .filter(|deadline| *deadline > now_millis())
            .map(|_| review_at));
    }
    Ok(None)
}

/// `createSupervisorStatusController`.
pub fn create_supervisor_status_controller(
    pi: &ExtensionApi,
    refresher: WaitCountdownRefresher,
) -> std::sync::Arc<SupervisorStatusController> {
    std::sync::Arc::new(SupervisorStatusController {
        pi: pi.clone(),
        refresher,
    })
}

/// `renderSupervisorStatusEntry`.
pub fn render_supervisor_status_entry(
    entry: &CustomEntry,
    _options: &EntryRenderOptions,
    theme: &Theme,
) -> Option<ComponentRef> {
    let (message, review_at) = status_details(entry.data.as_ref());
    let deadline_ms = review_at.as_deref().and_then(date_parse_millis);
    let background_theme = theme.clone();
    let countdown_theme = theme.clone();
    let label_theme = theme.clone();
    let message_theme = theme.clone();
    Some(Rc::new(RefCell::new(SupervisorStatusComponent {
        message,
        deadline_ms,
        styles: SupervisorStatusStyles {
            background: Rc::new(move |text| background_theme.bg(ThemeBg::CustomMessageBg, text)),
            countdown: Rc::new(move |text| countdown_theme.fg(ThemeColor::Dim, text)),
            label: Rc::new(move |text| label_theme.fg(ThemeColor::CustomMessageLabel, &label_theme.bold(text))),
            message: Rc::new(move |text| message_theme.fg(ThemeColor::CustomMessageText, text)),
        },
    })) as ComponentRef)
}

/// `renderSupervisorMessage`.
pub fn render_supervisor_message(
    message: &CustomMessageValue,
    _options: &MessageRenderOptions,
    theme: &Theme,
) -> Option<ComponentRef> {
    let content = match &message.content {
        CustomMessageContent::Text(text) => text.clone(),
        CustomMessageContent::Blocks(_) => String::new(),
    };
    let background_theme = theme.clone();
    let label_theme = theme.clone();
    let message_theme = theme.clone();
    let mut container = BoxComponent::new(
        1,
        1,
        Some(Rc::new(move |text| background_theme.bg(ThemeBg::CustomMessageBg, text))),
    );
    container.add_child(Rc::new(RefCell::new(Text::new(
        label_theme.fg(ThemeColor::CustomMessageLabel, &label_theme.bold("[Supervisor]")),
        0,
        0,
    ))) as ComponentRef);
    container.add_child(Rc::new(RefCell::new(Spacer::new(1))) as ComponentRef);
    container.add_child(Rc::new(RefCell::new(Text::new(
        message_theme.fg(ThemeColor::CustomMessageText, &supervisor_instruction_body(&content)),
        0,
        0,
    ))) as ComponentRef);
    Some(Rc::new(RefCell::new(container)) as ComponentRef)
}

pub fn supervisor_status_entry_renderer() -> EntryRenderer {
    std::sync::Arc::new(render_supervisor_status_entry)
}

pub fn supervisor_message_renderer() -> MessageRenderer {
    std::sync::Arc::new(render_supervisor_message)
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_and_unwraps_supervisor_instructions() {
        let content = supervisor_instruction_content("do work");
        assert_eq!(content, "<supervisor-instruction>\ndo work\n</supervisor-instruction>");
        assert_eq!(supervisor_instruction_body(&content), "do work");
        assert_eq!(supervisor_instruction_body("plain"), "plain");
    }

    #[test]
    fn formats_review_countdown() {
        assert_eq!(format_review_countdown(61_000, 0), "Next review in 1:01");
        assert_eq!(format_review_countdown(60_000, 0), "Next review in 1:00");
        assert_eq!(format_review_countdown(5_000, 5_000), "Review due…");
    }
}
