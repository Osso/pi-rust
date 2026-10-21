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
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::core::agent_session::{AgentSession, AgentSessionEvent, PromptOptions};
use crate::core::extensions::loader::ExtensionFactory;
use crate::core::extensions::types::{
    ExtensionContext, HandlerError, ToolCallEvent, ToolCallEventResult, ToolGateHandle,
};
use crate::core::model_runtime::{CreateModelRuntimeOptions, ModelRuntime};
use crate::core::resident_console_transport::{
    ResidentConsoleHost, ResidentConsoleServer, ResidentConsoleServerOptions, ResidentConsoleService,
    ResidentConsoleSnapshot, Unsubscribe,
};
use crate::core::resource_loader::{
    DefaultResourceLoader, DefaultResourceLoaderOptions, InlineExtension, ResourceLoader,
};
use crate::core::sdk::{CreateAgentSessionOptions, CreateAgentSessionResult, create_agent_session};
use crate::core::session_control_db::{
    ControlDbError, SupervisorRequest, SupervisorRequestKind, SupervisorRequestStatus, SupervisorResponse,
    claim_next_supervisor_request, complete_supervisor_request, read_supervisor_request, recover_supervisor_requests,
};
use crate::core::session_manager::{SessionEntry, SessionManager};
use crate::core::settings_manager::{Settings, SettingsManager, SettingsManagerCreateOptions};
use crate::supervisor::project_resolver::DEFAULT_SUPERVISOR_KB_DIR;
use crate::supervisor::request_wake::{SupervisorRequestWakeServer, notify_supervisor_request};
use crate::supervisor::response_tool::{SUPERVISOR_RESPONSE_TOOL_NAME, create_supervisor_response_tool};
use crate::supervisor::service::{
    RunSupervisorRequestInput, SupervisorEvaluator, SupervisorInstructionReviewDecision, parse_supervisor_response,
    run_supervisor_request, supervisor_instruction_rejection_feedback,
};
use crate::utils::paths::{process_cwd, resolve_path};

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

const SUPERVISOR_COMPACTION_INSTRUCTIONS: &str = "Preserve Supervisor decisions, project-specific policies, instruction-veto policy feedback, and reusable approval rationale.";

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
        identity: Some(crate::core::resident_console_transport::ResidentConsoleIdentity {
            version: crate::config::version().to_owned(),
            pid: std::process::id().into(),
            executable: std::env::current_exe()
                .expect("running executable path")
                .display()
                .to_string(),
            entrypoint: std::env::args().nth(1),
            instance_id: supervisor_instance_id().to_owned(),
            managed_by: if pi_js_compat::process_env::get(crate::supervisor::ensure_running::SUPERVISOR_AUTOSTART_ENV)
                .as_deref()
                == Some("1")
            {
                crate::core::resident_console_transport::ResidentManagedBy::Pi
            } else {
                crate::core::resident_console_transport::ResidentManagedBy::External
            },
            ready: true,
        }),
        branch,
    }
}

fn supervisor_instance_id() -> &'static str {
    static INSTANCE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    INSTANCE.get_or_init(pi_ai::utils::uuid::random_uuid)
}

/// `openSupervisorSession`.
pub fn open_supervisor_session(agent_dir: &str, kb_dir: &str, control_db_path: &str) -> Result<SessionManager, String> {
    let prepared = crate::core::resident_session::prepare_resident_session_file(
        agent_dir,
        crate::core::resident_session::ResidentRole::Supervisor,
        control_db_path,
    )
    .map_err(|error| error.to_string())?;
    let session_dir_text = prepared.session_dir;
    let session_path = PathBuf::from(prepared.session_path);
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
                index_message_text: None,
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
    fn abort(&self) -> BoxFuture<'static, Result<(), String>>;
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
    fn append_policy_feedback(&self, _content: String, _details: Value) -> BoxFuture<'static, Result<(), String>> {
        Box::pin(async { Ok(()) })
    }
    fn get_branch(&self) -> Vec<SessionEntry>;
    fn get_leaf_id(&self) -> Option<String>;
}

