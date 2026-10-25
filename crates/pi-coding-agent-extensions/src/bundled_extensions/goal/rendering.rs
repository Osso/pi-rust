//! Port of `packages/coding-agent/extensions/goal/src/rendering.ts`.

use std::cell::RefCell;
use std::rc::Rc;

use pi_js_compat::js_json::{JsObject, JsString, JsValue as Value};
use pi_tui::components::r#box::BoxComponent;
use pi_tui::components::spacer::Spacer;
use pi_tui::components::text::{StyleFn, Text};
use pi_tui::tui::{Component, ComponentRef};

use crate::core::extensions::types::{
    CustomMessageContent, CustomMessagePayload, CustomMessageValue, EntryRenderOptions, EntryRenderer, ExtensionApi,
    ExtensionContext, HandlerError, MessageRenderOptions, MessageRenderer, SendMessageOptions,
};
use crate::core::session_manager::CustomEntry;
use crate::modes::interactive::theme::{Theme, ThemeBg, ThemeColor};

use super::wait_countdown::{WaitCountdownRefresher, parse_review_deadline};

const SUPERVISOR_INSTRUCTION_OPEN: &str = "<supervisor-instruction>";
const SUPERVISOR_INSTRUCTION_CLOSE: &str = "</supervisor-instruction>";
pub const SUPERVISOR_STATUS_TYPE: &str = "supervisor-status";
const REVIEW_DUE_TEXT: &str = "Review due…";

struct SupervisorStatusStyles {
    background: StyleFn,
    countdown: StyleFn,
    label: StyleFn,
    message: StyleFn,
}

struct SupervisorStatusComponent {
    message: JsString,
    deadline_ms: Option<i64>,
    styles: SupervisorStatusStyles,
}

impl Component for SupervisorStatusComponent {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        let mut container = BoxComponent::new(1, 1, Some(Rc::clone(&self.styles.background)));
        container.add_child(Rc::new(RefCell::new(Text::new(
            (self.styles.label)(&"[Supervisor]".into()),
            0,
            0,
        ))) as ComponentRef);
        container.add_child(Rc::new(RefCell::new(Spacer::new(1))) as ComponentRef);
        container
            .add_child(Rc::new(RefCell::new(Text::new((self.styles.message)(&self.message), 0, 0))) as ComponentRef);
        if let Some(deadline_ms) = self.deadline_ms {
            let countdown = format_review_countdown(deadline_ms, now_millis());
            container.add_child(Rc::new(RefCell::new(Text::new(
                (self.styles.countdown)(&countdown.into()),
                0,
                0,
            ))) as ComponentRef);
        }
        container.render(width)
    }
}

pub struct SupervisorStatusController {
    pi: ExtensionApi,
    refresher: WaitCountdownRefresher,
}

pub type AppendSupervisorStatus =
    std::sync::Arc<dyn Fn(ExtensionContext, &JsString, Option<&str>) -> Result<(), HandlerError> + Send + Sync>;

impl SupervisorStatusController {
    /// `append`.
    pub fn append(
        &self,
        ctx: ExtensionContext,
        message: impl Into<JsString>,
        review_at: Option<&str>,
    ) -> Result<(), HandlerError> {
        let session = ctx.session_manager()?;
        let session_id = session.get_session_id().clone();
        self.refresher.clear_session(&session_id);
        let data = match review_at {
            Some(review_at) => {
                let mut object = JsObject::new();
                object.insert("message".into(), Value::String(message.into()));
                object.insert("reviewAt".into(), Value::String(review_at.into()));
                Some(Value::Object(object))
            }
            None => {
                let mut object = JsObject::new();
                object.insert("message".into(), Value::String(message.into()));
                Some(Value::Object(object))
            }
        };
        self.pi
            .append_entry(&SUPERVISOR_STATUS_TYPE.into(), data)
            .map_err(|error| HandlerError::new(error.to_string()))?;
        if let Some(review_at) = review_at {
            self.refresher.start(ctx, &review_at.into());
        }
        Ok(())
    }

    pub fn clear_all(&self) {
        self.refresher.clear_all();
    }

    pub fn clear_session(&self, session_id: &JsString) {
        self.refresher.clear_session(session_id);
    }

