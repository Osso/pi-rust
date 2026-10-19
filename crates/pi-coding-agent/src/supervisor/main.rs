//! Port of `packages/coding-agent/src/supervisor/main.ts`.
//!
//! Session creation, extension loading, and console wiring are blocked on the Rust extension API
//! and agent-session slices; the portable configuration, gating, prompt-queue, request-loop, and
//! response-reading contracts are ported here.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use pi_agent_core::agent::AgentMessage;
use pi_agent_core::harness::types::BeforeToolCallResult;
use pi_ai::request::BoxFuture;
use pi_ai::session_resources::cleanup_session_resources;
use pi_ai::types::{AssistantContent, StopReason};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::core::resident_console_transport::{ResidentConsoleService, ResidentConsoleSnapshot};
use crate::core::session_control_db::{
    ControlDbError, SupervisorRequest, SupervisorResponse, claim_next_supervisor_request, complete_supervisor_request,
    recover_supervisor_requests,
};
use crate::core::session_manager::{SessionEntry, SessionManager};
use crate::core::settings_manager::{Settings, SettingsManager, SettingsManagerCreateOptions};
use crate::supervisor::request_wake::SupervisorRequestWakeServer;
use crate::supervisor::response_tool::SUPERVISOR_RESPONSE_TOOL_NAME;
use crate::supervisor::service::{RunSupervisorRequestInput, SupervisorEvaluator, run_supervisor_request};
use crate::utils::paths::resolve_path;

pub const SUPERVISOR_SESSION_ID: &str = "supervisor";
pub const SUPERVISOR_COMPACTION_PERCENT: f64 = 75.0;
pub const SUPERVISOR_TOOL_NAMES: [&str; 3] = ["read", "edit", "write"];
pub const SUPERVISOR_EXCLUDED_TOOL_NAMES: [&str; 12] = [
    "ask_architect",
    "attach_session_agent",
    "bash",
    "channel_post",
    "contact_parent",
    "loop",
    "manage_goal",
    "pyrun_eval",
    "restart_self",
    "resume_session",
    "send_agent_message",
    "spawn_agent",
];

const SUPERVISOR_COMPACTION_INSTRUCTIONS: &str =
    "Preserve Supervisor decisions, project-specific policies, and reusable approval rationale.";

/// One queued console prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupervisorConsolePrompt {
    pub id: String,
    pub text: String,
}

/// FIFO queue for console prompts.
#[derive(Debug, Default)]
pub struct SupervisorConsolePromptQueue {
    prompts: VecDeque<SupervisorConsolePrompt>,
}

impl SupervisorConsolePromptQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enqueue(&mut self, text: String, id: String) {
        self.prompts.push_back(SupervisorConsolePrompt { id, text });
    }

    pub fn take(&mut self) -> Option<SupervisorConsolePrompt> {
        self.prompts.pop_front()
    }
}

/// `createSupervisorSettingsManager`.
pub fn create_supervisor_settings_manager() -> SettingsManager {
    let mut settings = Settings::new();
    settings.insert("approvalPolicy".to_owned(), Value::String("auto-approve".to_owned()));
    settings.insert("approvalPreset".to_owned(), Value::String("auto-approve".to_owned()));
    settings.insert("sandboxProfile".to_owned(), Value::String("full-access".to_owned()));
    SettingsManager::in_memory(settings, SettingsManagerCreateOptions { project_trusted: None })
}

/// `validateSupervisorExtensionLoad` over the diagnostic `path: error` pairs.
pub fn validate_supervisor_extension_load(errors: &[(String, String)]) -> Result<(), String> {
    if errors.is_empty() {
        return Ok(());
    }
    let details = errors
        .iter()
        .map(|(path, error)| format!("{path}: {error}"))
        .collect::<Vec<_>>()
        .join("; ");
    Err(format!("Supervisor extension load failed: {details}"))
}

/// `blockSupervisorFileAccess`.
pub fn block_supervisor_file_access(kb_dir: &str, tool_name: &str, input: &Value) -> Option<BeforeToolCallResult> {
    if !SUPERVISOR_TOOL_NAMES.contains(&tool_name) {
        return None;
    }
    let Some(path) = read_tool_path(input) else {
        return Some(supervisor_file_access_block());
    };
    let resolved = if tool_name == "read" {
        crate::core::tools::path_utils::resolve_read_path(path, kb_dir)
    } else {
        crate::core::tools::path_utils::resolve_to_cwd(path, kb_dir)
    };
    let Ok(resolved_path) = resolved else {
        return Some(supervisor_file_access_block());
    };
    let within = resolve_existing_path(&resolved_path)
        .zip(resolve_existing_path(kb_dir))
        .is_some_and(|(path, root)| is_path_within_root(&path, &root));
    if within {
        None
    } else {
        Some(supervisor_file_access_block())
    }
}