type ClaimNextRequest = Arc<dyn Fn(&str, &str) -> Result<Option<SupervisorRequest>, ControlDbError> + Send + Sync>;
type ProcessRequest = Arc<
    dyn Fn(String, SupervisorRequest, Arc<dyn SupervisorSession>) -> BoxFuture<'static, Result<(), ControlDbError>>
        + Send
        + Sync,
>;

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
                Box::pin(async move { process_supervisor_request(&control_db_path, request, session).await })
                    as BoxFuture<'static, Result<(), ControlDbError>>
            },
        )
    });
    while !input.signal.is_cancelled() {
        let observed_generation = input.wake_server.current_generation();
        let request = claim_request(&input.control_db_path, &input.claim_token)?;
        if let Some(request) = request {
            handle_request(input.control_db_path.clone(), request, Arc::clone(&input.session)).await?;
            continue;
        }
        let console_prompt = input
            .console_prompts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        if let Some(console_prompt) = console_prompt {
            input
                .session
                .prompt(console_prompt.text)
                .await
                .map_err(ControlDbError::new)?;
            continue;
        }
        input
            .wake_server
            .wait_for_wake_after(observed_generation, &input.signal)
            .await;
    }
    Ok(())
}

pub const GENERIC_GOAL_CONTINUATION: &str = "Continue working toward the active goal.";
pub type SupervisorInstructionReviewer = Arc<
    dyn Fn(String, CancellationToken) -> BoxFuture<'static, Result<Option<SupervisorInstructionReviewDecision>, String>>
        + Send
        + Sync,
>;

pub type SupervisorInstructionEvaluator =
    Arc<dyn Fn(String, CancellationToken) -> BoxFuture<'static, Result<Option<String>, String>> + Send + Sync>;

pub fn create_supervisor_instruction_reviewer(
    evaluate: SupervisorInstructionEvaluator,
) -> SupervisorInstructionReviewer {
    Arc::new(move |instructions, signal| {
        let evaluate = evaluate.clone();
        Box::pin(async move {
            if signal.is_cancelled() {
                return Ok(None);
            }
            let raw = evaluate(instructions, signal).await?;
            Ok(raw.and_then(|text| {
                crate::supervisor::service::parse_supervisor_instruction_review_decision(&Value::String(text))
            }))
        })
    })
}

pub async fn create_supervisor_instruction_evaluator(
    agent_dir: &str,
) -> Result<SupervisorInstructionEvaluator, String> {
    let runtime = ModelRuntime::create(CreateModelRuntimeOptions {
        auth_path: Some(Path::new(agent_dir).join("auth.json").display().to_string()),
        models_path: Some(Some(Path::new(agent_dir).join("models.json").display().to_string())),
        ..Default::default()
    })
    .await
    .map_err(|error| error.to_string())?;
    create_supervisor_instruction_evaluator_with_runtime(runtime)
}

pub fn create_supervisor_instruction_evaluator_with_runtime(
    runtime: ModelRuntime,
) -> Result<SupervisorInstructionEvaluator, String> {
    let models = require_supervisor_models(&runtime)?;
    Ok(Arc::new(move |instructions, signal| {
        let runtime = runtime.clone();
        let models = models.clone();
        Box::pin(async move {
            for model in models {
                if signal.is_cancelled() {
                    return Ok(None);
                }
                let message = evaluate_instruction_model(&runtime, model, &instructions, signal.clone()).await?;
                if signal.is_cancelled() {
                    return Ok(None);
                }
                if !is_supervisor_quota_exhaustion(&message) {
                    return Ok(read_instruction_review_text(&message));
                }
            }
            Ok(None)
        })
    }))
}

