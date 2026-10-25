//! Port of `packages/coding-agent/src/modes/rpc/rpc-mode.ts`.
//!
//! Reads JSON commands on stdin and writes responses, extension UI requests, and
//! session events on stdout. The TypeScript mode never resolves; shutdown exits
//! the process exactly like `process.exit`.

mod input;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use pi_agent_core::agent::types::AgentEvent;
use pi_ai::request::{BoxFuture, CallbackError};
use pi_ai::types::InputSource;
use pi_ai::utils::random_uuid::random_uuid;
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json::{JsString, JsValue, encode_node_utf8};
use serde::Serialize;
use tokio::sync::oneshot;
use tokio::task::{JoinHandle, LocalSet};

use crate::config::get_user_state_root;
use crate::core::agent_session::{
    AgentSession, AgentSessionEvent, CycleDirection, ExecuteBashOptions, ExtensionBindings, PromptOptions,
    StreamingBehavior,
};
use crate::core::agent_session_runtime::{
    AgentSessionRuntime, ForkPosition, NewSessionOptions as RuntimeNewSessionOptions,
};
use crate::core::compaction::CompactionResult;
use crate::core::extensions::types::{
    ExtensionError, ExtensionMode, ExtensionUiContext, ExtensionUiDialogOptions, ExtensionWidgetOptions, NotifyType,
    ThemeSetResult, WorkingIndicatorOptions,
};
use crate::core::footer_data_provider::ExtensionStatus;
use crate::core::output_guard::{
    flush_raw_stdout, take_over_stdout, wait_for_raw_stdout_backpressure, write_raw_stdout,
};
use crate::core::session_control_db::get_control_db_path;
use crate::core::session_manager::{SessionEntry, SessionTreeNode};
use crate::core::slash_commands::{SlashCommandInfo, SlashCommandSource};
use crate::modes::extension_actions::command_context_actions;
use crate::modes::rpc::jsonl::{JsonlLineReader, attach_jsonl_line_reader, serialize_json_line};
use crate::modes::rpc::rpc_types::{
    ExtensionUiRequestTag, RpcExtensionUiRequest, RpcExtensionUiRequestPayload, RpcResponse, RpcSessionState,
    RpcSlashCommand,
};
use crate::utils::shell::kill_tracked_detached_children;
use pi_js_compat::js_json;

type PendingSender = oneshot::Sender<Option<JsValue>>;

/// Pending extension UI requests awaiting a client response line.
#[derive(Clone, Default)]
struct DialogBroker {
    pending: Arc<Mutex<HashMap<String, PendingSender>>>,
}

impl DialogBroker {
    fn register(&self, id: String, sender: PendingSender) {
        lock(&self.pending).insert(id, sender);
    }

    /// Routes a response line to its request; unknown or already-settled ids are ignored.
    fn resolve(&self, response: JsValue) -> bool {
        let Some(id) = response
            .get("id")
            .and_then(JsValue::as_string)
            .and_then(|id| id.to_utf8().ok())
        else {
            return false;
        };
        self.settle(&id, Some(response))
    }

    fn settle(&self, id: &str, response: Option<JsValue>) -> bool {
        let sender = lock(&self.pending).remove(id);
        match sender {
            Some(sender) => sender.send(response).is_ok(),
            None => false,
        }
    }
}

struct Subscriptions {
    owner: AgentSession,
    session_id: u64,
    agent_id: u64,
}

struct RpcState {
    session: Mutex<AgentSession>,
    runtime: AgentSessionRuntime,
    agent_dir: Option<JsString>,
    dialogs: DialogBroker,
    shutdown_requested: AtomicBool,
    shutting_down: AtomicBool,
    subscriptions: Mutex<Option<Subscriptions>>,
    input: Mutex<Option<JsonlLineReader>>,
}

impl RpcState {
    fn current_session(&self) -> AgentSession {
        lock(&self.session).clone()
    }

    fn clear_subscriptions(&self) {
        let subscriptions = lock(&self.subscriptions).take();
        if let Some(subscriptions) = subscriptions {
            subscriptions.owner.unsubscribe(subscriptions.session_id);
            subscriptions.owner.agent().unsubscribe(subscriptions.agent_id);
        }
    }

    fn output(&self, value: &JsValue) {
        write_json_line(value);
    }

    fn output_response(&self, response: &RpcResponse) {
        self.output(&js_json::to_value(response).expect("rpc responses must serialize"));
    }
}