fn read_tool_path(input: &Value) -> Option<&str> {
    input.get("path").and_then(Value::as_str)
}

fn supervisor_file_access_block() -> BeforeToolCallResult {
    BeforeToolCallResult {
        block: Some(true),
        reason: Some("Pi Supervisor may access files only inside the configured KB directory.".to_owned()),
        terminate: None,
    }
}

fn resolve_existing_path(path: &str) -> Option<String> {
    let resolved = resolve_path(path, &process_cwd(), &Default::default()).ok()?;
    let mut suffix: Vec<String> = Vec::new();
    let mut existing = PathBuf::from(&resolved);
    while !existing.exists() {
        let parent = existing.parent()?.to_path_buf();
        if parent == existing {
            return Some(resolved);
        }
        suffix.insert(
            0,
            existing
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
        );
        existing = parent;
    }
    let canonical = std::fs::canonicalize(&existing).ok()?;
    let mut result = canonical;
    for component in suffix {
        result.push(component);
    }
    Some(result.display().to_string())
}

fn is_path_within_root(path: &str, root: &str) -> bool {
    path == root || path.starts_with(&format!("{root}{}", std::path::MAIN_SEPARATOR))
}

fn process_cwd() -> String {
    std::env::current_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_default()
}

/// `createSupervisorConsoleSnapshot`.
pub fn create_supervisor_console_snapshot(
    cwd: &str,
    generation: u64,
    session_id: Option<&str>,
    branch: Vec<SessionEntry>,
) -> ResidentConsoleSnapshot<SessionEntry> {
    ResidentConsoleSnapshot {
        service: ResidentConsoleService::Supervisor,
        session_id: session_id.unwrap_or(SUPERVISOR_SESSION_ID).to_owned(),
        cwd: cwd.to_owned(),
        generation,
        branch,
    }
}

/// `openSupervisorSession`.
pub fn open_supervisor_session(agent_dir: &str, kb_dir: &str, control_db_path: &str) -> Result<SessionManager, String> {
    let session_dir = Path::new(agent_dir).join("supervisor-sessions");
    std::fs::create_dir_all(&session_dir).map_err(|error| error.to_string())?;
    let session_dir_text = session_dir.display().to_string();
    let suffix = format!("_{SUPERVISOR_SESSION_ID}.jsonl");
    let mut candidates: Vec<String> = std::fs::read_dir(&session_dir)
        .map_err(|error| error.to_string())?
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.ends_with(&suffix).then_some(name)
        })
        .collect();
    candidates.sort();
    let session_path = candidates.last().map_or_else(
        || session_dir.join(format!("{SUPERVISOR_SESSION_ID}.jsonl")),
        |name| session_dir.join(name),
    );
    let mut session_manager = if session_path.exists() {
        SessionManager::open(
            &session_path.display().to_string(),
            Some(&session_dir_text),
            Some(kb_dir),
        )
    } else {
        SessionManager::create(
            kb_dir,
            Some(&session_dir_text),
            Some(crate::core::session_manager::NewSessionOptions {
                id: Some(SUPERVISOR_SESSION_ID.to_owned()),
                ..Default::default()
            }),
        )
    }
    .map_err(|error| error.to_string())?;
    session_manager
        .set_metadata_control_db_path(
            Some(control_db_path),
            crate::core::session_manager::SetMetadataControlDbPathOptions {
                index_message_text: Some(false),
                cwd_override: None,
            },
        )
        .map_err(|error| error.to_string())?;
    if let Some(persisted_path) = session_manager.get_session_file() {
        crate::core::session_control_db::archive_session(control_db_path, persisted_path)
            .map_err(|error| error.to_string())?;
    }
    Ok(session_manager)
}