    /// `restore`.
    pub fn restore(&self, ctx: ExtensionContext) -> Result<(), HandlerError> {
        let session = ctx.session_manager()?;
        let session_id = session.get_session_id().clone();
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
pub fn supervisor_instruction_content(instructions: impl Into<JsString>) -> JsString {
    JsString::from(SUPERVISOR_INSTRUCTION_OPEN) + "\n" + instructions.into() + "\n" + SUPERVISOR_INSTRUCTION_CLOSE
}

fn supervisor_message(content: &JsString) -> CustomMessagePayload {
    CustomMessagePayload {
        custom_type: "supervisor".into(),
        content: CustomMessageContent::Text(supervisor_instruction_content(content).into()),
        display: true,
        details: None,
    }
}

/// `sendSupervisorInstructions`.
pub fn send_supervisor_instructions(pi: &ExtensionApi, instructions: &JsString) -> Result<(), HandlerError> {
    pi.send_message(
        supervisor_message(instructions),
        Some(SendMessageOptions {
            trigger_turn: Some(true),
            deliver_as: Some(crate::core::extensions::types::MessageDelivery::FollowUp),
        }),
    )
    .map_err(|error| HandlerError::new(error.to_string()))
}

fn has_supervisor_instruction_wrapper(content: &JsString) -> bool {
    content.starts_with(SUPERVISOR_INSTRUCTION_OPEN) && content.ends_with(SUPERVISOR_INSTRUCTION_CLOSE)
}

fn supervisor_instruction_body(content: &JsString) -> JsString {
    if !has_supervisor_instruction_wrapper(content) {
        return content.to_owned();
    }
    let mut body = content.slice(
        SUPERVISOR_INSTRUCTION_OPEN.len(),
        content.len() - SUPERVISOR_INSTRUCTION_CLOSE.len(),
    );
    if body.starts_with("\n") {
        body = body.slice(1, body.len());
    }
    if body.ends_with("\n") {
        body = body.slice(0, body.len() - 1);
    }
    body
}

fn status_details(data: Option<&Value>) -> (JsString, Option<JsString>) {
    let Some(Value::Object(details)) = data else {
        return ("Supervisor status unavailable".into(), None);
    };
    let message = details
        .get("message")
        .and_then(Value::as_string)
        .cloned()
        .unwrap_or_else(|| "Supervisor status unavailable".into());
    let review_at = details.get("reviewAt").and_then(Value::as_string).cloned();
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

fn latest_future_review_at(ctx: &ExtensionContext) -> Result<Option<JsString>, HandlerError> {
    let session = ctx.session_manager()?;
    let entries = session.get_entries();
    for entry in entries.iter().rev() {
        let known = entry.known();
        let Some(crate::core::session_manager::KnownSessionEntry::Custom(entry)) = known.as_deref() else {
            continue;
        };
        if entry.custom_type != SUPERVISOR_STATUS_TYPE {
            continue;
        }
        let (_, review_at) = status_details(entry.data.as_ref());
        let Some(review_at) = review_at else {
            return Ok(None);
        };
        let deadline_ms = parse_review_deadline(&review_at);
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
    let deadline_ms = review_at.as_ref().and_then(parse_review_deadline);
    let background_theme = theme.clone();
    let countdown_theme = theme.clone();
    let label_theme = theme.clone();
    let message_theme = theme.clone();
    Some(Rc::new(RefCell::new(SupervisorStatusComponent {
        message,
        deadline_ms,
        styles: SupervisorStatusStyles {
            background: Rc::new(move |text: &JsString| background_theme.bg(ThemeBg::CustomMessageBg, text.clone())),
            countdown: Rc::new(move |text: &JsString| countdown_theme.fg(ThemeColor::Dim, text.clone())),
            label: Rc::new(move |text: &JsString| {
                label_theme.fg(ThemeColor::CustomMessageLabel, label_theme.bold(text.clone()))
            }),
            message: Rc::new(move |text: &JsString| message_theme.fg(ThemeColor::CustomMessageText, text.clone())),
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
        CustomMessageContent::Blocks(_) => JsString::default(),
    };
    let background_theme = theme.clone();
    let label_theme = theme.clone();
    let message_theme = theme.clone();
    let mut container = BoxComponent::new(
        1,
        1,
        Some(Rc::new(move |text: &JsString| {
            background_theme.bg(ThemeBg::CustomMessageBg, text.clone())
        })),
    );
    container.add_child(Rc::new(RefCell::new(Text::new(
        label_theme.fg(ThemeColor::CustomMessageLabel, label_theme.bold("[Supervisor]")),
        0,
        0,
    ))) as ComponentRef);
    container.add_child(Rc::new(RefCell::new(Spacer::new(1))) as ComponentRef);
    container.add_child(Rc::new(RefCell::new(Text::new(
        message_theme.fg(ThemeColor::CustomMessageText, supervisor_instruction_body(&content)),
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
        assert_eq!(supervisor_instruction_body(&"plain".into()), "plain");
    }

    #[test]
    fn formats_review_countdown() {
        assert_eq!(format_review_countdown(61_000, 0), "Next review in 1:01");
        assert_eq!(format_review_countdown(60_000, 0), "Next review in 1:00");
        assert_eq!(format_review_countdown(5_000, 5_000), "Review due…");
    }
}