/// Runs RPC mode until shutdown exits the process.
pub async fn run_rpc_mode(runtime: AgentSessionRuntime) -> Result<(), CallbackError> {
    take_over_stdout();
    let (session, agent_dir) = {
        let runtime = runtime.clone();
        (runtime.session().clone(), Some(runtime.services().agent_dir.clone()))
    };
    let state = Arc::new(RpcState {
        session: Mutex::new(session),
        agent_dir,
        runtime,
        dialogs: DialogBroker::default(),
        shutdown_requested: AtomicBool::new(false),
        shutting_down: AtomicBool::new(false),
        subscriptions: Mutex::new(None),
        input: Mutex::new(None),
    });

    set_rebind_session(&state).await;
    rebind_session(&state).await?;
    let signal_tasks = register_signal_handlers(Arc::clone(&state))?;

    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel::<String>();
    let reader = attach_jsonl_line_reader(tokio::io::stdin(), move |line| {
        let _ = sender.send(line);
        Ok(())
    });
    *lock(&state.input) = Some(reader);

    let local = LocalSet::new();
    local
        .run_until(async {
            resume_interrupted_session(&state);
            while let Some(line) = receiver.recv().await {
                let state = Arc::clone(&state);
                tokio::task::spawn_local(async move {
                    handle_input_line(&state, line).await;
                });
            }
            // Frozen detachInput removes onInputEnd. Detaching our reader also closes
            // this channel; only an attached input subscription can report real EOF.
            let input_listener_attached = lock(&state.input).is_some();
            if input_listener_attached {
                shutdown(&state, 0, None).await;
            }
            std::future::pending::<()>().await;
        })
        .await;

    for task in signal_tasks {
        task.abort();
    }
    Ok(())
}

/// Continues a restored session that was interrupted or asked for a continuation.
fn resume_interrupted_session(state: &Arc<RpcState>) {
    let session = state.current_session();
    let extension_requested = session.consume_resume_continuation_request();
    if !extension_requested && !crate::core::agent_session::should_continue_interrupted_session(&session.messages()) {
        return;
    }
    tokio::task::spawn_local(async move {
        if let Err(error) = session.continue_run().await {
            eprintln!(
                "Failed to continue restored session: {}",
                encode_node_utf8(error.message())
            );
        }
    });
}

async fn handle_input_line(state: &Arc<RpcState>, line: String) {
    let parsed = match js_json::parse_lossless(&line) {
        Ok(value) => value,
        Err(error) => {
            state.output_response(&RpcResponse::error(
                None,
                "parse",
                format!("Failed to parse command: {error}"),
            ));
            wait_for_raw_stdout_backpressure().await;
            return;
        }
    };

    if parsed.get("type").and_then(JsValue::as_string) == Some(&"extension_ui_response".into()) {
        route_extension_ui_response(state, parsed);
        return;
    }

    match input::handle_command(state, &parsed).await {
        Ok(Some(response)) => {
            state.output(&response);
            wait_for_raw_stdout_backpressure().await;
        }
        Ok(None) => {}
        Err(error) => {
            let message = input::caught_message(&error)
                .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
            let response = input::command_response(&parsed, false, Some(("error", JsValue::String(message))))
                .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
            state.output(&response);
            wait_for_raw_stdout_backpressure().await;
            return;
        }
    }
    check_shutdown_requested(state).await;
}

fn route_extension_ui_response(state: &RpcState, parsed: JsValue) {
    state.dialogs.resolve(parsed);
}

fn to_session_streaming_behavior(behavior: crate::core::extensions::types::StreamingBehavior) -> StreamingBehavior {
    match behavior {
        crate::core::extensions::types::StreamingBehavior::Steer => StreamingBehavior::Steer,
        crate::core::extensions::types::StreamingBehavior::FollowUp => StreamingBehavior::FollowUp,
    }
}

fn session_state(session: &AgentSession) -> RpcSessionState {
    RpcSessionState {
        model: Some(session.model()),
        thinking_level: session.thinking_level(),
        is_streaming: session.is_streaming(),
        is_compacting: session.is_compacting(),
        steering_mode: session.steering_mode(),
        follow_up_mode: session.follow_up_mode(),
        session_file: session.session_file(),
        session_id: session.session_id(),
        session_name: session.session_name().map(Into::into),
        auto_compaction_enabled: session.auto_compaction_enabled(),
        message_count: session.messages().len() as f64,
        pending_message_count: session.pending_message_count() as f64,
    }
}