/// Session surface needed by the request loop and request processing.
pub trait SupervisorSession: Send + Sync {
    fn abort(&self) -> BoxFuture<'static, ()>;
    fn prompt(&self, content: String) -> BoxFuture<'static, Result<(), String>>;
    /// `None` when the source session has no `compact` method.
    fn compact(&self, _instructions: &str) -> Option<BoxFuture<'static, Result<(), String>>> {
        None
    }
    /// `None` when the source session has no `getContextUsage` method.
    fn context_percent(&self) -> Option<f64> {
        None
    }
    fn session_id(&self) -> Option<String> {
        None
    }
    fn get_branch(&self) -> Vec<SessionEntry>;
    fn get_leaf_id(&self) -> Option<String>;
}

type ClaimNextRequest = Arc<dyn Fn(&str, &str) -> Result<Option<SupervisorRequest>, ControlDbError> + Send + Sync>;
type ProcessRequest =
    Arc<dyn Fn(String, SupervisorRequest, Arc<dyn SupervisorSession>) -> BoxFuture<'static, ()> + Send + Sync>;

/// Inputs for [`run_supervisor_request_loop`].
pub struct RunSupervisorRequestLoopInput {
    pub claim_token: String,
    pub console_prompts: Arc<Mutex<SupervisorConsolePromptQueue>>,
    pub control_db_path: String,
    pub session: Arc<dyn SupervisorSession>,
    pub signal: CancellationToken,
    pub wake_server: Arc<SupervisorRequestWakeServer>,
    pub claim_next_request: Option<ClaimNextRequest>,
    pub process_request: Option<ProcessRequest>,
}

/// `runSupervisorRequestLoop`.
pub async fn run_supervisor_request_loop(input: RunSupervisorRequestLoopInput) -> Result<(), ControlDbError> {
    recover_supervisor_requests(&input.control_db_path)?;
    let claim_request = input.claim_next_request.unwrap_or_else(|| {
        Arc::new(|control_db_path: &str, claim_token: &str| claim_next_supervisor_request(control_db_path, claim_token))
    });
    let handle_request = input.process_request.unwrap_or_else(|| {
        Arc::new(
            |control_db_path: String, request: SupervisorRequest, session: Arc<dyn SupervisorSession>| {
                Box::pin(async move {
                    process_supervisor_request(&control_db_path, request, session).await;
                }) as BoxFuture<'static, ()>
            },
        )
    });
    while !input.signal.is_cancelled() {
        let observed_generation = input.wake_server.current_generation();
        let request = claim_request(&input.control_db_path, &input.claim_token)?;
        if let Some(request) = request {
            handle_request(input.control_db_path.clone(), request, Arc::clone(&input.session)).await;
            continue;
        }
        let console_prompt = input
            .console_prompts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        if let Some(console_prompt) = console_prompt {
            let _ = input.session.prompt(console_prompt.text).await;
            continue;
        }
        input
            .wake_server
            .wait_for_wake_after(observed_generation, &input.signal)
            .await;
    }
    Ok(())
}

/// `processSupervisorRequest`.
pub async fn process_supervisor_request(
    control_db_path: &str,
    request: SupervisorRequest,
    session: Arc<dyn SupervisorSession>,
) {
    let evaluator = supervisor_evaluator(Arc::clone(&session));
    let result = run_supervisor_request(RunSupervisorRequestInput {
        control_db_path: control_db_path.to_owned(),
        evaluate: evaluator,
        request: request.clone(),
        poll_interval_ms: None,
    })
    .await;
    if let Err(error) = result
        && let Some(claim_token) = request.claim_token.as_deref()
    {
        let _ = complete_supervisor_request(
            control_db_path,
            request.id,
            claim_token,
            &SupervisorResponse::Error {
                reason: error.to_string(),
            },
        );
    }
}

fn supervisor_evaluator(session: Arc<dyn SupervisorSession>) -> SupervisorEvaluator {
    Arc::new(move |prompt: String, signal: CancellationToken| {
        let session = Arc::clone(&session);
        Box::pin(async move { evaluate_supervisor_prompt(session, prompt, signal).await })
            as BoxFuture<'static, Result<Value, String>>
    })
}