async fn evaluate_instruction_model(
    runtime: &ModelRuntime,
    model: pi_ai::types::Model,
    instructions: &str,
    signal: CancellationToken,
) -> Result<pi_ai::types::AssistantMessage, String> {
    use crate::core::model_registry::{ModelRegistry, ResolvedRequestAuth};
    use pi_ai::request::{ProviderRequestOptions, SimpleStreamOptions, StreamOptions};
    let registry = ModelRegistry::new(runtime.clone());
    let auth = registry.get_api_key_and_headers(&model).await;
    let ResolvedRequestAuth::Ok {
        api_key, headers, env, ..
    } = auth
    else {
        return Err(auth.error().unwrap_or_default().into());
    };
    let settings = create_supervisor_settings_manager();
    let retry = settings.get_provider_retry_settings();
    let idle = settings.get_http_idle_timeout_ms().map_err(|error| error.to_string())?;
    let timeout = if idle == 0 { 2_147_483_647.0 } else { idle as f64 };
    let headers =
        crate::core::provider_attribution::merge_provider_attribution_headers(&model, &settings, None, &[headers]);
    let context =
        crate::supervisor::service::build_supervisor_instruction_review_context(instructions, supervisor_now_millis());
    let context = serde_json::from_value(context).map_err(|error| error.to_string())?;
    let options = SimpleStreamOptions {
        stream: StreamOptions {
            request: ProviderRequestOptions {
                api_key,
                headers,
                env,
                signal: Some(signal),
                max_retries: retry.max_retries,
                max_retry_delay_ms: Some(retry.max_retry_delay_ms),
                timeout_ms: Some(retry.timeout_ms.unwrap_or(timeout)),
                ..Default::default()
            },
            websocket_connect_timeout_ms: settings
                .get_websocket_connect_timeout_ms()
                .map_err(|error| error.to_string())?
                .map(|timeout| timeout as f64),
            ..Default::default()
        },
        reasoning: Some(pi_ai::types::ThinkingLevel::Low),
        ..Default::default()
    };
    runtime
        .complete_simple(model, context, options)
        .await
        .map_err(|error| error.to_string())
}