fn collect_commands(session: &AgentSession) -> Vec<RpcSlashCommand> {
    let mut commands = Vec::new();
    for command in session.extension_runner().get_registered_commands() {
        commands.push(SlashCommandInfo {
            name: command.invocation_name,
            description: command.description,
            source: SlashCommandSource::Extension,
            source_info: command.source_info,
        });
    }
    for template in session.prompt_templates() {
        commands.push(SlashCommandInfo {
            name: template.name,
            description: Some(template.description),
            source: SlashCommandSource::Prompt,
            source_info: template.source_info,
        });
    }
    for skill in session.resource_loader().get_skills().skills {
        commands.push(SlashCommandInfo {
            name: JsString::from("skill:") + &skill.name,
            description: Some(skill.description),
            source: SlashCommandSource::Skill,
            source_info: skill.source_info,
        });
    }
    commands
}

fn start_prompt(
    state: &Arc<RpcState>,
    id: Option<JsValue>,
    message: JsString,
    images: Option<Vec<pi_ai::types::ImageContent>>,
    streaming_behavior: Option<StreamingBehavior>,
) {
    let session = state.current_session();
    let preflight_succeeded = Arc::new(AtomicBool::new(false));
    let preflight_state = Arc::downgrade(state);
    let preflight_flag = Arc::clone(&preflight_succeeded);
    let preflight_id = id.clone();
    let options = PromptOptions {
        images,
        streaming_behavior,
        source: Some(InputSource::Rpc),
        preflight_result: Some(Arc::new(move |did_succeed| {
            if !did_succeed {
                return;
            }
            preflight_flag.store(true, Ordering::SeqCst);
            if let Some(state) = preflight_state.upgrade() {
                state.output(&input::response(
                    preflight_id.clone(),
                    Some(JsValue::String("prompt".into())),
                    true,
                    None,
                ));
            }
        })),
        ..PromptOptions::default()
    };

    let error_state = Arc::downgrade(state);
    let error_flag = Arc::clone(&preflight_succeeded);
    let error_id = id;
    tokio::task::spawn_local(async move {
        if let Err(error) = session.prompt(&message, options).await
            && !error_flag.load(Ordering::SeqCst)
            && let Some(state) = error_state.upgrade()
        {
            let thrown = match CallbackError::from(error).thrown_value() {
                pi_js_compat::abort::AbortReason::Error(error)
                | pi_js_compat::abort::AbortReason::DomException(error) => error.value(),
                pi_js_compat::abort::AbortReason::Value(value) => value,
            };
            let message = input::property(&thrown, "message")
                .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error))
                .cloned();
            state.output(&input::response(
                error_id,
                Some(JsValue::String("prompt".into())),
                false,
                message.map(|value| ("error", value)),
            ));
        }
    });
}

async fn set_rebind_session(state: &Arc<RpcState>) {
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
    state.runtime.clone().set_rebind_session(Some(handler));
}

async fn rebind_session(state: &Arc<RpcState>) -> Result<(), CallbackError> {
    let session = state.current_session();
    let ui_state = Arc::downgrade(state);
    let shutdown_state = Arc::downgrade(state);
    let error_state = Arc::downgrade(state);
    session
        .bind_extensions(ExtensionBindings {
            ui_context: Some(Arc::new(RpcUiContext { state: ui_state })),
            mode: Some(ExtensionMode::Rpc),
            control_db_path: state
                .agent_dir
                .as_ref()
                .map(|_| get_user_state_root().map(|root| get_control_db_path(&root).into()))
                .transpose()
                .map_err(CallbackError::from)?,
            command_context_actions: Some(command_context_actions(
                Arc::new(Mutex::new(session.clone())),
                state.runtime.clone(),
            )),
            shutdown_handler: Some(Arc::new(move || {
                if let Some(state) = shutdown_state.upgrade() {
                    state.shutdown_requested.store(true, Ordering::SeqCst);
                }
            })),
            on_error: Some(Arc::new(move |error: &ExtensionError| {
                if let Some(state) = error_state.upgrade() {
                    state.output(&pi_js_compat::js_json!({
                        "type": "extension_error",
                        "extensionPath": error.extension_path,
                        "event": error.event,
                        "error": error.error,
                    }));
                }
            })),
            ..ExtensionBindings::default()
        })
        .await
        .map_err(CallbackError::from)?;

    state.clear_subscriptions();
    let listener_state = Arc::downgrade(state);
    let session_id = session.subscribe(Arc::new(move |event: &AgentSessionEvent| {
        let Some(state) = listener_state.upgrade() else {
            return;
        };
        state.output(&js_json::to_value(event).expect("session event must serialize"));
    }));

    let backpressure_state = Arc::downgrade(state);
    let agent_id = session
        .agent()
        .subscribe(Arc::new(move |_event: AgentEvent, _signal: AbortSignal| {
            let backpressure_state = backpressure_state.clone();
            Box::pin(async move {
                let _ = backpressure_state;
                wait_for_raw_stdout_backpressure().await;
                Ok(())
            })
        }));
    *lock(&state.subscriptions) = Some(Subscriptions {
        owner: session,
        session_id,
        agent_id,
    });
    Ok(())
}

