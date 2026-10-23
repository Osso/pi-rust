//! Port of `packages/coding-agent/src/modes/rpc/rpc-mode.ts`.
//!
//! Reads JSON commands on stdin and writes responses, extension UI requests, and
//! session events on stdout. The TypeScript mode never resolves; shutdown exits
//! the process exactly like `process.exit`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use pi_agent_core::agent::types::AgentEvent;
use pi_ai::request::{BoxFuture, CallbackError};
use pi_ai::types::InputSource;
use pi_ai::utils::random_uuid::random_uuid;
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsString, JsValue, encode_node_utf8};
use serde::Serialize;
use tokio::sync::oneshot;
use tokio::task::{JoinHandle, LocalSet};

use crate::config::get_user_state_root;
use crate::core::agent_session::{
    AgentSession, AgentSessionEvent, CycleDirection, ExecuteBashOptions, ExtensionBindings, PromptOptions,
    SessionStats, StreamingBehavior,
};
use crate::core::agent_session_runtime::{
    AgentSessionRuntime, ForkPosition, NewSessionOptions as RuntimeNewSessionOptions,
};
use crate::core::bash_executor::BashResult;
use crate::core::compaction::CompactionResult;
use crate::core::extensions::types::{
    ExtensionError, ExtensionMode, ExtensionUiContext, ExtensionUiDialogOptions, ExtensionWidgetOptions, NotifyType,
    ThemeSetResult, UserBashEvent, UserBashEventResult, WorkingIndicatorOptions,
};
use crate::core::output_guard::{flush_raw_stdout, wait_for_raw_stdout_backpressure, write_raw_stdout};
use crate::core::session_control_db::get_control_db_path;
use crate::core::session_manager::{SessionEntry, SessionTreeNode};
use crate::core::slash_commands::{SlashCommandInfo, SlashCommandSource};
use crate::modes::extension_actions::command_context_actions;
use crate::modes::rpc::jsonl::{JsonlLineReader, attach_jsonl_line_reader, serialize_json_line};
use crate::modes::rpc::rpc_types::{
    ExtensionUiRequestTag, RpcCommand, RpcCommandBody, RpcExtensionUiRequest, RpcExtensionUiRequestPayload,
    RpcExtensionUiResponse, RpcExtensionUiResponsePayload, RpcResponse, RpcSessionState, RpcSlashCommand,
};
use crate::utils::shell::kill_tracked_detached_children;
use pi_js_compat::js_json;

type PendingSender = oneshot::Sender<RpcExtensionUiResponse>;

/// Pending extension UI requests awaiting a client response line.
#[derive(Default)]
struct DialogBroker {
    pending: Mutex<HashMap<String, PendingSender>>,
}

impl DialogBroker {
    fn register(&self, id: String, sender: PendingSender) {
        lock(&self.pending).insert(id, sender);
    }

    /// Routes a response line to its request; unknown or already-settled ids are ignored.
    fn resolve(&self, response: RpcExtensionUiResponse) -> bool {
        let sender = lock(&self.pending).remove(&response.id);
        match sender {
            Some(sender) => sender.send(response).is_ok(),
            None => false,
        }
    }

    fn remove(&self, id: &str) {
        lock(&self.pending).remove(id);
    }
}

#[derive(Default)]
struct Subscriptions {
    session: Vec<u64>,
    agent: Vec<u64>,
}

struct RpcState {
    session: Mutex<AgentSession>,
    runtime: Arc<tokio::sync::Mutex<AgentSessionRuntime>>,
    agent_dir: Option<JsString>,
    dialogs: DialogBroker,
    shutdown_requested: AtomicBool,
    shutting_down: AtomicBool,
    subscriptions: Mutex<Subscriptions>,
    input: Mutex<Option<JsonlLineReader>>,
}

impl RpcState {
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

    fn output(&self, value: &JsValue) {
        write_json_line(value);
    }

    fn output_response(&self, response: &RpcResponse) {
        self.output(&js_json::to_value(response).expect("rpc responses must serialize"));
    }
}