async fn evaluate_supervisor_prompt(
    session: Arc<dyn SupervisorSession>,
    prompt: String,
    signal: CancellationToken,
) -> Result<Value, String> {
    if signal.is_cancelled() {
        session.abort().await;
        return Err("Supervisor request aborted".to_owned());
    }
    let context_percent = session.context_percent();
    if let (Some(percent), Some(compact)) = (context_percent, session.compact(SUPERVISOR_COMPACTION_INSTRUCTIONS))
        && percent >= SUPERVISOR_COMPACTION_PERCENT
    {
        compact.await?;
        if let Some(session_id) = session.session_id() {
            cleanup_supervisor_provider_context(&session_id);
        }
    }
    if signal.is_cancelled() {
        return Err("Supervisor request aborted".to_owned());
    }
    let previous_leaf_id = session.get_leaf_id();
    tokio::select! {
        result = session.prompt(prompt) => result?,
        () = signal.cancelled() => {
            session.abort().await;
            return Err("Supervisor request aborted".to_owned());
        }
    }
    read_current_supervisor_response(&session.get_branch(), previous_leaf_id.as_deref())
}

/// `cleanupSupervisorProviderContext`.
pub fn cleanup_supervisor_provider_context(session_id: &str) {
    let _ = cleanup_session_resources(Some(session_id));
}

/// `readCurrentSupervisorResponse` over the current session branch.
pub fn read_current_supervisor_response(
    entries: &[SessionEntry],
    previous_leaf_id: Option<&str>,
) -> Result<Value, String> {
    let previous_leaf_index = match previous_leaf_id {
        Some(previous_leaf_id) => entries
            .iter()
            .position(|entry| entry.id() == Some(previous_leaf_id))
            .map_or(-1, |index| index as i64),
        None => -1,
    };
    if previous_leaf_id.is_some() && previous_leaf_index == -1 {
        return Err("Supervisor request boundary is missing from the current session branch".to_owned());
    }
    let current_messages: Vec<&AgentMessage> = entries
        .iter()
        .skip((previous_leaf_index + 1) as usize)
        .filter_map(|entry| entry.as_message().map(|message| message.message.as_ref()))
        .collect();
    let assistant_messages: Vec<&pi_ai::types::AssistantMessage> = current_messages
        .iter()
        .filter_map(|message| match message {
            AgentMessage::Assistant(assistant) => Some(assistant.as_ref()),
            _ => None,
        })
        .collect();
    let Some(terminal_message) = assistant_messages.last() else {
        return Err("Supervisor model returned no assistant text for current request".to_owned());
    };
    if let Some(structured) = read_structured_supervisor_response(terminal_message)? {
        return Ok(structured);
    }
    let response_message = if terminal_message_calls_end_turn(terminal_message) {
        assistant_messages
            .iter()
            .rev()
            .find(|message| message.stop_reason == StopReason::Stop)
            .copied()
            .unwrap_or(*terminal_message)
    } else {
        *terminal_message
    };
    if response_message.stop_reason != StopReason::Stop {
        return Err(format!(
            "Supervisor model request ended with {}",
            stop_reason_name(response_message.stop_reason)
        ));
    }
    let text: String = response_message
        .content
        .iter()
        .filter_map(|part| match part {
            AssistantContent::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect();
    if text.trim().is_empty() {
        return Err("Supervisor model returned no assistant text for current request".to_owned());
    }
    Ok(Value::String(text))
}

fn read_structured_supervisor_response(message: &pi_ai::types::AssistantMessage) -> Result<Option<Value>, String> {
    let Some(response_tool_call) = message.content.iter().find_map(|part| match part {
        AssistantContent::ToolCall(tool_call) if tool_call.name == SUPERVISOR_RESPONSE_TOOL_NAME => Some(tool_call),
        _ => None,
    }) else {
        return Ok(None);
    };
    let has_assistant_text = message.content.iter().any(|part| match part {
        AssistantContent::Text(text) => !text.text.trim().is_empty(),
        _ => false,
    });
    if has_assistant_text {
        return Err("Supervisor model emitted assistant text with structured response".to_owned());
    }
    Ok(Some(Value::Object(response_tool_call.arguments.clone())))
}

fn terminal_message_calls_end_turn(message: &pi_ai::types::AssistantMessage) -> bool {
    message.stop_reason == StopReason::ToolUse
        && message.content.iter().any(|part| match part {
            AssistantContent::ToolCall(tool_call) => tool_call.name == "end_turn",
            _ => false,
        })
}

fn stop_reason_name(stop_reason: StopReason) -> &'static str {
    match stop_reason {
        StopReason::Pending => "pending",
        StopReason::Stop => "stop",
        StopReason::Length => "length",
        StopReason::ToolUse => "toolUse",
        StopReason::Error => "error",
        StopReason::Aborted => "aborted",
        StopReason::Deferred => "deferred",
    }
}
