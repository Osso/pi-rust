//! Port of `packages/coding-agent/src/modes/print-mode.ts`.
//!
//! Single-shot mode: send prompts, write the final result to stdout, exit.

use pi_ai::request::CallbackError;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use pi_agent_core::agent::types::{AgentEvent, AgentMessage};
use pi_ai::types::{AssistantContent, AssistantMessage, ImageContent, StopReason};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use tokio::task::JoinHandle;

use crate::core::agent_session::{AgentSession, AgentSessionEvent, ExtensionBindings, PromptOptions};
use crate::core::agent_session_runtime::AgentSessionRuntime;
use crate::core::assistant_narration::get_assistant_content_text;
use crate::core::extensions::types::{ExtensionError, ExtensionMode};
use crate::core::output_guard::{flush_raw_stdout, wait_for_raw_stdout_backpressure, write_raw_stdout};
use crate::core::session_control_db::get_control_db_path;
use crate::modes::extension_actions::command_context_actions;
use crate::modes::json_event::to_json_print_mode_event;
use crate::utils::shell::kill_tracked_detached_children;

/// Output mode: `text` prints the final response only, `json` prints all events.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PrintMode {
    /// `pi -p "prompt"`.
    #[default]
    Text,
    /// `pi --mode json "prompt"`.
    Json,
}

/// Options for [`run_print_mode`].
#[derive(Debug, Clone, Default)]
pub struct PrintModeOptions {
    /// Output mode.
    pub mode: PrintMode,
    /// Additional prompts sent after `initial_message`.
    pub messages: Vec<JsString>,
    /// First message to send (may contain `@file` content).
    pub initial_message: Option<JsString>,
    /// Images attached to the initial message.
    pub initial_images: Vec<ImageContent>,
}

#[derive(Default)]
struct Subscriptions {
    session: Vec<u64>,
    agent: Vec<u64>,
}

struct PrintState {
    mode: PrintMode,
    session: Mutex<AgentSession>,
    runtime: Arc<tokio::sync::Mutex<AgentSessionRuntime>>,
    subscriptions: Mutex<Subscriptions>,
    disposed: AtomicBool,
}

impl PrintState {
    fn current_session(&self) -> AgentSession {
        lock(&self.session).clone()
    }

    fn clear_subscriptions(&self) {
        let session = self.current_session();
        let subscriptions = std::mem::take(&mut *lock(&self.subscriptions));
        for id in subscriptions.session {
            session.unsubscribe(id);
        }
        for id in subscriptions.agent {
            session.agent().unsubscribe(id);
        }
    }

    async fn dispose(&self) {
        if self.disposed.swap(true, Ordering::SeqCst) {
            return;
        }
        self.clear_subscriptions();
        self.runtime.lock().await.dispose().await;
    }
}

/// Runs print mode and returns the process exit code.
pub async fn run_print_mode(runtime: Arc<tokio::sync::Mutex<AgentSessionRuntime>>, options: PrintModeOptions) -> i32 {
    let session = runtime.lock().await.session().clone();
    let state = Arc::new(PrintState {
        mode: options.mode,
        session: Mutex::new(session),
        runtime,
        subscriptions: Mutex::new(Subscriptions::default()),
        disposed: AtomicBool::new(false),
    });
    let signal_tasks = register_signal_handlers(Arc::clone(&state));
    set_rebind_session(&state).await;

    if state.mode == PrintMode::Json {
        let header = state.current_session().session_manager().get_header().cloned();
        if let Some(header) = header {
            write_json_line(&header);
        }
    }

    let exit_code = match run_prompts(&state, &options).await {
        Ok(()) if state.mode == PrintMode::Text => write_text_result(&state),
        Ok(()) => 0,
        Err(message) => {
            eprintln!("{}", encode_node_utf8(&message.message));
            1
        }
    };

    for task in signal_tasks {
        task.abort();
    }
    state.dispose().await;
    flush_raw_stdout().await;
    exit_code
}

async fn run_prompts(state: &Arc<PrintState>, options: &PrintModeOptions) -> Result<(), CallbackError> {
    rebind_session(state).await?;
    if let Some(message) = &options.initial_message {
        state
            .current_session()
            .prompt(
                message,
                PromptOptions {
                    images: options.initial_images.clone(),
                    ..PromptOptions::default()
                },
            )
            .await
            .map_err(CallbackError::from)?;
    }
    for message in &options.messages {
        state
            .current_session()
            .prompt(message, PromptOptions::default())
            .await
            .map_err(CallbackError::from)?;
    }
    Ok(())
}

fn write_text_result(state: &PrintState) -> i32 {
    let session = state.current_session();
    let assistant_message = find_last_printable_assistant_message(&session.state().messages);

    match assistant_message {
        Some(message) if matches!(message.stop_reason, StopReason::Error | StopReason::Aborted) => {
            eprintln!("{}", encode_node_utf8(&error_message_or_request(&message)));
            1
        }
        Some(message) => {
            for content in &message.content {
                if let Some(text) = get_assistant_content_text(content) {
                    write_raw_stdout(&format!("{}\n", encode_node_utf8(&text)));
                }
            }
            0
        }
        None => 0,
    }
}