/// Runs RPC mode until shutdown exits the process.
pub async fn run_rpc_mode(runtime: Arc<tokio::sync::Mutex<AgentSessionRuntime>>) -> Result<(), CallbackError> {
    let (session, agent_dir) = {
        let runtime = runtime.lock().await;
        (runtime.session().clone(), Some(runtime.services().agent_dir.clone()))
    };
    let state = Arc::new(RpcState {
        session: Mutex::new(session),
        agent_dir,
        runtime,
        dialogs: DialogBroker::default(),
        shutdown_requested: AtomicBool::new(false),
        shutting_down: AtomicBool::new(false),
        subscriptions: Mutex::new(Subscriptions::default()),
        input: Mutex::new(None),
    });

    set_rebind_session(&state).await;
    rebind_session(&state).await?;
    let signal_tasks = register_signal_handlers(Arc::clone(&state));

    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel::<String>();
    let reader = attach_jsonl_line_reader(tokio::io::stdin(), move |line| {
        let _ = sender.send(line);
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
            let state = Arc::clone(&state);
            shutdown(&state, 0, None).await;
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

    let command = match deserialize_command(state, &parsed) {
        Some(command) => command,
        None => {
            wait_for_raw_stdout_backpressure().await;
            check_shutdown_requested(state).await;
            return;
        }
    };

    match handle_command(state, &command).await {
        Ok(Some(response)) => {
            state.output_response(&response);
            wait_for_raw_stdout_backpressure().await;
        }
        Ok(None) => {}
        Err(message) => {
            state.output_response(&RpcResponse::error(
                command.id.clone(),
                command.body.command_type(),
                message.message,
            ));
            wait_for_raw_stdout_backpressure().await;
            return;
        }
    }
    check_shutdown_requested(state).await;
}

/// Parses a command line, emitting the source's unknown-command error when the
/// discriminator is not part of the RPC command union.
fn deserialize_command(state: &RpcState, parsed: &JsValue) -> Option<RpcCommand> {
    let type_name = parsed
        .get("type")
        .and_then(JsValue::as_string)
        .and_then(|text| text.to_utf8().ok());
    let type_name = type_name.as_deref();
    if type_name.is_none_or(|name| !crate::modes::rpc::rpc_types::is_known_command_type(name)) {
        let id = parsed.get("id").and_then(JsValue::as_string).cloned();
        let label = type_name.unwrap_or("undefined");
        state.output_response(&RpcResponse::error_optional(
            id,
            type_name.map(str::to_owned),
            format!("Unknown command: {label}"),
        ));
        return None;
    }

    match js_json::from_value::<RpcCommand>(parsed) {
        Ok(command) => Some(command),
        Err(error) => {
            let id = parsed.get("id").and_then(JsValue::as_string).cloned();
            state.output_response(&RpcResponse::error(
                id,
                type_name.unwrap_or("undefined"),
                error.to_string(),
            ));
            None
        }
    }
}

fn route_extension_ui_response(state: &RpcState, parsed: JsValue) {
    let Ok(response) = js_json::from_value::<RpcExtensionUiResponse>(&parsed) else {
        return;
    };
    state.dialogs.resolve(response);
}

async fn handle_command(state: &Arc<RpcState>, command: &RpcCommand) -> Result<Option<RpcResponse>, CallbackError> {
    let id = command.id.clone();
    let session = state.current_session();
    match &command.body {
        RpcCommandBody::Prompt {
            message,
            images,
            streaming_behavior,
        } => {
            start_prompt(
                state,
                id,
                message.clone(),
                images.clone().unwrap_or_default(),
                streaming_behavior.map(to_session_streaming_behavior),
            );
            Ok(None)
        }
        RpcCommandBody::Steer { message, images } => {
            session
                .steer(message, images.as_deref().unwrap_or_default())
                .await
                .map_err(|error| error.message().to_owned())?;
            Ok(Some(RpcResponse::success(id, "steer")))
        }
        RpcCommandBody::FollowUp { message, images } => {
            session
                .follow_up(message, images.as_deref().unwrap_or_default())
                .await
                .map_err(|error| error.message().to_owned())?;
            Ok(Some(RpcResponse::success(id, "follow_up")))
        }
        RpcCommandBody::Abort => {
            session.abort().await.map_err(|error| error.message().to_owned())?;
            Ok(Some(RpcResponse::success(id, "abort")))
        }
        RpcCommandBody::Interrupt => {
            let (steering, follow_up) = session.clear_queue();
            let message = JsString::join(steering.into_iter().chain(follow_up), "\n\n");
            let session = session.clone();
            tokio::task::spawn_local(async move {
                if let Err(error) = session.interrupt(&message, InputSource::Rpc).await {
                    eprintln!("RPC interrupt failed: {}", encode_node_utf8(error.message()));
                }
            });
            Ok(Some(RpcResponse::success(id, "interrupt")))
        }
        RpcCommandBody::NewSession { parent_session } => {
            let result = {
                let mut runtime = state.runtime.lock().await;
                runtime
                    .new_session(RuntimeNewSessionOptions {
                        parent_session: parent_session.clone(),
                        setup: None,
                        with_session: None,
                    })
                    .await?
            };
            if !result.cancelled {
                rebind_session(state).await?;
            }
            Ok(Some(RpcResponse::success_with_data(
                id,
                "new_session",
                pi_js_compat::js_json!({"cancelled": result.cancelled}),
            )))
        }
        RpcCommandBody::GetState => Ok(Some(RpcResponse::success_with_data(
            id,
            "get_state",
            js_json::to_value(&session_state(&session)).expect("state must serialize"),
        ))),
        RpcCommandBody::SetModel { provider, model_id } => {
            let model = session
                .model_registry()
                .get_available()
                .map_err(|error| error.message)?
                .into_iter()
                .find(|model| &model.provider == provider && &model.id == model_id)
                .ok_or_else(|| format!("Model not found: {provider}/{model_id}"))?;
            session
                .set_model(model.clone())
                .await
                .map_err(|error| error.message().to_owned())?;
            Ok(Some(RpcResponse::success_with_data(
                id,
                "set_model",
                js_json::to_value(&model).expect("model must serialize"),
            )))
        }
        RpcCommandBody::CycleModel => {
            let result = session
                .cycle_model(CycleDirection::Forward)
                .await
                .map_err(|error| error.message().to_owned())?;
            let data = match result {
                Some(result) => js_json::to_value(&result).expect("cycle result must serialize"),
                None => JsValue::Null,
            };
            Ok(Some(RpcResponse::success_with_data(id, "cycle_model", data)))
        }
        RpcCommandBody::GetAvailableModels => {
            let models = session
                .model_registry()
                .get_available()
                .map_err(|error| error.message)?;
            Ok(Some(RpcResponse::success_with_data(
                id,
                "get_available_models",
                pi_js_compat::js_json!({"models": models}),
            )))
        }
        RpcCommandBody::SetThinkingLevel { level } => {
            session
                .set_thinking_level(*level)
                .map_err(|error| error.message().to_owned())?;
            Ok(Some(RpcResponse::success(id, "set_thinking_level")))
        }
        RpcCommandBody::CycleThinkingLevel => {
            let data = match session
                .cycle_thinking_level()
                .map_err(|error| error.message().to_owned())?
            {
                Some(level) => pi_js_compat::js_json!({"level": level}),
                None => JsValue::Null,
            };
            Ok(Some(RpcResponse::success_with_data(id, "cycle_thinking_level", data)))
        }
        // Source discrepancy: `rpc-types.ts` and `rpc-client.ts` declare and send
        // `get_available_thinking_levels`, but `rpc-mode.ts` has no case for it, so
        // the server answers with the unknown-command error. Preserved verbatim.
        RpcCommandBody::GetAvailableThinkingLevels => Ok(Some(RpcResponse::error(
            id,
            "get_available_thinking_levels",
            "Unknown command: get_available_thinking_levels",
        ))),
        RpcCommandBody::SetSteeringMode { mode } => {
            session.set_steering_mode(*mode);
            Ok(Some(RpcResponse::success(id, "set_steering_mode")))
        }
        RpcCommandBody::SetFollowUpMode { mode } => {
            session.set_follow_up_mode(*mode);
            Ok(Some(RpcResponse::success(id, "set_follow_up_mode")))
        }
        RpcCommandBody::Compact { custom_instructions } => {
            let result: CompactionResult = session
                .compact(custom_instructions.as_ref())
                .await
                .map_err(|error| error.message().to_owned())?;
            Ok(Some(RpcResponse::success_with_data(
                id,
                "compact",
                js_json::to_value(&result).expect("compaction result must serialize"),
            )))
        }
        RpcCommandBody::SetAutoCompaction { enabled } => {
            session.set_auto_compaction_enabled(*enabled);
            Ok(Some(RpcResponse::success(id, "set_auto_compaction")))
        }
        RpcCommandBody::SetAutoRetry { enabled } => {
            session.set_auto_retry_enabled(*enabled);
            Ok(Some(RpcResponse::success(id, "set_auto_retry")))
        }
        RpcCommandBody::AbortRetry => {
            session.abort_retry();
            Ok(Some(RpcResponse::success(id, "abort_retry")))
        }
        RpcCommandBody::Bash {
            command: bash_command,
            exclude_from_context,
        } => {
            let result = run_bash(&session, bash_command, *exclude_from_context).await?;
            Ok(Some(RpcResponse::success_with_data(
                id,
                "bash",
                js_json::to_value(&result).expect("bash result must serialize"),
            )))
        }
        RpcCommandBody::AbortBash => {
            session.abort_bash();
            Ok(Some(RpcResponse::success(id, "abort_bash")))
        }
        RpcCommandBody::GetSessionStats => {
            let stats: SessionStats = session.get_session_stats();
            Ok(Some(RpcResponse::success_with_data(
                id,
                "get_session_stats",
                js_json::to_value(&stats).expect("session stats must serialize"),
            )))
        }
        RpcCommandBody::ExportHtml { output_path } => {
            let path = session
                .export_to_html(output_path.as_ref())
                .await
                .map_err(|error| error.message().to_owned())?;
            Ok(Some(RpcResponse::success_with_data(
                id,
                "export_html",
                pi_js_compat::js_json!({"path": path}),
            )))
        }
        RpcCommandBody::SwitchSession { session_path } => {
            let result = {
                let mut runtime = state.runtime.lock().await;
                runtime.switch_session(session_path, None, None).await?
            };
            if !result.cancelled {
                rebind_session(state).await?;
            }
            Ok(Some(RpcResponse::success_with_data(
                id,
                "switch_session",
                pi_js_compat::js_json!({"cancelled": result.cancelled}),
            )))
        }
        RpcCommandBody::Fork { entry_id } => {
            let result = {
                let mut runtime = state.runtime.lock().await;
                runtime.fork(entry_id, ForkPosition::Before, None).await?
            };
            if !result.cancelled {
                rebind_session(state).await?;
            }
            let data = ForkResponseData {
                text: result.selected_text,
                cancelled: result.cancelled,
            };
            Ok(Some(RpcResponse::success_with_data(
                id,
                "fork",
                js_json::to_value(&data).expect("fork data must serialize"),
            )))
        }
        RpcCommandBody::Clone => {
            let leaf_id = session.session_manager().get_leaf_id().cloned();
            let Some(leaf_id) = leaf_id else {
                return Err("Cannot clone session: no current entry selected".into());
            };
            let result = {
                let mut runtime = state.runtime.lock().await;
                runtime.fork(&leaf_id, ForkPosition::At, None).await?
            };
            if !result.cancelled {
                rebind_session(state).await?;
            }
            Ok(Some(RpcResponse::success_with_data(
                id,
                "clone",
                pi_js_compat::js_json!({"cancelled": result.cancelled}),
            )))
        }
        RpcCommandBody::GetForkMessages => {
            let messages: Vec<ForkMessageData> = session
                .get_user_messages_for_forking()
                .into_iter()
                .map(|message| ForkMessageData {
                    entry_id: message.entry_id,
                    text: message.text,
                })
                .collect();
            Ok(Some(RpcResponse::success_with_data(
                id,
                "get_fork_messages",
                pi_js_compat::js_json!({"messages": messages}),
            )))
        }
        RpcCommandBody::GetEntries { since } => {
            let manager = session.session_manager();
            let mut entries = manager.get_entries();
            if let Some(since) = since {
                let Some(index) = entries.iter().position(|entry| entry.id() == Some(since)) else {
                    return Err(js_format!("Entry not found: {}", since).into());
                };
                entries = entries.into_iter().skip(index + 1).collect();
            }
            let leaf_id = manager.get_leaf_id().cloned();
            let data = EntriesResponseData { entries, leaf_id };
            Ok(Some(RpcResponse::success_with_data(
                id,
                "get_entries",
                js_json::to_value(&data).expect("entries must serialize"),
            )))
        }
        RpcCommandBody::GetTree => {
            let manager = session.session_manager();
            let data = TreeResponseData {
                tree: manager.get_tree(),
                leaf_id: manager.get_leaf_id().cloned(),
            };
            Ok(Some(RpcResponse::success_with_data(
                id,
                "get_tree",
                js_json::to_value(&data).expect("tree must serialize"),
            )))
        }
        RpcCommandBody::GetLastAssistantText => {
            let text = session.get_last_assistant_text();
            Ok(Some(RpcResponse::success_with_data(
                id,
                "get_last_assistant_text",
                js_json::to_value(&LastAssistantTextData { text }).expect("text must serialize"),
            )))
        }
        RpcCommandBody::SetSessionName { name } => {
            let name = name.trim();
            if name.is_empty() {
                return Err("Session name cannot be empty".into());
            }
            session
                .set_session_name(&name)
                .map_err(|failure| failure.message().to_owned())?;
            Ok(Some(RpcResponse::success(id, "set_session_name")))
        }
        RpcCommandBody::GetMessages => Ok(Some(RpcResponse::success_with_data(
            id,
            "get_messages",
            pi_js_compat::js_json!({"messages": session.messages()}),
        ))),
        RpcCommandBody::GetCommands => {
            let commands = collect_commands(&session);
            Ok(Some(RpcResponse::success_with_data(
                id,
                "get_commands",
                pi_js_compat::js_json!({"commands": commands}),
            )))
        }
    }
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
        message_count: session.messages().len() as u64,
        pending_message_count: session.pending_message_count() as u64,
    }
}

async fn run_bash(
    session: &AgentSession,
    command: &JsString,
    exclude_from_context: Option<bool>,
) -> Result<BashResult, JsString> {
    let cwd = session.session_manager().get_cwd().to_owned();
    let event_result: Option<UserBashEventResult> = session
        .extension_runner()
        .emit_user_bash(&UserBashEvent {
            command: command.to_owned(),
            exclude_from_context: exclude_from_context.unwrap_or(false),
            cwd,
        })
        .await;

    if let Some(result) = event_result.as_ref().and_then(|result| result.result.clone()) {
        let result: BashResult =
            js_json::from_value(&result).map_err(|error| format!("Invalid user_bash result: {error}"))?;
        session.record_bash_result(command, &result, exclude_from_context);
        return Ok(result);
    }
    let operations = event_result.as_ref().and_then(|result| result.operations.clone());

    session
        .execute_bash(
            command,
            None,
            ExecuteBashOptions {
                exclude_from_context,
                operations,
            },
        )
        .await
        .map_err(|error| error.message().to_owned())
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
    id: Option<JsString>,
    message: JsString,
    images: Vec<pi_ai::types::ImageContent>,
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
                state.output_response(&RpcResponse::success(preflight_id.clone(), "prompt"));
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
            state.output_response(&RpcResponse::error(error_id, "prompt", error.message()));
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
    state.runtime.lock().await.set_rebind_session(Some(handler));
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
                Arc::clone(&state.runtime),
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
    let id = session.subscribe(Arc::new(move |event: &AgentSessionEvent| {
        let Some(state) = listener_state.upgrade() else {
            return;
        };
        state.output(&js_json::to_value(event).expect("session event must serialize"));
    }));
    lock(&state.subscriptions).session.push(id);

    let backpressure_state = Arc::downgrade(state);
    let id = session
        .agent()
        .subscribe(Arc::new(move |_event: AgentEvent, _signal: AbortSignal| {
            let backpressure_state = backpressure_state.clone();
            Box::pin(async move {
                let _ = backpressure_state;
                wait_for_raw_stdout_backpressure().await;
                Ok(())
            })
        }));
    lock(&state.subscriptions).agent.push(id);
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
    state.runtime.lock().await.dispose().await;
    if let Some(reader) = lock(&state.input).take() {
        reader.detach();
    }
    if signal != Some("SIGTERM") {
        flush_raw_stdout().await;
    }
    std::process::exit(exit_code);
}

fn register_signal_handlers(state: Arc<RpcState>) -> Vec<JoinHandle<()>> {
    vec![
        spawn_signal_handler(
            tokio::signal::unix::SignalKind::terminate(),
            "SIGTERM",
            143,
            Arc::clone(&state),
        ),
        spawn_signal_handler(tokio::signal::unix::SignalKind::hangup(), "SIGHUP", 129, state),
    ]
}

#[cfg(unix)]
fn spawn_signal_handler(
    kind: tokio::signal::unix::SignalKind,
    signal: &'static str,
    exit_code: i32,
    state: Arc<RpcState>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let Ok(mut signals) = tokio::signal::unix::signal(kind) else {
            return;
        };
        if signals.recv().await.is_none() {
            return;
        }
        kill_tracked_detached_children();
        shutdown(&state, exit_code, Some(signal)).await;
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
    ) -> BoxFuture<'a, Result<Option<JsString>, JsString>> {
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
            Ok(create_dialog_promise(
                &state.dialogs,
                |value| state.output(value),
                opts,
                None,
                request,
                |response| match &response.payload {
                    RpcExtensionUiResponsePayload::Value { value } => Some(value.clone()),
                    RpcExtensionUiResponsePayload::Confirmed { .. }
                    | RpcExtensionUiResponsePayload::Cancelled { .. } => None,
                },
            )
            .await)
        })
    }

    fn confirm<'a>(
        &'a self,
        title: &JsString,
        message: &JsString,
        opts: Option<&ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<bool, JsString>> {
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
            Ok(create_dialog_promise(
                &state.dialogs,
                |value| state.output(value),
                opts,
                false,
                request,
                |response| {
                    matches!(
                        &response.payload,
                        RpcExtensionUiResponsePayload::Confirmed { confirmed: true }
                    )
                },
            )
            .await)
        })
    }

    fn input<'a>(
        &'a self,
        title: &JsString,
        placeholder: Option<&JsString>,
        opts: Option<&ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<Option<JsString>, JsString>> {
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
            Ok(create_dialog_promise(
                &state.dialogs,
                |value| state.output(value),
                opts,
                None,
                request,
                |response| match &response.payload {
                    RpcExtensionUiResponsePayload::Value { value } => Some(value.clone()),
                    RpcExtensionUiResponsePayload::Confirmed { .. }
                    | RpcExtensionUiResponsePayload::Cancelled { .. } => None,
                },
            )
            .await)
        })
    }

    fn notify(&self, message: &JsString, notify_type: Option<NotifyType>) {
        let Some(state) = self.state() else {
            return;
        };
        output_ui_request(
            &state,
            RpcExtensionUiRequestPayload::Notify {
                message: message.clone(),
                notify_type,
            },
        );
    }

    fn set_status(&self, key: &JsString, text: Option<&JsString>) {
        let Some(state) = self.state() else {
            return;
        };
        output_ui_request(
            &state,
            RpcExtensionUiRequestPayload::SetStatus {
                status_key: key.clone(),
                status_text: text.cloned(),
            },
        );
    }

    fn set_widget(&self, key: &JsString, content: Option<&[JsString]>, options: Option<&ExtensionWidgetOptions>) {
        let Some(state) = self.state() else {
            return;
        };
        output_ui_request(
            &state,
            RpcExtensionUiRequestPayload::SetWidget {
                widget_key: key.clone(),
                widget_lines: content.map(<[JsString]>::to_vec),
                widget_placement: options.and_then(|options| options.placement),
            },
        );
    }

    fn set_title(&self, title: &JsString) {
        let Some(state) = self.state() else {
            return;
        };
        output_ui_request(&state, RpcExtensionUiRequestPayload::SetTitle { title: title.clone() });
    }

    fn paste_to_editor(&self, text: &JsString) {
        self.set_editor_text(text);
    }

    fn set_editor_text(&self, text: &JsString) {
        let Some(state) = self.state() else {
            return;
        };
        output_ui_request(
            &state,
            RpcExtensionUiRequestPayload::SetEditorText { text: text.clone() },
        );
    }

    fn editor<'a>(
        &'a self,
        title: &JsString,
        prefill: Option<&JsString>,
    ) -> BoxFuture<'a, Result<Option<JsString>, JsString>> {
        let state = self.state();
        let request = RpcExtensionUiRequestPayload::Editor {
            title: title.clone(),
            prefill: prefill.cloned(),
        };
        Box::pin(async move {
            let Some(state) = state else {
                return Ok(None);
            };
            Ok(create_dialog_promise(
                &state.dialogs,
                |value| state.output(value),
                None,
                None,
                request,
                |response| match &response.payload {
                    RpcExtensionUiResponsePayload::Value { value } => Some(value.clone()),
                    RpcExtensionUiResponsePayload::Confirmed { .. }
                    | RpcExtensionUiResponsePayload::Cancelled { .. } => None,
                },
            )
            .await)
        })
    }

    fn set_theme(&self, _theme: &JsString) -> ThemeSetResult {
        ThemeSetResult {
            success: false,
            error: Some("Theme switching not supported in RPC mode".into()),
        }
    }

    fn set_working_message(&self, _message: Option<&JsString>) {}

    fn set_working_visible(&self, _visible: bool) {}

    fn set_working_indicator(&self, _options: Option<&WorkingIndicatorOptions>) {}

    fn set_hidden_thinking_label(&self, _label: Option<&JsString>) {}

    fn request_render(&self) {}
}