async fn check_shutdown_requested(state: &Arc<RpcState>) {
    if !state.shutdown_requested.load(Ordering::SeqCst) {
        return;
    }
    shutdown(state, 0, None).await;
}

async fn shutdown(state: &Arc<RpcState>, exit_code: i32, signal: Option<&str>) {
    if state.shutting_down.swap(true, Ordering::SeqCst) {
        std::process::exit(exit_code);
    }
    state.clear_subscriptions();
    state
        .runtime
        .clone()
        .dispose()
        .await
        .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
    if let Some(reader) = lock(&state.input).take() {
        reader.detach();
    }
    if signal != Some("SIGTERM") {
        flush_raw_stdout().await;
    }
    std::process::exit(exit_code);
}

fn register_signal_handlers(state: Arc<RpcState>) -> Result<Vec<JoinHandle<()>>, CallbackError> {
    let terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let hangup = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())?;
    Ok(vec![
        spawn_signal_handler(terminate, "SIGTERM", 143, Arc::clone(&state)),
        spawn_signal_handler(hangup, "SIGHUP", 129, state),
    ])
}

#[cfg(unix)]
fn spawn_signal_handler(
    mut signals: tokio::signal::unix::Signal,
    signal: &'static str,
    exit_code: i32,
    state: Arc<RpcState>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        while signals.recv().await.is_some() {
            if !state.shutting_down.load(Ordering::SeqCst) {
                kill_tracked_detached_children();
            }
            let state = Arc::clone(&state);
            tokio::spawn(async move {
                shutdown(&state, exit_code, Some(signal)).await;
            });
        }
    })
}

fn write_json_line(value: &JsValue) {
    let line = serialize_json_line(value).expect("rpc values must serialize");
    write_raw_stdout(&line);
}