fn supervisor_now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock after epoch")
        .as_millis() as i64
}
fn read_instruction_review_text(message: &pi_ai::types::AssistantMessage) -> Option<String> {
    if message.stop_reason != StopReason::Stop {
        return None;
    }
    let text: String = message
        .content
        .iter()
        .filter_map(|part| match part {
            AssistantContent::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect();
    (!text.is_empty()).then_some(text)
}
fn is_supervisor_quota_exhaustion(message: &pi_ai::types::AssistantMessage) -> bool {
    static PATTERN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let pattern=PATTERN.get_or_init(||regex::Regex::new(r"(?i)GoUsageLimitError|FreeUsageLimitError|usage limit|available balance|insufficient_quota|out of budget|quota exceeded|billing (?:limit|quota|exhausted)").expect("quota pattern"));
    message.stop_reason == StopReason::Error
        && message
            .error_message
            .as_deref()
            .is_some_and(|text| pattern.is_match(text))
}

pub async fn process_supervisor_request(
    control_db_path: &str,
    request: SupervisorRequest,
    session: Arc<dyn SupervisorSession>,
) -> Result<(), ControlDbError> {
    let reviewer: SupervisorInstructionReviewer = Arc::new(|_, _| Box::pin(async { Ok(None) }));
    process_supervisor_request_with_reviewer(control_db_path, request, session, reviewer).await
}

pub async fn process_supervisor_request_with_reviewer(
    control_db_path: &str,
    request: SupervisorRequest,
    session: Arc<dyn SupervisorSession>,
    reviewer: SupervisorInstructionReviewer,
) -> Result<(), ControlDbError> {
    let evaluation_request = request.clone();
    let evaluate: SupervisorEvaluator = Arc::new(move |prompt, signal| {
        let session = session.clone();
        let request = evaluation_request.clone();
        let reviewer = reviewer.clone();
        Box::pin(evaluate_supervisor_request(session, request, reviewer, prompt, signal))
    });
    let result = run_supervisor_request(RunSupervisorRequestInput {
        control_db_path: control_db_path.into(),
        evaluate,
        request: request.clone(),
        poll_interval_ms: None,
    })
    .await;
    if let Err(error) = result {
        if read_supervisor_request(control_db_path, request.id)?
            .is_some_and(|row| row.status == SupervisorRequestStatus::Cancelled)
        {
            return Ok(());
        }
        let token = request
            .claim_token
            .as_deref()
            .filter(|token| !token.is_empty())
            .ok_or_else(|| ControlDbError::new(format!("Supervisor request {} has no claim token", request.id)))?;
        let completion = complete_supervisor_request(
            control_db_path,
            request.id,
            token,
            &SupervisorResponse::Error {
                reason: error.to_string(),
            },
        );
        if let Err(error) = completion {
            if read_supervisor_request(control_db_path, request.id)?
                .is_some_and(|row| row.status == SupervisorRequestStatus::Cancelled)
            {
                return Ok(());
            }
            return Err(error);
        }
    }
    Ok(())
}

async fn apply_supervisor_instruction_veto(
    request: &SupervisorRequest,
    raw: Value,
    session: Arc<dyn SupervisorSession>,
    reviewer: SupervisorInstructionReviewer,
    signal: CancellationToken,
) -> Result<Value, String> {
    if !matches!(
        request.kind,
        SupervisorRequestKind::GoalIdleReview | SupervisorRequestKind::GoalCompletionReview
    ) {
        return Ok(raw);
    }
    let Some(response @ SupervisorResponse::Continue { .. }) = parse_supervisor_response(request.kind, &raw) else {
        return Ok(raw);
    };
    let SupervisorResponse::Continue { instructions, reason } = response else {
        unreachable!()
    };
    if instructions == GENERIC_GOAL_CONTINUATION {
        return Ok(raw);
    }
    let decision = (reviewer)(instructions.clone(), signal.clone()).await;
    if signal.is_cancelled() {
        return Err("Supervisor instruction review aborted".into());
    }
    let (feedback, details) = match decision {
        Ok(Some(SupervisorInstructionReviewDecision::Accept)) => {
            return serde_json::to_value(SupervisorResponse::Continue { instructions, reason })
                .map_err(|error| error.to_string());
        }
        Ok(Some(rejection)) => (
            supervisor_instruction_rejection_feedback(rejection).to_owned(),
            json!({"reason":rejection}),
        ),
        Ok(None) => (
            "Policy gate failure: invalid_response. Non-generic instructions were suppressed.".into(),
            json!({"failure":"invalid_response"}),
        ),
        Err(_) => (
            "Policy gate failure: evaluation_error. Non-generic instructions were suppressed.".into(),
            json!({"failure":"evaluation_error"}),
        ),
    };
    session.append_policy_feedback(feedback, details).await?;
    serde_json::to_value(SupervisorResponse::Continue {
        instructions: GENERIC_GOAL_CONTINUATION.into(),
        reason,
    })
    .map_err(|error| error.to_string())
}

async fn evaluate_supervisor_request(
    session: Arc<dyn SupervisorSession>,
    request: SupervisorRequest,
    reviewer: SupervisorInstructionReviewer,
    prompt: String,
    signal: CancellationToken,
) -> Result<Value, String> {
    if signal.is_cancelled() {
        session.abort().await?;
        return Err("Supervisor request aborted".to_owned());
    }
    let evaluation = async {
        let raw = evaluate_supervisor_prompt_workflow(session.clone(), prompt, signal.clone()).await?;
        apply_supervisor_instruction_veto(&request, raw, session.clone(), reviewer, signal.clone()).await
    };
    tokio::pin!(evaluation);
    tokio::select! {
        biased;
        ()=signal.cancelled()=>{session.abort().await?;evaluation.await},
        result=&mut evaluation=>result,
    }
}

async fn evaluate_supervisor_prompt_workflow(
    session: Arc<dyn SupervisorSession>,
    prompt: String,
    signal: CancellationToken,
) -> Result<Value, String> {
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
    session.prompt(prompt).await?;
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

/// `createSupervisorResourceLoader`.
pub async fn create_supervisor_resource_loader(
    agent_dir: &str,
    kb_dir: &str,
    settings_manager: Arc<Mutex<SettingsManager>>,
) -> Result<Arc<DefaultResourceLoader>, String> {
    let gate_kb_dir = kb_dir.to_owned();
    let gate: ExtensionFactory = Arc::new(move |api| {
        let gate_kb_dir = gate_kb_dir.clone();
        let gate: ToolGateHandle = Arc::new(move |event: ToolCallEvent, _context: ExtensionContext| {
            let gate_kb_dir = gate_kb_dir.clone();
            Box::pin(async move {
                let input = Value::Object(event.input.clone());
                Ok(
                    block_supervisor_file_access(&gate_kb_dir, event.tool_name.as_str(), &input).map(|result| {
                        ToolCallEventResult {
                            block: result.block,
                            input: None,
                            reason: result.reason,
                            terminate: None,
                        }
                    }),
                )
            }) as BoxFuture<'static, Result<Option<ToolCallEventResult>, HandlerError>>
        });
        let result = api.register_tool_gate(gate);
        Box::pin(async move { result.map_err(HandlerError::new) })
    });
    let loader = DefaultResourceLoader::new(DefaultResourceLoaderOptions {
        cwd: kb_dir.to_owned(),
        agent_dir: agent_dir.to_owned(),
        extension_factories: vec![InlineExtension::Factory {
            extension_path: None,
            factory: gate,
        }],
        no_context_files: true,
        no_extensions: true,
        no_prompt_templates: true,
        no_skills: true,
        no_themes: true,
        settings_manager: Some(settings_manager),
        ..Default::default()
    })?;
    loader.reload().await?;
    let extensions = loader.get_extensions();
    let errors: Vec<(String, String)> = extensions
        .errors
        .iter()
        .map(|error| (error.path.clone(), error.error.clone()))
        .collect();
    validate_supervisor_extension_load(&errors)?;
    Ok(Arc::new(loader))
}

/// Options for [`create_supervisor_session`].
pub struct CreateSupervisorSessionOptions {
    pub agent_dir: String,
    pub kb_dir: String,
    pub model_runtime: ModelRuntime,
    pub resource_loader: Arc<dyn ResourceLoader>,
    pub session_manager: SessionManager,
    pub settings_manager: Arc<Mutex<SettingsManager>>,
}

const SUPERVISOR_MODEL_ID: &str = "gpt-6.1-sol";
const SUPERVISOR_PROVIDERS: [&str; 3] = ["openai-codex", "openai-codex-gc", "openai-codex-team"];

fn require_supervisor_models(runtime: &ModelRuntime) -> Result<Vec<pi_ai::types::Model>, String> {
    let models: Vec<_> = SUPERVISOR_PROVIDERS
        .iter()
        .filter(|provider| runtime.has_configured_auth(provider))
        .filter_map(|provider| runtime.get_model(provider, SUPERVISOR_MODEL_ID))
        .collect();
    if models.is_empty() {
        return Err(format!(
            "Pi Supervisor requires {SUPERVISOR_MODEL_ID} on an authenticated provider: {}",
            SUPERVISOR_PROVIDERS.join(", ")
        ));
    }
    Ok(models)
}

/// `createSupervisorSession`.
pub async fn create_supervisor_session(
    options: CreateSupervisorSessionOptions,
) -> Result<CreateAgentSessionResult, String> {
    let model = require_supervisor_models(&options.model_runtime)?.remove(0);
    let mut tools: Vec<String> = SUPERVISOR_TOOL_NAMES.iter().map(|name| (*name).to_owned()).collect();
    tools.push(SUPERVISOR_RESPONSE_TOOL_NAME.to_owned());
    create_agent_session(CreateAgentSessionOptions {
        agent_dir: Some(options.agent_dir),
        cwd: Some(options.kb_dir),
        custom_tool_definitions: vec![create_supervisor_response_tool()],
        disable_runtime_coordination_inbound: true,
        exclude_tools: Some(
            SUPERVISOR_EXCLUDED_TOOL_NAMES
                .iter()
                .map(|name| (*name).to_owned())
                .collect(),
        ),
        model: Some(model),
        model_runtime: Some(options.model_runtime),
        resource_loader: Some(options.resource_loader),
        session_manager: Some(options.session_manager),
        settings_manager: Some(options.settings_manager),
        thinking_level: Some(pi_agent_core::harness::types::ThinkingLevel::Low),
        tools: Some(tools),
        ..Default::default()
    })
    .await
}

impl SupervisorSession for AgentSession {
    fn append_policy_feedback(&self, content: String, details: Value) -> BoxFuture<'static, Result<(), String>> {
        let session = self.clone();
        Box::pin(async move {
            session
                .send_custom_message(
                    "supervisor_policy_feedback",
                    pi_agent_core::harness::messages::CustomMessageContent::Text(content),
                    false,
                    Some(details),
                    false,
                    None,
                )
                .await
                .map_err(|error| error.message().into())
        })
    }

    fn abort(&self) -> BoxFuture<'static, Result<(), String>> {
        let session = self.clone();
        Box::pin(async move { session.abort().await.map_err(|error| error.message().to_owned()) })
    }

    fn prompt(&self, content: String) -> BoxFuture<'static, Result<(), String>> {
        let session = self.clone();
        Box::pin(async move {
            session
                .prompt(&content, PromptOptions::default())
                .await
                .map_err(|error| error.message().to_owned())
        })
    }

    fn compact(&self, instructions: &str) -> Option<BoxFuture<'static, Result<(), String>>> {
        let session = self.clone();
        let instructions = instructions.to_owned();
        Some(Box::pin(async move {
            session
                .compact(Some(&instructions))
                .await
                .map(|_| ())
                .map_err(|error| error.message().to_owned())
        }))
    }

    fn context_percent(&self) -> Option<f64> {
        self.get_context_usage().map(|usage| usage.percent)
    }

    fn session_id(&self) -> Option<String> {
        Some(AgentSession::session_id(self))
    }

    fn get_branch(&self) -> Vec<SessionEntry> {
        self.session_manager().get_branch(None)
    }

    fn get_leaf_id(&self) -> Option<String> {
        self.session_manager().get_leaf_id().map(str::to_owned)
    }
}