fn output_ui_request(state: &RpcState, payload: RpcExtensionUiRequestPayload) {
    let request = RpcExtensionUiRequest {
        kind: ExtensionUiRequestTag::ExtensionUiRequest,
        id: random_uuid(),
        payload,
    };
    state.output(&js_json::to_value(&request).expect("extension UI request must serialize"));
}

async fn create_dialog_promise<T, P, F>(
    dialogs: &DialogBroker,
    emit: F,
    opts: Option<ExtensionUiDialogOptions>,
    default: T,
    request: RpcExtensionUiRequestPayload,
    parse: P,
) -> T
where
    P: FnOnce(&RpcExtensionUiResponse) -> T,
    F: Fn(&JsValue),
{
    let (signal, timeout) = match opts {
        Some(opts) => (opts.signal, opts.timeout),
        None => (None, None),
    };
    if signal.as_ref().is_some_and(AbortSignal::aborted) {
        return default;
    }

    let id = random_uuid();
    let (sender, receiver) = oneshot::channel();
    dialogs.register(id.clone(), sender);
    let request = RpcExtensionUiRequest {
        kind: ExtensionUiRequestTag::ExtensionUiRequest,
        id: id.clone(),
        payload: request,
    };
    emit(&js_json::to_value(&request).expect("extension UI request must serialize"));

    let response = await_dialog(receiver, signal, timeout).await;
    dialogs.remove(&id);
    match response {
        Some(response) => parse(&response),
        None => default,
    }
}