fn error_message_or_request(message: &AssistantMessage) -> JsString {
    message
        .error_message
        .clone()
        .filter(|error| !error.is_empty())
        .unwrap_or_else(|| JsString::from("Request ") + stop_reason_name(message.stop_reason))
}

fn stop_reason_name(reason: StopReason) -> &'static str {
    match reason {
        StopReason::Stop => "stop",
        StopReason::Length => "length",
        StopReason::ToolUse => "toolUse",
        StopReason::Error => "error",
        StopReason::Aborted => "aborted",
    }
}

fn is_printable_assistant_message(message: &AssistantMessage) -> bool {
    matches!(message.stop_reason, StopReason::Error | StopReason::Aborted)
        || message
            .content
            .iter()
            .filter_map(get_assistant_content_text)
            .any(|text| !text.is_empty())
}

fn find_last_printable_assistant_message(messages: &[AgentMessage]) -> Option<AssistantMessage> {
    for message in messages.iter().rev() {
        match message {
            AgentMessage::ToolResult(result) if result.tool_name == "end_turn" && !result.is_error => continue,
            AgentMessage::Assistant(assistant) => {
                if is_printable_assistant_message(assistant) {
                    return Some(assistant.as_ref().clone());
                }
                if assistant
                    .content
                    .iter()
                    .any(|content| matches!(content, AssistantContent::ToolCall(call) if call.name == "end_turn"))
                {
                    continue;
                }
                return None;
            }
            _ => return None,
        }
    }
    None
}

fn write_json_line<T: serde::Serialize>(value: &T) {
    let line = crate::modes::rpc::serialize_json_line(value).expect("print mode values must serialize");
    write_raw_stdout(&line);
}

async fn set_rebind_session(state: &Arc<PrintState>) {
    let handler_state = Arc::downgrade(state);
    let handler: crate::core::agent_session_runtime::RebindSessionHandler = Arc::new(move |session| {
        let handler_state = handler_state.clone();
        Box::pin(async move {
            let Some(state) = handler_state.upgrade() else {
                return Ok(());
            };
            *lock(&state.session) = session;
            rebind_session(&state).await
        })
    });
    state.runtime.lock().await.set_rebind_session(Some(handler));
}

async fn rebind_session(state: &Arc<PrintState>) -> Result<(), CallbackError> {
    let session = state.current_session();
    let mode = if state.mode == PrintMode::Json {
        ExtensionMode::Json
    } else {
        ExtensionMode::Print
    };
    session
        .bind_extensions(ExtensionBindings {
            mode: Some(mode),
            control_db_path: Some(
                get_control_db_path(&crate::config::get_user_state_root().map_err(CallbackError::from)?).into(),
            ),
            command_context_actions: Some(command_context_actions(
                Arc::new(Mutex::new(session.clone())),
                Arc::clone(&state.runtime),
            )),
            on_error: Some(Arc::new(|error: &ExtensionError| {
                let message = js_format!("Extension error ({}): {}", error.extension_path, error.error);
                eprintln!("{}", encode_node_utf8(&message));
            })),
            ..ExtensionBindings::default()
        })
        .await
        .map_err(CallbackError::from)?;

    state.clear_subscriptions();
    let json_mode = state.mode == PrintMode::Json;
    let id = session.subscribe(Arc::new(move |event: &AgentSessionEvent| {
        if json_mode {
            write_json_line(&to_json_print_mode_event(event));
        }
    }));
    lock(&state.subscriptions).session.push(id);

    if json_mode {
        let id = session
            .agent()
            .subscribe(Arc::new(move |_event: AgentEvent, _signal: AbortSignal| {
                Box::pin(async move {
                    wait_for_raw_stdout_backpressure().await;
                    Ok(())
                })
            }));
        lock(&state.subscriptions).agent.push(id);
    }
    Ok(())
}

fn lock<T>(value: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    value.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(unix)]
fn register_signal_handlers(state: Arc<PrintState>) -> Vec<JoinHandle<()>> {
    vec![
        spawn_signal_handler(tokio::signal::unix::SignalKind::terminate(), 143, Arc::clone(&state)),
        spawn_signal_handler(tokio::signal::unix::SignalKind::hangup(), 129, state),
    ]
}

#[cfg(not(unix))]
fn register_signal_handlers(_state: Arc<PrintState>) -> Vec<JoinHandle<()>> {
    Vec::new()
}

#[cfg(unix)]
fn spawn_signal_handler(
    kind: tokio::signal::unix::SignalKind,
    exit_code: i32,
    state: Arc<PrintState>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let Ok(mut signals) = tokio::signal::unix::signal(kind) else {
            return;
        };
        if signals.recv().await.is_none() {
            return;
        }
        kill_tracked_detached_children();
        state.dispose().await;
        std::process::exit(exit_code);
    })
}
