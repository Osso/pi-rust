//! Bundled `loop` extension (`packages/coding-agent/extensions/loop/src/index.ts`).

use std::sync::{Arc, Mutex};

use pi_agent_core::AgentToolResult;
use pi_js_compat::js_json::{JsObject as Map, JsString, JsValue as Value};
use pi_js_compat::number::js_number_to_string;
use pi_js_compat::string::js_trim;
use regex::Regex;
use serde_json::json;

use crate::core::extensions::types::{
    AgentEndEvent, BoxFuture, CustomMessageContent, CustomMessagePayload, ExtensionApi, ExtensionCommandContext,
    ExtensionContext, HandlerError, MessageDelivery, NotifyType, SendMessageOptions, ToolDefinition,
};

const MINIMUM_INTERVAL_MS: f64 = 1_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopAction {
    Start,
    Stop,
    Status,
}

impl LoopAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Status => "status",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoopToolDetails {
    pub action: LoopAction,
    pub active: bool,
    pub interval_ms: Option<f64>,
    pub prompt: Option<JsString>,
}

impl LoopToolDetails {
    pub fn to_value(&self) -> Value {
        let mut object = Map::new();
        object.insert("action".into(), Value::String(self.action.as_str().into()));
        object.insert("active".into(), Value::Bool(self.active));
        if let Some(interval_ms) = self.interval_ms {
            object.insert(
                "intervalMs".into(),
                pi_js_compat::number::js_number_value(interval_ms).into(),
            );
        }
        if let Some(prompt) = &self.prompt {
            object.insert("prompt".into(), Value::String(prompt.clone()));
        }
        Value::Object(object)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActiveLoop {
    pub interval_ms: f64,
    pub prompt: JsString,
    pub deferred: bool,
    pub generation: u64,
    pub in_flight: bool,
    pub session_id: JsString,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParsedLoopCommand {
    Start { interval_ms: f64, prompt: JsString },
    Stop,
    Status,
    Error(String),
}

struct LoopState {
    active: Option<ActiveLoop>,
    task: Option<tokio::task::JoinHandle<()>>,
    next_generation: u64,
    closed: bool,
}

/// Controller shared by the slash command, the tool, and shutdown.
#[derive(Clone)]
pub struct LoopController {
    state: Arc<Mutex<LoopState>>,
    api: ExtensionApi,
}

impl LoopController {
    pub fn new(api: ExtensionApi) -> Self {
        Self {
            state: Arc::new(Mutex::new(LoopState {
                active: None,
                task: None,
                next_generation: 0,
                closed: false,
            })),
            api,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, LoopState> {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn stop(&self) -> bool {
        let mut state = self.lock();
        if state.active.is_none() {
            return false;
        }
        if let Some(task) = state.task.take() {
            task.abort();
        }
        state.active = None;
        true
    }

    pub fn status(&self) -> Option<ActiveLoop> {
        self.lock().active.clone()
    }

    pub fn shutdown(&self) {
        self.lock().closed = true;
        self.stop();
    }

    pub fn start(
        &self,
        interval_ms: f64,
        prompt: JsString,
        ctx: &ExtensionContext,
    ) -> Result<ActiveLoop, HandlerError> {
        if self.lock().closed {
            return Err(HandlerError::new("Cannot start a loop after session shutdown"));
        }
        self.stop();
        let generation = {
            let mut state = self.lock();
            state.next_generation += 1;
            state.next_generation
        };
        let session_id = crate::core::session_manager::lock_session_manager(&ctx.session_manager()?)
            .get_session_id()
            .clone();
        let active = ActiveLoop {
            interval_ms,
            prompt,
            deferred: false,
            generation,
            in_flight: false,
            session_id,
        };
        let task = self.spawn_interval(interval_ms, generation, ctx.clone());
        let mut state = self.lock();
        state.active = Some(active.clone());
        state.task = Some(task);
        Ok(active)
    }

    fn spawn_interval(&self, interval_ms: f64, generation: u64, ctx: ExtensionContext) -> tokio::task::JoinHandle<()> {
        let controller = self.clone();
        let period = std::time::Duration::from_millis(interval_ms as u64);
        let first_tick = tokio::time::Instant::now() + period;
        tokio::spawn(async move {
            let mut timer = tokio::time::interval_at(first_tick, period);
            timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                timer.tick().await;
                if let Err(error) = controller.handle_interval(generation, &ctx) {
                    crate::utils::uncaught::throw_uncaught(crate::utils::uncaught::ERROR, error.message);
                }
            }
        })
    }

    fn handle_interval(&self, generation: u64, ctx: &ExtensionContext) -> Result<(), HandlerError> {
        let mut state = self.lock();
        let Some(active) = state.active.as_mut().filter(|active| active.generation == generation) else {
            return Ok(());
        };
        if active.session_id
            != crate::core::session_manager::lock_session_manager(&ctx.session_manager()?)
                .get_session_id()
                .clone()
            || active.in_flight
        {
            return Ok(());
        }
        if !ctx.is_idle()? || ctx.has_pending_messages()? {
            active.deferred = true;
            return Ok(());
        }
        let message = prepare_loop_prompt(active);
        drop(state);
        self.send_loop_prompt(message)
    }

    pub fn handle_agent_end(&self, event: &AgentEndEvent, ctx: &ExtensionContext) -> Result<(), HandlerError> {
        let mut state = self.lock();
        let Some(active) = state.active.as_mut() else {
            return Ok(());
        };
        if active.session_id
            != crate::core::session_manager::lock_session_manager(&ctx.session_manager()?)
                .get_session_id()
                .clone()
        {
            return Ok(());
        }
        if event
            .messages
            .iter()
            .any(|message| includes_loop_prompt(message, active))
        {
            active.in_flight = false;
        }
        if !active.deferred || active.in_flight || ctx.has_pending_messages()? {
            return Ok(());
        }
        let message = prepare_loop_prompt(active);
        drop(state);
        self.send_loop_prompt(message)
    }

    fn send_loop_prompt(&self, message: CustomMessagePayload) -> Result<(), HandlerError> {
        self.api.send_message(
            message,
            Some(SendMessageOptions {
                deliver_as: Some(MessageDelivery::FollowUp),
                trigger_turn: Some(true),
            }),
        )
    }
}

fn includes_loop_prompt(message: &pi_agent_core::agent::AgentMessage, active: &ActiveLoop) -> bool {
    let pi_agent_core::agent::AgentMessage::Custom(message) = message else {
        return false;
    };
    if !message
        .get("role")
        .and_then(Value::as_string)
        .is_some_and(|role| role == "custom")
        || !message
            .get("customType")
            .and_then(Value::as_string)
            .is_some_and(|kind| kind == "loop")
    {
        return false;
    }
    let Some(details) = message.get("details").and_then(Value::as_object) else {
        return false;
    };
    details.get("generation").and_then(Value::as_f64) == Some(active.generation as f64)
        && details.get("sessionId").and_then(Value::as_string) == Some(&active.session_id)
}

fn prepare_loop_prompt(active: &mut ActiveLoop) -> CustomMessagePayload {
    active.deferred = false;
    active.in_flight = true;
    CustomMessagePayload {
        custom_type: "loop".into(),
        content: CustomMessageContent::Text(active.prompt.clone()),
        display: true,
        details: Some(pi_js_compat::js_json!({"generation": active.generation, "sessionId": active.session_id})),
    }
}

#[cfg(test)]
mod lossless_tests {
    use super::*;
    #[test]
    fn recurring_prompt_is_not_normalized_to_scalar_text() {
        let raw = JsString::from_utf16(vec![0xd800, 0x61, 0xdc00]);
        let ParsedLoopCommand::Start { prompt, interval_ms } = parse_loop_command(JsString::from("1s ") + &raw) else {
            panic!("start loop")
        };
        assert_eq!(prompt, raw);
        assert_eq!(interval_ms, 1000.0);
        let details = LoopToolDetails {
            action: LoopAction::Start,
            active: true,
            interval_ms: Some(interval_ms),
            prompt: Some(prompt),
        }
        .to_value();
        assert_eq!(details["prompt"].as_string(), Some(&raw));
    }
}

pub fn parse_interval_ms(value: &str) -> Option<f64> {
    let pattern = Regex::new(r"^(\d+(?:\.\d+)?)(ms|s|m|h)?$").expect("static interval pattern");
    let trimmed = js_trim(value);
    let captures = pattern.captures(trimmed)?;
    let amount = pi_js_compat::number::parse_number(&captures[1]);
    if !amount.is_finite() || amount <= 0.0 {
        return None;
    }
    let unit = captures.get(2).map_or("s", |capture| capture.as_str());
    let multiplier = match unit {
        "ms" => 1.0,
        "s" => 1_000.0,
        "m" => 60_000.0,
        _ => 3_600_000.0,
    };
    let interval_ms = (amount * multiplier).floor();
    (interval_ms >= MINIMUM_INTERVAL_MS).then_some(interval_ms)
}

pub fn format_interval(interval_ms: f64) -> String {
    if interval_ms % 3_600_000.0 == 0.0 {
        return format!("{}h", js_number_to_string(interval_ms / 3_600_000.0));
    }
    if interval_ms % 60_000.0 == 0.0 {
        return format!("{}m", js_number_to_string(interval_ms / 60_000.0));
    }
    if interval_ms % 1_000.0 == 0.0 {
        return format!("{}s", js_number_to_string(interval_ms / 1_000.0));
    }
    format!("{}ms", js_number_to_string(interval_ms))
}

pub fn parse_loop_command(args: impl Into<JsString>) -> ParsedLoopCommand {
    let trimmed = args.into().trim();
    if trimmed.is_empty() || trimmed == "status" {
        return ParsedLoopCommand::Status;
    }
    if trimmed == "stop" || trimmed == "off" || trimmed == "clear" {
        return ParsedLoopCommand::Stop;
    }

    let parts = trimmed
        .units()
        .split(|unit| pi_js_compat::string::is_js_whitespace_unit(*unit))
        .filter(|part| !part.is_empty())
        .map(|part| JsString::from_utf16(part.to_vec()))
        .collect::<Vec<_>>();
    let interval_ms = parts
        .first()
        .and_then(|part| part.to_utf8().ok())
        .and_then(|text| parse_interval_ms(&text));
    let Some(interval_ms) = interval_ms else {
        return ParsedLoopCommand::Error("Usage: /loop <interval> <prompt> | /loop status | /loop stop".to_owned());
    };
    let prompt = JsString::join(parts.iter().skip(1), " ").trim();
    if prompt.is_empty() {
        return ParsedLoopCommand::Error("Loop prompt is required.".to_owned());
    }
    ParsedLoopCommand::Start { interval_ms, prompt }
}

pub fn describe_loop(active: Option<&ActiveLoop>) -> JsString {
    match active {
        None => "No active loop".into(),
        Some(active) => {
            JsString::from(format!("Loop active every {}: ", format_interval(active.interval_ms))) + &active.prompt
        }
    }
}

fn text_result(text: impl Into<JsString>, details: LoopToolDetails) -> AgentToolResult {
    AgentToolResult {
        content: vec![pi_ai::types::TextOrImageContent::Text(pi_ai::types::TextContent::new(
            text,
        ))],
        details: Some(details.to_value().into()),
        is_error: None,
        terminate: None,
    }
}

fn loop_tool_definition(controller: LoopController) -> ToolDefinition {
    let parameters = json!({
        "type": "object",
        "required": ["action"],
        "properties": {
            "action": {
                "anyOf": [
                    {"type": "string", "const": "start"},
                    {"type": "string", "const": "stop"},
                    {"type": "string", "const": "status"}
                ]
            },
            "intervalSeconds": {
                "type": "number",
                "description": "Interval in seconds. Required for action=start."
            },
            "prompt": {
                "type": "string",
                "description": "Prompt to inject on every interval. Required for action=start."
            }
        }
    });
    ToolDefinition {
        name: "loop".to_owned(),
        label: "Loop".into(),
        description: "Start, stop, or inspect a recurring prompt injected into this Pi session.".into(),
        prompt_snippet: Some(
            "Schedule recurring user prompts at a fixed interval, or stop/status the active loop.".into(),
        ),
        prompt_guidelines: Some(vec![
            "Use loop only when the user wants recurring follow-up prompts injected into the current session.".into(),
            "Prefer action=status before changing an existing loop when unsure.".into(),
            "Use action=stop when the recurring prompts are no longer needed.".into(),
        ]),
        approval_required: Some(true),
        approval_kind: None,
        parameters,
        render_shell: None,
        prepare_arguments: None,
        prepare_execution: None,
        execution_mode: None,
        execute: Arc::new(
            move |_tool_call_id, params, _signal, _on_update, _ctx: Option<ExtensionContext>| {
                let controller = controller.clone();
                Box::pin(async move { execute_loop_tool(&controller, params, _ctx) })
            },
        ),
        render_call: None,
        render_result: None,
    }
}

fn execute_loop_tool(
    controller: &LoopController,
    params: Value,
    ctx: Option<ExtensionContext>,
) -> Result<AgentToolResult, HandlerError> {
    let action = params
        .get("action")
        .and_then(Value::as_string)
        .cloned()
        .unwrap_or_default();
    if action == "status" {
        let active = controller.status();
        let text = describe_loop(active.as_ref());
        return Ok(text_result(
            text,
            LoopToolDetails {
                action: LoopAction::Status,
                active: active.is_some(),
                interval_ms: active.as_ref().map(|active| active.interval_ms),
                prompt: active.map(|active| active.prompt),
            },
        ));
    }

    if action == "stop" {
        let stopped = controller.stop();
        return Ok(text_result(
            if stopped { "Loop stopped" } else { "No active loop" }.to_owned(),
            LoopToolDetails {
                action: LoopAction::Stop,
                active: false,
                interval_ms: None,
                prompt: None,
            },
        ));
    }

    let prompt = params
        .get("prompt")
        .and_then(Value::as_string)
        .map(JsString::trim)
        .filter(|prompt| !prompt.is_empty());
    let interval_ms = params
        .get("intervalSeconds")
        .and_then(Value::as_f64)
        .and_then(|seconds| parse_interval_ms(&format!("{}s", js_number_to_string(seconds))));

    let Some(interval_ms) = interval_ms else {
        return Ok(text_result(
            "intervalSeconds must be at least 1.".to_owned(),
            LoopToolDetails {
                action: LoopAction::Start,
                active: false,
                interval_ms: None,
                prompt: None,
            },
        ));
    };
    let Some(prompt) = prompt else {
        return Ok(text_result(
            "Prompt is required for action=start.".to_owned(),
            LoopToolDetails {
                action: LoopAction::Start,
                active: false,
                interval_ms: None,
                prompt: None,
            },
        ));
    };

    let ctx = crate::core::extensions::types::require_extension_context(ctx)?;
    let active = controller.start(interval_ms, prompt.to_owned(), &ctx)?;
    Ok(text_result(
        JsString::from(format!("Loop started every {}: ", format_interval(active.interval_ms))) + &prompt,
        LoopToolDetails {
            action: LoopAction::Start,
            active: true,
            interval_ms: Some(active.interval_ms),
            prompt: Some(prompt.to_owned()),
        },
    ))
}

/// Bundled `loop` extension.
pub fn loop_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let controller = LoopController::new(pi.clone());

    let shutdown_controller = controller.clone();
    pi.on_session_shutdown(
        move |_event, _ctx| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let controller = shutdown_controller.clone();
            Box::pin(async move {
                controller.shutdown();
                Ok(None)
            })
        },
    )
    .map_err(HandlerError::from)?;

    pi.on_agent_end({
        let controller = controller.clone();
        move |event: AgentEndEvent, ctx| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let controller = controller.clone();
            Box::pin(async move {
                if event.session_continuation.is_none() {
                    controller.handle_agent_end(&event, &ctx)?;
                }
                Ok(None)
            })
        }
    })
    .map_err(HandlerError::new)?;

    pi.register_tool(loop_tool_definition(controller.clone()))
        .map_err(HandlerError::from)?;

    pi.register_command(
        "loop",
        Some("Inject a recurring prompt (/loop <interval> <prompt> | /loop status | /loop stop).".into()),
        None,
        Arc::new(move |args, ctx: ExtensionCommandContext| {
            let controller = controller.clone();
            Box::pin(async move { handle_loop_command(&args, &ctx, &controller).await })
        }),
    )
    .map_err(HandlerError::from)?;
    Ok(())
}

async fn handle_loop_command(
    args: &JsString,
    ctx: &ExtensionCommandContext,
    controller: &LoopController,
) -> Result<(), HandlerError> {
    let ui = ctx.ui()?;
    match parse_loop_command(args) {
        ParsedLoopCommand::Error(error) => {
            ui.notify(&error.into(), Some(NotifyType::Error))?;
        }
        ParsedLoopCommand::Status => {
            ui.notify(&describe_loop(controller.status().as_ref()), Some(NotifyType::Info))?;
        }
        ParsedLoopCommand::Stop => {
            let message = if controller.stop() {
                "Loop stopped"
            } else {
                "No active loop"
            };
            ui.notify(&message.into(), Some(NotifyType::Info))?;
        }
        ParsedLoopCommand::Start { interval_ms, prompt } => {
            let active = controller.start(interval_ms, prompt, ctx)?;
            ui.notify(
                &format!("Loop started every {}", format_interval(active.interval_ms)).into(),
                Some(NotifyType::Info),
            )?;
            ui.set_editor_text(&JsString::default())?;
        }
    }
    Ok(())
}