async fn await_dialog(
    receiver: oneshot::Receiver<RpcExtensionUiResponse>,
    signal: Option<AbortSignal>,
    timeout: Option<f64>,
) -> Option<RpcExtensionUiResponse> {
    let timeout = timeout
        .filter(|timeout| *timeout != 0.0 && !timeout.is_nan())
        .map(|timeout| Duration::from_millis(timeout.max(0.0) as u64));
    match (signal, timeout) {
        (Some(signal), Some(timeout)) => tokio::select! {
            response = receiver => response.ok(),
            _ = signal.wait() => None,
            () = tokio::time::sleep(timeout) => None,
        },
        (Some(signal), None) => tokio::select! {
            response = receiver => response.ok(),
            _ = signal.wait() => None,
        },
        (None, Some(timeout)) => tokio::select! {
            response = receiver => response.ok(),
            () = tokio::time::sleep(timeout) => None,
        },
        (None, None) => receiver.await.ok(),
    }
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

    fn value_response(id: &str, value: &str) -> RpcExtensionUiResponse {
        RpcExtensionUiResponse {
            kind: crate::modes::rpc::rpc_types::ExtensionUiResponseTag::ExtensionUiResponse,
            id: id.to_owned(),
            payload: RpcExtensionUiResponsePayload::Value { value: value.into() },
        }
    }

    #[tokio::test]
    async fn dialog_resolves_the_parsed_client_response() {
        let broker = DialogBroker::default();
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let emit = move |value: &JsValue| {
            let _ = sender.send(value.clone());
        };
        let promise = create_dialog_promise(&broker, emit, None, None, select_request(), |response| match &response
            .payload
        {
            RpcExtensionUiResponsePayload::Value { value } => Some(value.clone()),
            _ => None,
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
        assert_eq!(result, Some(JsString::from("two")));
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
        assert_eq!(result, Some("default".to_owned()));
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
        let result =
            create_dialog_promise(&broker, emit, Some(options), false, select_request(), |_response| true).await;
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
        assert_eq!(result, "default");
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

#[cfg(test)]
mod bash_operation_tests {
    use std::sync::{Arc, Mutex};

    use pi_agent_core::ToolError;
    use pi_ai::request::BoxFuture;

    use super::run_bash;
    use crate::core::event_bus::EventBusController;
    use crate::core::extensions::types::{UserBashEvent, UserBashEventResult};
    use crate::core::extensions::{
        ExtensionFactory, LoadExtensionsResult, create_extension_runtime, load_extension_from_factory,
    };
    use crate::core::resource_loader::StaticResourceLoader;
    use crate::core::sdk::{CreateAgentSessionOptions, create_agent_session};
    use crate::core::session_manager::SessionManager;
    use crate::core::tools::bash::{BashExecOptions, BashExecResult, BashOperations};
    use pi_js_compat::js_json::JsString;

    struct RecordingOperations {
        commands: Arc<Mutex<Vec<JsString>>>,
    }

    impl BashOperations for RecordingOperations {
        fn exec<'a>(
            &'a self,
            command: &'a JsString,
            _cwd: &'a JsString,
            options: BashExecOptions,
        ) -> BoxFuture<'a, Result<BashExecResult, ToolError>> {
            self.commands
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(command.to_owned());
            (options.on_data)(b"from extension\n");
            Box::pin(async {
                Ok(BashExecResult {
                    exit_code: Some(0),
                    detached: None,
                })
            })
        }
    }

    #[tokio::test]
    async fn user_bash_operations_reach_execute_bash() {
        let commands = Arc::new(Mutex::new(Vec::new()));
        let handler_commands = Arc::clone(&commands);
        let factory: ExtensionFactory = Arc::new(move |api| {
            let handler_commands = Arc::clone(&handler_commands);
            api.on_user_bash(move |_event: UserBashEvent, _ctx| {
                let handler_commands = Arc::clone(&handler_commands);
                Box::pin(async move {
                    Ok(Some(UserBashEventResult {
                        operations: Some(Arc::new(RecordingOperations {
                            commands: handler_commands,
                        }) as Arc<dyn BashOperations>),
                        result: None,
                    }))
                })
                    as crate::core::extensions::types::BoxFuture<
                        'static,
                        Result<Option<UserBashEventResult>, crate::core::extensions::types::HandlerError>,
                    >
            })
            .expect("register user_bash");
            Box::pin(async { Ok(()) })
        });
        let temp = tempfile::tempdir().expect("temp dir");
        let temp_path = |path: &std::path::Path| JsString::from(path.to_str().expect("UTF-8 temp path"));
        let cwd = temp_path(temp.path());
        let runtime = create_extension_runtime();
        let extension = load_extension_from_factory(
            factory,
            &cwd,
            Arc::new(EventBusController::new()),
            runtime.clone(),
            &"user-bash-extension".into(),
        )
        .await
        .expect("extension");
        let loader = Arc::new(StaticResourceLoader {
            extensions: Some(LoadExtensionsResult {
                extensions: vec![extension],
                errors: Vec::new(),
                runtime,
            }),
            ..StaticResourceLoader::default()
        });
        let session_dir = temp_path(&temp.path().join("sessions"));
        let control_db = temp_path(&temp.path().join("control.db"));
        let mut session_manager = SessionManager::create(&cwd, Some(&session_dir), None).expect("session manager");
        session_manager
            .set_metadata_control_db_path(Some(&control_db), Default::default())
            .expect("metadata control db");
        let result = create_agent_session(CreateAgentSessionOptions {
            cwd: Some(cwd.clone()),
            agent_dir: Some(cwd),
            resource_loader: Some(loader),
            session_manager: Some(session_manager),
            control_db_path: Some(control_db),
            ..Default::default()
        })
        .await
        .expect("session");

        let command = JsString::from("echo ") + JsString::from_utf16(vec![0xd800]);
        let bash = run_bash(&result.session, &command, None).await.expect("bash");
        assert_eq!(bash.output, "from extension\n");
        assert_eq!(
            commands
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .as_slice(),
            [command]
        );
    }
}