struct SupervisorConsoleHost {
    session: AgentSession,
    cwd: String,
    generation: u64,
    prompts: Arc<Mutex<SupervisorConsolePromptQueue>>,
    control_db_path: String,
}

impl ResidentConsoleHost<SessionEntry, AgentSessionEvent> for SupervisorConsoleHost {
    fn get_snapshot(&self) -> ResidentConsoleSnapshot<SessionEntry> {
        create_supervisor_console_snapshot(
            &self.cwd,
            self.generation,
            Some(&AgentSession::session_id(&self.session)),
            self.session.session_manager().get_branch(None),
        )
    }

    fn enqueue_prompt(&self, text: String, id: String) -> pi_ai::request::BoxFuture<'static, Result<(), String>> {
        self.prompts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .enqueue(text, id);
        notify_supervisor_request(&self.control_db_path);
        Box::pin(async { Ok(()) })
    }

    fn subscribe(&self, listener: Arc<dyn Fn(AgentSessionEvent) + Send + Sync>) -> Unsubscribe {
        let id = self.session.subscribe(Arc::new(move |event: &AgentSessionEvent| {
            listener(event.clone());
        }));
        let session = self.session.clone();
        Box::new(move || session.unsubscribe(id))
    }
}

/// `runSupervisorService`.
pub async fn run_supervisor_service() -> Result<(), String> {
    let agent_dir = crate::config::get_agent_dir().display().to_string();
    let kb_dir = pi_js_compat::process_env::get("PI_KB_DIR").unwrap_or_else(|| DEFAULT_SUPERVISOR_KB_DIR.to_owned());
    let control_db_path = crate::core::session_control_db::get_control_db_path(&crate::config::get_user_state_root());
    let session_manager = open_supervisor_session(&agent_dir, &kb_dir, &control_db_path)?;
    let model_runtime = ModelRuntime::create(CreateModelRuntimeOptions {
        auth_path: Some(Path::new(&agent_dir).join("auth.json").display().to_string()),
        models_path: Some(Some(Path::new(&agent_dir).join("models.json").display().to_string())),
        ..Default::default()
    })
    .await
    .map_err(|error| error.to_string())?;
    let review_instructions = create_supervisor_instruction_reviewer(
        create_supervisor_instruction_evaluator_with_runtime(model_runtime.clone())?,
    );
    let settings_manager = Arc::new(Mutex::new(create_supervisor_settings_manager()));
    let resource_loader = create_supervisor_resource_loader(&agent_dir, &kb_dir, Arc::clone(&settings_manager)).await?;
    let session = create_supervisor_session(CreateSupervisorSessionOptions {
        agent_dir,
        kb_dir: kb_dir.clone(),
        model_runtime,
        resource_loader,
        session_manager,
        settings_manager,
    })
    .await?
    .session;
    let wake_server = Arc::new(SupervisorRequestWakeServer::new(&control_db_path));
    let console_prompts = Arc::new(Mutex::new(SupervisorConsolePromptQueue::new()));
    let console_server = Arc::new(ResidentConsoleServer::new(ResidentConsoleServerOptions {
        socket_path: PathBuf::from(format!("{control_db_path}.supervisor-console.sock")),
        service: ResidentConsoleService::Supervisor,
        host: Arc::new(SupervisorConsoleHost {
            session: session.clone(),
            cwd: kb_dir,
            generation: std::process::id() as u64,
            prompts: Arc::clone(&console_prompts),
            control_db_path: control_db_path.clone(),
        }),
    }));
    wake_server.start().await?;
    console_server.start().await?;
    let signal = CancellationToken::new();
    install_stop_signals(&signal);
    let loop_result = run_supervisor_request_loop(RunSupervisorRequestLoopInput {
        claim_token: pi_ai::utils::uuid::random_uuid(),
        console_prompts,
        control_db_path,
        session: Arc::new(session.clone()),
        signal,
        wake_server: Arc::clone(&wake_server),
        claim_next_request: None,
        process_request: Some(Arc::new(move |path, request, session| {
            let reviewer = review_instructions.clone();
            Box::pin(async move { process_supervisor_request_with_reviewer(&path, request, session, reviewer).await })
        })),
    })
    .await;
    console_server.close().await?;
    wake_server.close().await?;
    session.abort().await.map_err(|error| error.message().to_owned())?;
    loop_result.map_err(|error| error.to_string())
}

fn install_stop_signals(signal: &CancellationToken) {
    let signal = signal.clone();
    tokio::spawn(async move {
        let mut interrupt = match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt()) {
            Ok(signal) => signal,
            Err(_) => return,
        };
        let mut terminate = match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(signal) => signal,
            Err(_) => return,
        };
        tokio::select! {
            _ = interrupt.recv() => {}
            _ = terminate.recv() => {}
        }
        signal.cancel();
    });
}