fn lock<T>(value: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    value.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ForkResponseData {
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<JsString>,
    cancelled: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ForkMessageData {
    entry_id: JsString,
    text: JsString,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EntriesResponseData {
    entries: Vec<SessionEntry>,
    /// `getLeafId()` returns `null` for an empty session.
    leaf_id: Option<JsString>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TreeResponseData {
    tree: Vec<SessionTreeNode>,
    /// `getLeafId()` returns `null` for an empty session.
    leaf_id: Option<JsString>,
}

#[derive(Serialize)]
struct LastAssistantTextData {
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<JsString>,
}

/// RPC-backed extension UI context.
struct RpcUiContext {
    state: Weak<RpcState>,
}

impl RpcUiContext {
    fn state(&self) -> Option<Arc<RpcState>> {
        self.state.upgrade()
    }
}

impl ExtensionUiContext for RpcUiContext {
    fn select<'a>(
        &'a self,
        title: &JsString,
        options: &[JsString],
        opts: Option<&ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<Option<JsString>, CallbackError>> {
        let state = self.state();
        let request = RpcExtensionUiRequestPayload::Select {
            title: title.clone(),
            options: options.to_vec(),
            timeout: opts.and_then(|opts| opts.timeout),
        };
        let opts = opts.cloned();
        Box::pin(async move {
            let Some(state) = state else {
                return Ok(None);
            };
            create_dialog_promise(
                &state.dialogs,
                |value| state.output(value),
                opts,
                Ok(None),
                request,
                |response| parse_dialog_value(response, "select"),
            )
            .await?
            .map_err(CallbackError::from)
        })
    }

    fn confirm<'a>(
        &'a self,
        title: &JsString,
        message: &JsString,
        opts: Option<&ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<bool, CallbackError>> {
        let state = self.state();
        let request = RpcExtensionUiRequestPayload::Confirm {
            title: title.clone(),
            message: message.clone(),
            timeout: opts.and_then(|opts| opts.timeout),
        };
        let opts = opts.cloned();
        Box::pin(async move {
            let Some(state) = state else {
                return Ok(false);
            };
            create_dialog_promise(
                &state.dialogs,
                |value| state.output(value),
                opts,
                Ok(false),
                request,
                parse_dialog_confirm,
            )
            .await?
            .map_err(CallbackError::from)
        })
    }

    fn input<'a>(
        &'a self,
        title: &JsString,
        placeholder: Option<&JsString>,
        opts: Option<&ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<Option<JsString>, CallbackError>> {
        let state = self.state();
        let request = RpcExtensionUiRequestPayload::Input {
            title: title.clone(),
            placeholder: placeholder.cloned(),
            timeout: opts.and_then(|opts| opts.timeout),
        };
        let opts = opts.cloned();
        Box::pin(async move {
            let Some(state) = state else {
                return Ok(None);
            };
            create_dialog_promise(
                &state.dialogs,
                |value| state.output(value),
                opts,
                Ok(None),
                request,
                |response| parse_dialog_value(response, "input"),
            )
            .await?
            .map_err(CallbackError::from)
        })
    }

    fn notify(
        &self,
        message: &JsString,
        notify_type: Option<NotifyType>,
    ) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        let Some(state) = self.state() else {
            return Ok(());
        };
        output_ui_request(
            &state,
            RpcExtensionUiRequestPayload::Notify {
                message: message.clone(),
                notify_type,
            },
        )
    }

    fn set_status(&self, key: &JsString, text: Option<ExtensionStatus>) -> Result<(), CallbackError> {
        let Some(state) = self.state() else {
            return Ok(());
        };
        let text = text.map(|status| status.evaluate(None)).transpose()?.flatten();
        output_ui_request(
            &state,
            RpcExtensionUiRequestPayload::SetStatus {
                status_key: key.clone(),
                status_text: text,
            },
        )
    }

    fn set_widget(
        &self,
        key: &JsString,
        content: Option<&[JsString]>,
        options: Option<&ExtensionWidgetOptions>,
    ) -> Result<(), CallbackError> {
        let Some(state) = self.state() else {
            return Ok(());
        };
        output_ui_request(
            &state,
            RpcExtensionUiRequestPayload::SetWidget {
                widget_key: key.clone(),
                widget_lines: content.map(<[JsString]>::to_vec),
                widget_placement: options.and_then(|options| options.placement),
            },
        )
    }

    fn set_title(&self, title: &JsString) -> Result<(), CallbackError> {
        let Some(state) = self.state() else {
            return Ok(());
        };
        output_ui_request(&state, RpcExtensionUiRequestPayload::SetTitle { title: title.clone() })
    }

    fn paste_to_editor(&self, text: &JsString) -> Result<(), CallbackError> {
        self.set_editor_text(text)
    }

    fn set_editor_text(&self, text: &JsString) -> Result<(), CallbackError> {
        let Some(state) = self.state() else {
            return Ok(());
        };
        output_ui_request(
            &state,
            RpcExtensionUiRequestPayload::SetEditorText { text: text.clone() },
        )
    }

    fn editor<'a>(
        &'a self,
        title: &JsString,
        prefill: Option<&JsString>,
    ) -> BoxFuture<'a, Result<Option<JsString>, CallbackError>> {
        let state = self.state();
        let request = RpcExtensionUiRequestPayload::Editor {
            title: title.clone(),
            prefill: prefill.cloned(),
        };
        Box::pin(async move {
            let Some(state) = state else {
                return Ok(None);
            };
            create_dialog_promise(
                &state.dialogs,
                |value| state.output(value),
                None,
                Ok(None),
                request,
                |response| parse_dialog_value(response, "editor"),
            )
            .await?
            .map_err(CallbackError::from)
        })
    }

    fn set_theme(&self, _theme: &JsString) -> ThemeSetResult {
        reject_rpc_theme_switch()
    }

    fn set_theme_object(&self, _theme: Arc<crate::modes::interactive::theme::Theme>) -> ThemeSetResult {
        reject_rpc_theme_switch()
    }

    fn set_working_message(&self, _message: Option<&JsString>) {}

    fn set_working_visible(&self, _visible: bool) {}

    fn set_working_indicator(&self, _options: Option<&WorkingIndicatorOptions>) {}

    fn set_hidden_thinking_label(&self, _label: Option<&JsString>) {}

    fn request_render(&self) {}
}

fn reject_rpc_theme_switch() -> ThemeSetResult {
    ThemeSetResult {
        success: false,
        error: Some("Theme switching not supported in RPC mode".into()),
    }
}

fn output_ui_request(state: &RpcState, payload: RpcExtensionUiRequestPayload) -> Result<(), CallbackError> {
    let request = RpcExtensionUiRequest {
        kind: ExtensionUiRequestTag::ExtensionUiRequest,
        id: random_uuid()?.into(),
        payload,
    };
    state.output(&js_json::to_value(&request).expect("extension UI request must serialize"));
    Ok(())
}

fn parse_dialog_value(response: &JsValue, method: &str) -> Result<Option<JsString>, JsString> {
    if response.get("cancelled").is_some_and(js_json::is_truthy) {
        return Ok(None);
    }
    match response.get("value") {
        None => Ok(None),
        Some(JsValue::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(format!(
            "RPC field {method}.extension_ui_response.value requires a string at the typed core boundary"
        )
        .into()),
    }
}

fn parse_dialog_confirm(response: &JsValue) -> Result<bool, JsString> {
    if response.get("cancelled").is_some_and(js_json::is_truthy) {
        return Ok(false);
    }
    match response.get("confirmed") {
        None => Ok(false),
        Some(JsValue::Bool(confirmed)) => Ok(*confirmed),
        Some(_) => Err(
            "RPC field confirm.extension_ui_response.confirmed requires a boolean at the typed core boundary".into(),
        ),
    }
}

async fn create_dialog_promise<T, P, F>(
    dialogs: &DialogBroker,
    emit: F,
    opts: Option<ExtensionUiDialogOptions>,
    default: T,
    request: RpcExtensionUiRequestPayload,
    parse: P,
) -> Result<T, CallbackError>
where
    P: FnOnce(&JsValue) -> T,
    F: Fn(&JsValue),
{
    let (signal, timeout) = match opts {
        Some(opts) => (opts.signal, opts.timeout),
        None => (None, None),
    };
    if signal.as_ref().is_some_and(AbortSignal::aborted) {
        return Ok(default);
    }

    let id = random_uuid()?;
    let (sender, receiver) = oneshot::channel();
    dialogs.register(id.clone(), sender);
    let abort_subscription = subscribe_dialog_abort(dialogs, &id, signal.as_ref());
    let timer = schedule_dialog_timeout(dialogs, &id, timeout);
    let request = RpcExtensionUiRequest {
        kind: ExtensionUiRequestTag::ExtensionUiRequest,
        id: id.clone().into(),
        payload: request,
    };
    emit(&js_json::to_value(&request).expect("extension UI request must serialize"));

    let response = receiver
        .await
        .expect("pending dialog retains its sender until settlement");
    drop(abort_subscription);
    if let Some(timer) = timer {
        timer.abort();
    }
    Ok(match response {
        Some(response) => parse(&response),
        None => default,
    })
}

fn subscribe_dialog_abort(
    dialogs: &DialogBroker,
    id: &str,
    signal: Option<&AbortSignal>,
) -> Option<pi_js_compat::abort::AbortSubscription> {
    let signal = signal?;
    let broker = dialogs.clone();
    let request_id = id.to_owned();
    match signal.subscribe(move |_| {
        broker.settle(&request_id, None);
    }) {
        Ok(subscription) => Some(subscription),
        Err(_) => {
            dialogs.settle(id, None);
            None
        }
    }
}

fn schedule_dialog_timeout(dialogs: &DialogBroker, id: &str, timeout: Option<f64>) -> Option<JoinHandle<()>> {
    let timeout = timeout.filter(|timeout| *timeout != 0.0 && !timeout.is_nan())?;
    let delay = Duration::from_millis(pi_js_compat::node_timers::normalize_timeout_delay(timeout) as u64);
    let broker = dialogs.clone();
    let request_id = id.to_owned();
    Some(tokio::spawn(async move {
        tokio::time::sleep(delay).await;
        broker.settle(&request_id, None);
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn select_request() -> RpcExtensionUiRequestPayload {
        RpcExtensionUiRequestPayload::Select {
            title: "Pick".into(),
            options: vec!["one".into(), "two".into()],
            timeout: None,
        }
    }

    fn value_response(id: &str, value: &str) -> JsValue {
        pi_js_compat::js_json!({"type":"extension_ui_response","id":id,"value":value})
    }

    #[tokio::test]
    async fn dialog_resolves_the_parsed_client_response() {
        let broker = DialogBroker::default();
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let emit = move |value: &JsValue| {
            let _ = sender.send(value.clone());
        };
        let promise = create_dialog_promise(&broker, emit, None, None, select_request(), |response| {
            response.get("value").and_then(JsValue::as_string).cloned()
        });
        let resolver = async {
            let request = receiver.recv().await.expect("request line");
            assert_eq!(request["type"], pi_js_compat::js_json!("extension_ui_request"));
            assert_eq!(request["method"], pi_js_compat::js_json!("select"));
            assert_eq!(request["title"], pi_js_compat::js_json!("Pick"));
            assert_eq!(request["options"], pi_js_compat::js_json!(["one", "two"]));
            let id = request["id"].as_string().expect("request id").to_utf8().unwrap();
            assert!(broker.resolve(value_response(&id, "two")));
        };
        let (result, ()) = tokio::join!(promise, resolver);
        assert_eq!(result.unwrap(), Some(JsString::from("two")));
    }

    #[tokio::test]
    async fn dialog_times_out_to_the_default_value() {
        let broker = DialogBroker::default();
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let emit = move |value: &JsValue| {
            let _ = sender.send(value.clone());
        };
        let options = ExtensionUiDialogOptions {
            timeout: Some(10.0),
            ..ExtensionUiDialogOptions::default()
        };
        let started = Instant::now();
        let result = create_dialog_promise(
            &broker,
            emit,
            Some(options),
            Some("default".to_owned()),
            select_request(),
            |_response| Some("answered".to_owned()),
        )
        .await;
        assert_eq!(result.unwrap(), Some("default".to_owned()));
        assert!(started.elapsed() >= Duration::from_millis(10));
        let request = receiver.recv().await.expect("request line");
        let id = request["id"].as_string().expect("request id").to_utf8().unwrap();
        assert!(!broker.resolve(value_response(&id, "late")));
    }

    #[tokio::test]
    async fn pre_cancelled_dialogs_resolve_without_emitting_a_request() {
        let broker = DialogBroker::default();
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let emit = move |value: &JsValue| {
            let _ = sender.send(value.clone());
        };
        let controller = pi_js_compat::abort::AbortController::new();
        let signal = controller.signal();
        controller.abort(None);
        let options = ExtensionUiDialogOptions {
            signal: Some(signal),
            ..ExtensionUiDialogOptions::default()
        };
        let result = create_dialog_promise(&broker, emit, Some(options), false, select_request(), |_response| true)
            .await
            .unwrap();
        assert!(!result);
        assert!(receiver.try_recv().is_err());
    }

    #[tokio::test]
    async fn cancelled_dialogs_resolve_with_the_default_value() {
        let broker = DialogBroker::default();
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let emit = move |value: &JsValue| {
            let _ = sender.send(value.clone());
        };
        let controller = pi_js_compat::abort::AbortController::new();
        let signal = controller.signal();
        let options = ExtensionUiDialogOptions {
            signal: Some(signal.clone()),
            ..ExtensionUiDialogOptions::default()
        };
        let promise = create_dialog_promise(
            &broker,
            emit,
            Some(options),
            "default",
            select_request(),
            |_response| "answered",
        );
        let resolver = async {
            let _request = receiver.recv().await.expect("request line");
            controller.abort(None);
        };
        let (result, ()) = tokio::join!(promise, resolver);
        assert_eq!(result.unwrap(), "default");
    }

    fn run_frozen_rpc_observation(order: &str) -> JsValue {
        use std::process::{Command, Stdio};
        let lock: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/porting/source-lock.json"
        )))
        .unwrap();
        let source = std::path::Path::new(lock["worktree"].as_str().unwrap());
        let head = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(source)
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8(head.stdout).unwrap().trim(),
            lock["commit"].as_str().unwrap()
        );
        let root = tempfile::tempdir().unwrap();
        let mut child = Command::new("/usr/bin/timeout")
            .args(["20", "/usr/bin/node"])
            .arg(source.join("node_modules/tsx/dist/cli.mjs"))
            .arg("--tsconfig")
            .arg(source.join("tsconfig.json"))
            .arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../oracle/rpc-dialog-order-oracle.mts"))
            .arg(source.join("packages/coding-agent/src"))
            .arg(root.path())
            .arg(order)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let output = child.wait_with_output().unwrap();
        drop(stdin);
        assert!(
            output.status.success(),
            "source stderr {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report =
            js_json::parse_lossless(std::fs::read_to_string(root.path().join("dialog-order.json")).unwrap()).unwrap();
        report["value"].clone()
    }

    #[tokio::test]
    async fn dialog_settlement_follows_response_and_abort_notification_order() {
        for (order, expected) in [
            ("response-first", Some(JsString::from("answered"))),
            ("abort-first", None),
        ] {
            assert_eq!(run_frozen_rpc_observation(order).as_string().cloned(), expected);
            let mut results = Vec::new();
            for _ in 0..32 {
                let broker = DialogBroker::default();
                let controller = pi_js_compat::abort::AbortController::new();
                let options = ExtensionUiDialogOptions {
                    signal: Some(controller.signal()),
                    ..Default::default()
                };
                let emit = |request: &JsValue| {
                    let id = request["id"].as_string().unwrap().to_utf8().unwrap();
                    if order == "response-first" {
                        broker.resolve(value_response(&id, "answered"));
                        controller.abort(None);
                    } else {
                        controller.abort(None);
                        broker.resolve(value_response(&id, "answered"));
                    }
                };
                results.push(
                    create_dialog_promise(&broker, emit, Some(options), None, select_request(), |response| {
                        response.get("value").and_then(JsValue::as_string).cloned()
                    })
                    .await
                    .unwrap(),
                );
            }
            assert!(results.iter().all(|result| result == &expected), "{order}: {results:?}");
        }
    }

    #[tokio::test]
    async fn rpc_noop_surface_and_both_theme_setter_overloads_match_frozen() {
        let expected = run_frozen_rpc_observation("surface");
        crate::modes::interactive::theme::init_theme(Some(&"dark".into()), false).unwrap();
        let ui = RpcUiContext { state: Weak::new() };
        ui.set_footer(Some(Arc::new(|_| panic!("RPC footer factory must not execute"))));
        ui.set_default_footer(Some(Arc::new(|_| {
            panic!("RPC default footer factory must not execute")
        })));
        ui.set_header(Some(Arc::new(|_| panic!("RPC header factory must not execute"))))
            .unwrap();
        ui.set_widget_component(
            &"ignored".into(),
            Some(Arc::new(|_| panic!("RPC widget factory must not execute"))),
            None,
        );
        ui.add_autocomplete_provider(Arc::new(|_| panic!("RPC autocomplete factory must not execute")));
        ui.set_editor_component(Some(Arc::new(|_, _, _| panic!("RPC editor factory must not execute"))));
        ui.request_render();
        ui.set_working_message(Some(&"ignored".into()));
        ui.set_working_visible(false);
        ui.set_working_indicator(Some(&WorkingIndicatorOptions::default()));
        ui.set_hidden_thinking_label(Some(&"ignored".into()));
        ui.set_tools_expanded(true);
        ui.on_terminal_input(Arc::new(|_| panic!("RPC input callback must not execute")))();
        let custom = ui
            .custom(Arc::new(|_, _| panic!("RPC custom factory must not execute")), None)
            .await
            .unwrap();
        let observed = pi_js_compat::js_json!({
            "stringTheme":ui.set_theme(&"dark".into()), "objectTheme":ui.set_theme_object(ui.theme().unwrap()),
            "editor":ui.get_editor_text().unwrap(), "themes":ui.get_all_themes(), "namedTheme":ui.get_theme(&"dark".into()).map(|_| true),
            "toolsExpanded":ui.get_tools_expanded(), "editorComponent":ui.get_editor_component().map(|_| true), "custom":custom
        });
        assert_eq!(observed, expected);
    }

    #[test]
    fn raw_dialog_return_boundaries_reject_unrepresentable_values_explicitly() {
        for method in ["select", "input", "editor"] {
            for value in [JsValue::Null, JsValue::Bool(false), pi_js_compat::js_json!({"x":1})] {
                let response = pi_js_compat::js_json!({"value":value});
                let error = parse_dialog_value(&response, method).unwrap_err();
                assert!(
                    error
                        .to_utf8()
                        .unwrap()
                        .contains(&format!("{method}.extension_ui_response.value"))
                );
            }
            assert_eq!(
                parse_dialog_value(&pi_js_compat::js_json!({"cancelled":{},"value":null}), method),
                Ok(None)
            );
        }
        for value in [JsValue::Null, pi_js_compat::js_json!(17), pi_js_compat::js_json!("yes")] {
            let error = parse_dialog_confirm(&pi_js_compat::js_json!({"confirmed":value})).unwrap_err();
            assert!(
                error
                    .to_utf8()
                    .unwrap()
                    .contains("confirm.extension_ui_response.confirmed")
            );
        }
        assert_eq!(
            parse_dialog_confirm(&pi_js_compat::js_json!({"cancelled":[1],"confirmed":17})),
            Ok(false)
        );
    }

    #[test]
    fn unknown_response_ids_are_ignored() {
        let broker = DialogBroker::default();
        assert!(!broker.resolve(value_response("missing", "value")));
    }

    #[test]
    fn rpc_ui_context_theme_switching_is_rejected() {
        let context = RpcUiContext { state: Weak::new() };
        let result = context.set_theme(&"dark".into());
        assert!(!result.success);
        assert_eq!(
            result.error,
            Some(JsString::from("Theme switching not supported in RPC mode"))
        );
    }
}
