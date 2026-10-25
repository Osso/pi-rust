//! Port of `packages/coding-agent/src/supervisor/main.ts`.
//!
//! Supervisor sessions, stateless instruction review, durable requests, and resident console runtime.

use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use crate::core::codex_quota_fallback::{CODEX_FALLBACK_PROVIDERS, is_quota_exhaustion_error};
use pi_agent_core::agent::AgentMessage;
use pi_agent_core::harness::types::BeforeToolCallResult;
use pi_ai::request::{BoxFuture, ProviderRequestOptions, SimpleStreamOptions, StreamOptions, now_millis};
use pi_ai::session_resources::cleanup_session_resources;
use pi_ai::types::{AssistantContent, Model, StopReason};
use pi_js_compat::js_json::{JsString, JsValue};

use pi_js_compat::abort::AbortSignal;

use crate::core::agent_session::{AgentSession, AgentSessionEvent, PromptOptions};
use crate::core::extensions::loader::ExtensionFactory;
use crate::core::extensions::types::{
    CustomMessageContent, CustomMessagePayload, ExtensionContext, HandlerError, ToolCallEvent, ToolCallEventResult,
    ToolGateHandle,
};
use crate::core::model_registry::{ModelRegistry, ResolvedRequestAuth};
use crate::core::provider_attribution::merge_provider_attribution_headers;
use crate::core::resident_console_transport::{
    ResidentConsoleHost, ResidentConsoleIdentity, ResidentConsoleManagedBy, ResidentConsoleServer,
    ResidentConsoleServerOptions, ResidentConsoleService, ResidentConsoleSnapshot, Unsubscribe,
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
    RunSupervisorRequestInput, SupervisorEvaluator, SupervisorInstructionReviewDecision,
    build_supervisor_instruction_review_context, parse_supervisor_instruction_review_decision,
    parse_supervisor_response, run_supervisor_request, supervisor_instruction_rejection_feedback,
};
use crate::utils::paths::{node_basename, node_dirname, node_join, node_resolve};
use pi_js_compat::js_format;
use pi_js_compat::js_json::encode_node_utf8;

pub const SUPERVISOR_SESSION_ID: &str = "supervisor";
pub const SUPERVISOR_COMPACTION_PERCENT: f64 = 75.0;
pub const GENERIC_GOAL_CONTINUATION: &str = "Continue working toward the active goal.";
pub type SupervisorInstructionEvaluator =
    Arc<dyn Fn(JsString, AbortSignal) -> BoxFuture<'static, Result<Option<JsValue>, JsString>> + Send + Sync>;
pub type SupervisorInstructionReviewer = Arc<
    dyn Fn(JsString, AbortSignal) -> BoxFuture<'static, Result<Option<SupervisorInstructionReviewDecision>, JsString>>
        + Send
        + Sync,
>;

pub fn create_supervisor_instruction_reviewer(
    evaluate: SupervisorInstructionEvaluator,
) -> SupervisorInstructionReviewer {
    Arc::new(move |instructions, signal| {
        if signal.aborted() {
            return Box::pin(async { Ok(None) });
        }
        let evaluation = evaluate(instructions, signal);
        Box::pin(async move {
            Ok(evaluation
                .await?
                .as_ref()
                .and_then(parse_supervisor_instruction_review_decision))
        })
    })
}
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

const SUPERVISOR_MODEL_ID: &str = "gpt-6.1-sol";

fn require_supervisor_models(registry: &ModelRegistry) -> Result<Vec<Model>, JsString> {
    let fallbacks = CODEX_FALLBACK_PROVIDERS
        .iter()
        .find_map(|(provider, fallback)| (*provider == "openai-codex").then_some(fallback.as_slice()))
        .unwrap_or_default();
    let providers = std::iter::once("openai-codex")
        .chain(fallbacks.iter().copied())
        .collect::<Vec<_>>();
    let mut models = Vec::new();
    for provider in &providers {
        if let Some(model) = registry.find(*provider, SUPERVISOR_MODEL_ID)
            && registry.has_configured_auth(&model).map_err(|error| error.message)?
        {
            models.push(model);
        }
    }
    if models.is_empty() {
        return Err(js_format!(
            "Pi Supervisor requires {} on an authenticated provider: {}",
            SUPERVISOR_MODEL_ID,
            providers.join(", ")
        ));
    }
    Ok(models)
}

pub fn create_supervisor_instruction_evaluator(
    agent_dir: &JsString,
) -> Result<SupervisorInstructionEvaluator, JsString> {
    let auth = crate::core::auth_storage::AuthStorage::create(Some(&node_join(agent_dir, "auth.json")))
        .map_err(|error| error.message().clone())?;
    let registry =
        ModelRegistry::create(auth, Some(node_join(agent_dir, "models.json"))).map_err(|error| error.message)?;
    let models = require_supervisor_models(&registry)?;
    let settings = Arc::new(Mutex::new(create_supervisor_settings_manager()));
    Ok(Arc::new(move |instructions, signal| {
        let registry = registry.clone();
        let models = models.clone();
        let settings = settings.clone();
        Box::pin(async move {
            for model in models {
                if signal.aborted() {
                    return Ok(None);
                }
                let response = evaluate_supervisor_instruction_with_model(
                    model,
                    &registry,
                    &settings,
                    &instructions,
                    signal.clone(),
                )
                .await?;
                if signal.aborted() {
                    return Ok(None);
                }
                if !is_quota_exhaustion_error(&response) {
                    return Ok(read_supervisor_instruction_review_text(&response).map(JsValue::String));
                }
            }
            Ok(None)
        })
    }))
}

async fn evaluate_supervisor_instruction_with_model(
    model: Model,
    registry: &ModelRegistry,
    settings: &Mutex<SettingsManager>,
    instructions: &JsString,
    signal: AbortSignal,
) -> Result<pi_ai::types::AssistantMessage, JsString> {
    let auth = registry
        .get_api_key_and_headers(&model)
        .await
        .map_err(|error| error.message)?;
    let options = supervisor_instruction_stream_options(
        &model,
        &settings.lock().unwrap_or_else(std::sync::PoisonError::into_inner),
        auth,
        signal,
    )?;
    let context = build_supervisor_instruction_review_context(instructions, now_millis() as f64);
    pi_ai::compat::stream_simple(model, context, options)
        .map_err(|error| error.message)?
        .result()
        .await
        .map_err(|error| error.message().clone())
}

fn supervisor_instruction_stream_options(
    model: &Model,
    settings: &SettingsManager,
    auth: ResolvedRequestAuth,
    signal: AbortSignal,
) -> Result<SimpleStreamOptions, JsString> {
    let (api_key, headers, env) = match auth {
        ResolvedRequestAuth::Ok { api_key, headers, env } => (api_key, headers, env),
        ResolvedRequestAuth::Err { error } => return Err(error),
    };
    let retry = settings.get_provider_retry_settings();
    let idle_timeout = settings
        .get_http_idle_timeout_ms()
        .map_err(|error| error.message().clone())?;
    let effective_idle_timeout = if idle_timeout == 0 {
        i32::MAX as f64
    } else {
        idle_timeout as f64
    };
    Ok(SimpleStreamOptions {
        reasoning: Some(pi_ai::types::ThinkingLevel::Low),
        stream: StreamOptions {
            request: ProviderRequestOptions {
                api_key,
                env,
                headers: merge_provider_attribution_headers(model, settings, None, &[headers]),
                max_retries: retry.max_retries,
                max_retry_delay_ms: Some(retry.max_retry_delay_ms),
                signal: Some(signal),
                timeout_ms: Some(retry.timeout_ms.unwrap_or(effective_idle_timeout)),
                ..Default::default()
            },
            websocket_connect_timeout_ms: settings
                .get_websocket_connect_timeout_ms()
                .map_err(|error| error.message().clone())?
                .map(|timeout| timeout as f64),
            ..Default::default()
        },
        ..Default::default()
    })
}

fn read_supervisor_instruction_review_text(message: &pi_ai::types::AssistantMessage) -> Option<JsString> {
    if message.stop_reason != StopReason::Stop {
        return None;
    }
    let text = JsString::join(
        message.content.iter().filter_map(|part| match part {
            AssistantContent::Text(text) => Some(&text.text),
            _ => None,
        }),
        "",
    );
    (!text.is_empty()).then_some(text)
}

const SUPERVISOR_COMPACTION_INSTRUCTIONS: &str = "Preserve Supervisor decisions, project-specific policies, instruction-veto policy feedback, and reusable approval rationale.";

/// One queued console prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupervisorConsolePrompt {
    pub id: JsString,
    pub text: JsString,
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

    pub fn enqueue(&mut self, text: JsString, id: JsString) {
        self.prompts.push_back(SupervisorConsolePrompt { id, text });
    }

    pub fn take(&mut self) -> Option<SupervisorConsolePrompt> {
        self.prompts.pop_front()
    }
}

/// `createSupervisorSettingsManager`.
pub fn create_supervisor_settings_manager() -> SettingsManager {
    let mut settings = Settings::new();
    settings.insert("approvalPolicy".into(), JsValue::String("auto-approve".into()));
    settings.insert("approvalPreset".into(), JsValue::String("auto-approve".into()));
    settings.insert("sandboxProfile".into(), JsValue::String("full-access".into()));
    SettingsManager::in_memory(settings, SettingsManagerCreateOptions { project_trusted: None })
}

/// `validateSupervisorExtensionLoad` over the diagnostic `path: error` pairs.
pub fn validate_supervisor_extension_load(errors: &[(JsString, JsString)]) -> Result<(), JsString> {
    if errors.is_empty() {
        return Ok(());
    }
    let details = JsString::join(
        errors.iter().map(|(path, error)| js_format!("{}: {}", path, error)),
        "; ",
    );
    Err(js_format!("Supervisor extension load failed: {}", details))
}

/// `blockSupervisorFileAccess`.
pub fn block_supervisor_file_access(
    kb_dir: impl Into<JsString>,
    tool_name: &str,
    input: &JsValue,
) -> Option<BeforeToolCallResult> {
    let kb_dir = kb_dir.into();
    if !SUPERVISOR_TOOL_NAMES.contains(&tool_name) {
        return None;
    }
    let Some(path) = read_tool_path(input).filter(|path| !path.is_empty()) else {
        return Some(supervisor_file_access_block());
    };
    let resolved = if tool_name == "read" {
        crate::core::tools::path_utils::resolve_read_path(path, &kb_dir)
    } else {
        crate::core::tools::path_utils::resolve_to_cwd(path, &kb_dir)
    };
    let Ok(resolved_path) = resolved else {
        return Some(supervisor_file_access_block());
    };
    let within = resolve_existing_path(&resolved_path)
        .zip(resolve_existing_path(&kb_dir))
        .is_some_and(|(path, root)| is_path_within_root(&path, &root));
    if within {
        None
    } else {
        Some(supervisor_file_access_block())
    }
}

fn read_tool_path(input: &JsValue) -> Option<&JsString> {
    input.get("path").and_then(JsValue::as_string)
}

fn supervisor_file_access_block() -> BeforeToolCallResult {
    BeforeToolCallResult {
        block: Some(true),
        reason: Some(
            "Pi Supervisor may access files only inside the configured KB directory."
                .to_owned()
                .into(),
        ),
    }
}

fn resolve_existing_path(path: &JsString) -> Option<JsString> {
    let resolved = node_resolve("", path).ok()?;
    let mut suffix = Vec::new();
    let mut existing = resolved.clone();
    while !Path::new(&encode_node_utf8(&existing)).exists() {
        let parent = node_dirname(&existing);
        if parent == existing {
            return Some(resolved);
        }
        suffix.push(node_basename(&existing));
        existing = parent;
    }
    let canonical = std::fs::canonicalize(encode_node_utf8(&existing)).ok()?;
    let mut result = pi_js_compat::node_fs::decode_node_path(canonical.as_os_str());
    for component in suffix.into_iter().rev() {
        result = node_join(result, component);
    }
    Some(result)
}

fn is_path_within_root(path: &JsString, root: &JsString) -> bool {
    path == root || path.starts_with(js_format!("{}{}", root, std::path::MAIN_SEPARATOR))
}

/// `createSupervisorConsoleSnapshot`.
pub fn create_supervisor_console_snapshot(
    cwd: &JsString,
    generation: u64,
    managed_by: ResidentConsoleManagedBy,
    session_id: Option<&JsString>,
    branch: Vec<SessionEntry>,
) -> ResidentConsoleSnapshot<SessionEntry> {
    static INSTANCE_ID: OnceLock<JsString> = OnceLock::new();
    let instance_id = INSTANCE_ID.get_or_init(|| {
        pi_ai::utils::random_uuid::random_uuid()
            .unwrap_or_else(|error| crate::utils::uncaught::throw_uncaught("Error", error.message))
            .into()
    });
    ResidentConsoleSnapshot {
        service: ResidentConsoleService::Supervisor,
        session_id: session_id.cloned().unwrap_or_else(|| SUPERVISOR_SESSION_ID.into()),
        cwd: cwd.to_owned(),
        generation,
        identity: Some(ResidentConsoleIdentity::for_compiled_process(
            instance_id.clone(),
            managed_by,
        )),
        branch,
    }
}

/// `openSupervisorSession`.
pub fn open_supervisor_session(
    agent_dir: &JsString,
    kb_dir: &JsString,
    control_db_path: &JsString,
) -> Result<SessionManager, JsString> {
    let prepared = crate::core::resident_session::prepare_resident_session_file(
        agent_dir,
        crate::core::resident_session::ResidentRole::Supervisor,
        control_db_path,
    )
    .map_err(|error| error.message())?;
    let session_dir = prepared.session_dir;
    let session_path = prepared.session_path;
    let mut session_manager = if Path::new(&encode_node_utf8(&session_path)).exists() {
        SessionManager::open(&session_path, Some(&session_dir), Some(kb_dir))
    } else {
        SessionManager::create(
            kb_dir,
            Some(&session_dir),
            Some(crate::core::session_manager::NewSessionOptions {
                id: Some(SUPERVISOR_SESSION_ID.into()),
                ..Default::default()
            }),
        )
    }
    .map_err(|error| error.message().clone())?;
    session_manager
        .set_metadata_control_db_path(
            Some(control_db_path),
            crate::core::session_manager::SetMetadataControlDbPathOptions::default(),
        )
        .map_err(|error| error.message().clone())?;
    if let Some(persisted_path) = session_manager.get_session_file() {
        crate::core::session_control_db::archive_session(control_db_path, persisted_path)
            .map_err(|error| error.message().clone())?;
    }
    Ok(session_manager)
}

/// Session surface needed by the request loop and request processing.
pub trait SupervisorSession: Send + Sync {
    fn abort(&self) -> BoxFuture<'static, Result<(), JsString>>;
    fn prompt(&self, content: JsString) -> BoxFuture<'static, Result<(), JsString>>;
    /// `None` when the source session has no `compact` method.
    fn compact(&self, _instructions: &JsString) -> Option<BoxFuture<'static, Result<(), JsString>>> {
        None
    }
    /// `None` when the source session has no `getContextUsage` method.
    fn context_percent(&self) -> Option<f64> {
        None
    }
    fn session_id(&self) -> Option<JsString> {
        None
    }
    fn get_branch(&self) -> Vec<SessionEntry>;
    fn get_leaf_id(&self) -> Option<JsString>;
    fn send_custom_message(&self, _message: CustomMessagePayload) -> Option<BoxFuture<'static, Result<(), JsString>>> {
        None
    }
}

type ClaimNextRequest =
    Arc<dyn Fn(&JsString, &JsString) -> Result<Option<SupervisorRequest>, ControlDbError> + Send + Sync>;
type ProcessRequest = Arc<
    dyn Fn(JsString, SupervisorRequest, Arc<dyn SupervisorSession>) -> BoxFuture<'static, Result<(), ControlDbError>>
        + Send
        + Sync,
>;

/// Inputs for [`run_supervisor_request_loop`].
pub struct RunSupervisorRequestLoopInput {
    pub claim_token: JsString,
    pub console_prompts: Arc<Mutex<SupervisorConsolePromptQueue>>,
    pub control_db_path: JsString,
    pub session: Arc<dyn SupervisorSession>,
    pub signal: AbortSignal,
    pub wake_server: Arc<SupervisorRequestWakeServer>,
    pub claim_next_request: Option<ClaimNextRequest>,
    pub process_request: Option<ProcessRequest>,
}

/// `runSupervisorRequestLoop`.
pub async fn run_supervisor_request_loop(input: RunSupervisorRequestLoopInput) -> Result<(), ControlDbError> {
    recover_supervisor_requests(&input.control_db_path)?;
    let claim_request = input.claim_next_request.unwrap_or_else(|| {
        Arc::new(|control_db_path: &JsString, claim_token: &JsString| {
            claim_next_supervisor_request(control_db_path, claim_token)
        })
    });
    let handle_request = input.process_request.unwrap_or_else(|| {
        Arc::new(
            |control_db_path: JsString, request: SupervisorRequest, session: Arc<dyn SupervisorSession>| {
                Box::pin(async move { process_supervisor_request(&control_db_path, request, session, None).await })
                    as BoxFuture<'static, Result<(), ControlDbError>>
            },
        )
    });
    while !input.signal.aborted() {
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

/// `processSupervisorRequest`.
pub async fn process_supervisor_request(
    control_db_path: &JsString,
    request: SupervisorRequest,
    session: Arc<dyn SupervisorSession>,
    review: Option<SupervisorInstructionReviewer>,
) -> Result<(), ControlDbError> {
    let review = review.unwrap_or_else(|| Arc::new(|_, _| Box::pin(async { Ok(None) })));
    let evaluator = supervisor_evaluator(Arc::clone(&session), request.clone(), review);
    let result = run_supervisor_request(RunSupervisorRequestInput {
        control_db_path: control_db_path.to_owned(),
        evaluate: evaluator,
        request: request.clone(),
        poll_interval_ms: None,
    })
    .await;
    if let Err(error) = result {
        complete_supervisor_error_unless_cancelled(control_db_path, &request, error)?;
    }
    Ok(())
}

fn complete_supervisor_error_unless_cancelled(
    control_db_path: &JsString,
    request: &SupervisorRequest,
    error: ControlDbError,
) -> Result<(), ControlDbError> {
    if read_supervisor_request_is_cancelled(control_db_path, request.id)? {
        return Ok(());
    }
    let completion = required_claim_token(request).and_then(|token| {
        complete_supervisor_request(
            control_db_path,
            request.id,
            token,
            &SupervisorResponse::Error {
                reason: error.message().clone(),
            },
        )
    });
    if let Err(error) = completion {
        if read_supervisor_request_is_cancelled(control_db_path, request.id)? {
            return Ok(());
        }
        return Err(error);
    }
    Ok(())
}

fn read_supervisor_request_is_cancelled(control_db_path: &JsString, id: i64) -> Result<bool, ControlDbError> {
    Ok(read_supervisor_request(control_db_path, id)?
        .is_some_and(|request| request.status == SupervisorRequestStatus::Cancelled))
}

fn required_claim_token(request: &SupervisorRequest) -> Result<&JsString, ControlDbError> {
    request
        .claim_token
        .as_ref()
        .filter(|token| !token.is_empty())
        .ok_or_else(|| ControlDbError::new(format!("Supervisor request {} has no claim token", request.id)))
}

fn supervisor_evaluator(
    session: Arc<dyn SupervisorSession>,
    request: SupervisorRequest,
    review: SupervisorInstructionReviewer,
) -> SupervisorEvaluator {
    Arc::new(move |prompt: JsString, signal: AbortSignal| {
        let session = Arc::clone(&session);
        let request = request.clone();
        let review = review.clone();
        Box::pin(async move { evaluate_supervisor_prompt(session, request, review, prompt, signal).await })
            as BoxFuture<'static, Result<JsValue, JsString>>
    })
}

async fn evaluate_supervisor_prompt(
    session: Arc<dyn SupervisorSession>,
    request: SupervisorRequest,
    review: SupervisorInstructionReviewer,
    prompt: JsString,
    signal: AbortSignal,
) -> Result<JsValue, JsString> {
    let abort_session = session.clone();
    let listener = signal.add_listener(move |_| {
        let session = abort_session.clone();
        drop(tokio::spawn(async move {
            if let Err(error) = session.abort().await {
                crate::utils::uncaught::throw_uncaught("Error", error);
            }
        }));
    });
    let response = evaluate_supervisor_prompt_inner(&session, prompt, signal.clone()).await;
    // Frozen returns the veto promise without awaiting it inside the listener's try/finally.
    signal.remove_listener(&listener);
    apply_supervisor_instruction_veto(&request, response?, session, review, signal).await
}

async fn evaluate_supervisor_prompt_inner(
    session: &Arc<dyn SupervisorSession>,
    prompt: JsString,
    signal: AbortSignal,
) -> Result<JsValue, JsString> {
    if signal.aborted() {
        session.abort().await?;
        return Err("Supervisor request aborted".into());
    }
    if session
        .context_percent()
        .is_some_and(|percent| percent >= SUPERVISOR_COMPACTION_PERCENT)
        && let Some(compact) = session.compact(&SUPERVISOR_COMPACTION_INSTRUCTIONS.into())
    {
        compact.await?;
        if let Some(session_id) = session.session_id().filter(|id| !id.is_empty()) {
            cleanup_supervisor_provider_context(&session_id)?;
        }
    }
    if signal.aborted() {
        return Err("Supervisor request aborted".into());
    }
    let previous_leaf_id = session.get_leaf_id();
    session.prompt(prompt).await?;
    read_current_supervisor_response(&session.get_branch(), previous_leaf_id.as_ref())
}

fn reviewable_goal_instructions(request: &SupervisorRequest, raw: &JsValue) -> Option<(JsString, JsString)> {
    if !matches!(
        request.kind,
        SupervisorRequestKind::GoalIdleReview | SupervisorRequestKind::GoalCompletionReview
    ) {
        return None;
    }
    match parse_supervisor_response(request.kind, raw) {
        Some(SupervisorResponse::Continue { instructions, reason }) if instructions != GENERIC_GOAL_CONTINUATION => {
            Some((instructions, reason))
        }
        _ => None,
    }
}

fn continue_response(instructions: JsString, reason: JsString) -> JsValue {
    JsValue::object([
        ("instructions", JsValue::String(instructions)),
        ("kind", JsValue::String("continue".into())),
        ("reason", JsValue::String(reason)),
    ])
}

async fn append_supervisor_policy_feedback(
    session: &Arc<dyn SupervisorSession>,
    content: JsString,
    details: JsValue,
) -> Result<(), JsString> {
    let message = CustomMessagePayload {
        custom_type: "supervisor_policy_feedback".into(),
        content: CustomMessageContent::Text(content),
        display: false,
        details: Some(details),
    };
    if let Some(send) = session.send_custom_message(message) {
        send.await?;
    }
    Ok(())
}

async fn append_review_failure_and_suppress(
    session: &Arc<dyn SupervisorSession>,
    failure: &'static str,
    reason: JsString,
) -> Result<JsValue, JsString> {
    append_supervisor_policy_feedback(
        session,
        js_format!(
            "Policy gate failure: {}. Non-generic instructions were suppressed.",
            failure
        ),
        pi_js_compat::js_json!({"failure": failure}),
    )
    .await?;
    Ok(continue_response(GENERIC_GOAL_CONTINUATION.into(), reason))
}

async fn apply_supervisor_instruction_veto(
    request: &SupervisorRequest,
    raw: JsValue,
    session: Arc<dyn SupervisorSession>,
    review: SupervisorInstructionReviewer,
    signal: AbortSignal,
) -> Result<JsValue, JsString> {
    let Some((instructions, reason)) = reviewable_goal_instructions(request, &raw) else {
        return Ok(raw);
    };
    let decision = match review(instructions.clone(), signal.clone()).await {
        Ok(decision) => decision,
        Err(error) => {
            if signal.aborted() {
                return Err(error);
            }
            return append_review_failure_and_suppress(&session, "evaluation_error", reason).await;
        }
    };
    if signal.aborted() {
        return Err("Supervisor instruction review aborted".into());
    }
    apply_supervisor_review_decision(decision, &session, instructions, reason).await
}

async fn apply_supervisor_review_decision(
    decision: Option<SupervisorInstructionReviewDecision>,
    session: &Arc<dyn SupervisorSession>,
    instructions: JsString,
    reason: JsString,
) -> Result<JsValue, JsString> {
    match decision {
        Some(SupervisorInstructionReviewDecision::Accept) => Ok(continue_response(instructions, reason)),
        Some(SupervisorInstructionReviewDecision::Reject(rejection)) => {
            append_supervisor_policy_feedback(
                session,
                supervisor_instruction_rejection_feedback(rejection),
                pi_js_compat::js_json!({"reason":rejection}),
            )
            .await?;
            Ok(continue_response(GENERIC_GOAL_CONTINUATION.into(), reason))
        }
        None => append_review_failure_and_suppress(session, "invalid_response", reason).await,
    }
}

/// `cleanupSupervisorProviderContext`.
pub fn cleanup_supervisor_provider_context(session_id: &JsString) -> Result<(), JsString> {
    cleanup_session_resources(Some(session_id)).map_err(|error| error.to_string().into())
}

/// `readCurrentSupervisorResponse` over the current session branch.
pub fn read_current_supervisor_response(
    entries: &[SessionEntry],
    previous_leaf_id: Option<&JsString>,
) -> Result<JsValue, JsString> {
    let previous_leaf_index = match previous_leaf_id {
        Some(previous_leaf_id) => entries
            .iter()
            .position(|entry| entry.id() == Some(previous_leaf_id))
            .map_or(-1, |index| index as i64),
        None => -1,
    };
    if previous_leaf_id.is_some() && previous_leaf_index == -1 {
        return Err("Supervisor request boundary is missing from the current session branch".into());
    }
    let current_messages: Vec<_> = entries
        .iter()
        .skip((previous_leaf_index + 1) as usize)
        .filter_map(|entry| entry.as_message())
        .collect();
    let assistant_messages: Vec<&pi_ai::types::AssistantMessage> = current_messages
        .iter()
        .filter_map(|message| match message.message.as_ref() {
            AgentMessage::Assistant(assistant) => Some(assistant.as_ref()),
            _ => None,
        })
        .collect();
    let Some(terminal_message) = assistant_messages.last() else {
        return Err("Supervisor model returned no assistant text for current request".into());
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
            .ok_or_else(|| JsString::from("Supervisor model returned no assistant text for current request"))?
    } else {
        *terminal_message
    };
    if response_message.stop_reason != StopReason::Stop {
        return Err(format!(
            "Supervisor model request ended with {}",
            stop_reason_name(response_message.stop_reason)
        )
        .into());
    }
    let text = JsString::join(
        response_message.content.iter().filter_map(|part| match part {
            AssistantContent::Text(text) => Some(&text.text),
            _ => None,
        }),
        "",
    );
    if text.trim().is_empty() {
        return Err("Supervisor model returned no assistant text for current request".into());
    }
    Ok(JsValue::String(text))
}

fn read_structured_supervisor_response(message: &pi_ai::types::AssistantMessage) -> Result<Option<JsValue>, JsString> {
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
        return Err("Supervisor model emitted assistant text with structured response".into());
    }
    Ok(Some(JsValue::Object(response_tool_call.arguments.clone())))
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
        StopReason::Stop => "stop",
        StopReason::Length => "length",
        StopReason::ToolUse => "toolUse",
        StopReason::Error => "error",
        StopReason::Aborted => "aborted",
    }
}

/// `createSupervisorResourceLoader`.
pub async fn create_supervisor_resource_loader(
    agent_dir: &JsString,
    kb_dir: &JsString,
    settings_manager: Arc<Mutex<SettingsManager>>,
) -> Result<Arc<DefaultResourceLoader>, pi_ai::request::CallbackError> {
    let gate_kb_dir = kb_dir.to_owned();
    let gate: ExtensionFactory = Arc::new(move |api| {
        let gate_kb_dir = gate_kb_dir.clone();
        let gate: ToolGateHandle = Arc::new(move |event: ToolCallEvent, _context: ExtensionContext| {
            let gate_kb_dir = gate_kb_dir.clone();
            Box::pin(async move {
                let input = JsValue::Object(event.input.clone());
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
    let errors: Vec<(JsString, JsString)> = extensions
        .errors
        .iter()
        .map(|error| (error.path.clone(), error.error.clone()))
        .collect();
    validate_supervisor_extension_load(&errors)?;
    Ok(Arc::new(loader))
}

/// Options for [`create_supervisor_session`].
pub struct CreateSupervisorSessionOptions {
    pub agent_dir: JsString,
    pub kb_dir: JsString,
    pub model_registry: ModelRegistry,
    pub resource_loader: Arc<dyn ResourceLoader>,
    pub session_manager: SessionManager,
    pub settings_manager: Arc<Mutex<SettingsManager>>,
}

/// `createSupervisorSession`.
pub async fn create_supervisor_session(
    options: CreateSupervisorSessionOptions,
) -> Result<CreateAgentSessionResult, JsString> {
    let models = require_supervisor_models(&options.model_registry)?;
    create_supervisor_session_for_model(options, models[0].clone()).await
}

async fn create_supervisor_session_for_model(
    options: CreateSupervisorSessionOptions,
    model: Model,
) -> Result<CreateAgentSessionResult, JsString> {
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
        auth_storage: Some(options.model_registry.auth_storage().clone()),
        model_registry: Some(options.model_registry),
        resource_loader: Some(options.resource_loader),
        session_manager: Some(options.session_manager),
        settings_manager: Some(options.settings_manager),
        thinking_level: Some(pi_agent_core::harness::types::ThinkingLevel::Low),
        tools: Some(tools),
        ..Default::default()
    })
    .await
    .map_err(|error| error.message)
}

impl SupervisorSession for AgentSession {
    fn send_custom_message(&self, message: CustomMessagePayload) -> Option<BoxFuture<'static, Result<(), JsString>>> {
        let session = self.clone();
        Some(Box::pin(async move {
            session
                .send_custom_message(
                    &message.custom_type,
                    message.content,
                    message.display,
                    message.details,
                    false,
                    None,
                )
                .await
                .map_err(|error| error.message().clone())
        }))
    }

    fn abort(&self) -> BoxFuture<'static, Result<(), JsString>> {
        let session = self.clone();
        Box::pin(async move { session.abort().await.map_err(|error| error.message().to_owned()) })
    }

    fn prompt(&self, content: JsString) -> BoxFuture<'static, Result<(), JsString>> {
        let session = self.clone();
        Box::pin(async move {
            session
                .prompt(content, PromptOptions::default())
                .await
                .map_err(|error| error.message().to_owned())
        })
    }

    fn compact(&self, instructions: &JsString) -> Option<BoxFuture<'static, Result<(), JsString>>> {
        let session = self.clone();
        let instructions = JsString::from(instructions);
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

    fn session_id(&self) -> Option<JsString> {
        Some(AgentSession::session_id(self))
    }

    fn get_branch(&self) -> Vec<SessionEntry> {
        self.session_manager().get_branch(None)
    }

    fn get_leaf_id(&self) -> Option<JsString> {
        self.session_manager().get_leaf_id().cloned()
    }
}

struct SupervisorConsoleHost {
    session: AgentSession,
    cwd: JsString,
    generation: u64,
    managed_by: ResidentConsoleManagedBy,
    prompts: Arc<Mutex<SupervisorConsolePromptQueue>>,
    control_db_path: JsString,
}

impl ResidentConsoleHost<SessionEntry, AgentSessionEvent> for SupervisorConsoleHost {
    fn get_snapshot(&self) -> ResidentConsoleSnapshot<SessionEntry> {
        create_supervisor_console_snapshot(
            &self.cwd,
            self.generation,
            self.managed_by,
            Some(&AgentSession::session_id(&self.session)),
            self.session.session_manager().get_branch(None),
        )
    }

    fn enqueue_prompt(&self, text: JsString, id: JsString) -> pi_ai::request::BoxFuture<'static, Result<(), JsString>> {
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
pub async fn run_supervisor_service() -> Result<(), pi_ai::request::CallbackError> {
    let agent_dir = crate::config::get_agent_dir().map_err(pi_ai::request::CallbackError::from)?;
    let kb_dir = pi_js_compat::process_env::get("PI_KB_DIR")
        .map(JsString::from)
        .unwrap_or_else(|| DEFAULT_SUPERVISOR_KB_DIR.into());
    let control_db_path = crate::core::session_control_db::get_control_db_path(
        &crate::config::get_user_state_root().map_err(pi_ai::request::CallbackError::from)?,
    );
    let session_manager = open_supervisor_session(&agent_dir, &kb_dir, &control_db_path)?;
    let session = create_supervisor_agent_session(&agent_dir, &kb_dir, session_manager).await?;
    let review = create_supervisor_instruction_reviewer(create_supervisor_instruction_evaluator(&agent_dir)?);
    let wake_server = Arc::new(SupervisorRequestWakeServer::new(&control_db_path));
    let console_prompts = Arc::new(Mutex::new(SupervisorConsolePromptQueue::new()));
    let console_server =
        create_supervisor_console_server(session.clone(), kb_dir, &control_db_path, console_prompts.clone());
    wake_server.start().await?;
    console_server.start().await?;
    let loop_result = run_supervisor_runtime_loop(
        control_db_path,
        session.clone(),
        console_prompts,
        wake_server.clone(),
        review,
    )
    .await;
    console_server.close().await?;
    wake_server.close().await?;
    session.abort().await.map_err(pi_ai::request::CallbackError::from)?;
    loop_result
}

async fn create_supervisor_agent_session(
    agent_dir: &JsString,
    kb_dir: &JsString,
    session_manager: SessionManager,
) -> Result<AgentSession, pi_ai::request::CallbackError> {
    let model_registry = ModelRegistry::create(
        crate::core::auth_storage::AuthStorage::create(Some(&node_join(agent_dir, "auth.json")))
            .map_err(pi_ai::request::CallbackError::from)?,
        Some(node_join(agent_dir, "models.json")),
    )?;
    let model = require_supervisor_models(&model_registry)?[0].clone();
    let settings_manager = Arc::new(Mutex::new(create_supervisor_settings_manager()));
    let resource_loader = create_supervisor_resource_loader(agent_dir, kb_dir, Arc::clone(&settings_manager)).await?;
    Ok(create_supervisor_session_for_model(
        CreateSupervisorSessionOptions {
            agent_dir: agent_dir.clone(),
            kb_dir: kb_dir.clone(),
            model_registry,
            resource_loader,
            session_manager,
            settings_manager,
        },
        model,
    )
    .await?
    .session)
}

fn create_supervisor_console_server(
    session: AgentSession,
    kb_dir: JsString,
    control_db_path: &JsString,
    console_prompts: Arc<Mutex<SupervisorConsolePromptQueue>>,
) -> ResidentConsoleServer<SessionEntry, AgentSessionEvent> {
    ResidentConsoleServer::new(ResidentConsoleServerOptions {
        socket_path: js_format!("{}.supervisor-console.sock", control_db_path),
        service: ResidentConsoleService::Supervisor,
        host: Arc::new(SupervisorConsoleHost {
            session,
            cwd: kb_dir,
            generation: std::process::id() as u64,
            managed_by: if pi_js_compat::process_env::get(crate::supervisor::ensure_running::SUPERVISOR_AUTOSTART_ENV)
                .is_some_and(|value| value == "1")
            {
                ResidentConsoleManagedBy::Pi
            } else {
                ResidentConsoleManagedBy::External
            },
            prompts: console_prompts,
            control_db_path: control_db_path.clone(),
        }),
    })
}

async fn run_supervisor_runtime_loop(
    control_db_path: JsString,
    session: AgentSession,
    console_prompts: Arc<Mutex<SupervisorConsolePromptQueue>>,
    wake_server: Arc<SupervisorRequestWakeServer>,
    review: SupervisorInstructionReviewer,
) -> Result<(), pi_ai::request::CallbackError> {
    let controller = pi_js_compat::abort::AbortController::new();
    let signal = controller.signal();
    let stop_signals = install_stop_signals(&controller)?;
    let loop_result = async {
        run_supervisor_request_loop(RunSupervisorRequestLoopInput {
            claim_token: pi_ai::utils::random_uuid::random_uuid()?.into(),
            console_prompts,
            control_db_path,
            session: Arc::new(session),
            signal,
            wake_server,
            claim_next_request: None,
            process_request: Some(Arc::new(move |path, request, session| {
                let review = review.clone();
                Box::pin(async move { process_supervisor_request(&path, request, session, Some(review)).await })
            })),
        })
        .await
        .map_err(pi_ai::request::CallbackError::from)
    }
    .await;
    controller.abort(None);
    stop_signals.abort();
    loop_result
}

fn install_stop_signals(
    signal: &pi_js_compat::abort::AbortController,
) -> Result<tokio::task::JoinHandle<()>, pi_ai::request::CallbackError> {
    let signal = signal.clone();
    let mut interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
        .map_err(|error| pi_ai::request::CallbackError::new(error.to_string()))?;
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|error| pi_ai::request::CallbackError::new(error.to_string()))?;
    Ok(tokio::spawn(async move {
        tokio::select! {
            _ = interrupt.recv() => {}
            _ = terminate.recv() => {}
        }
        signal.abort(None);
    }))
}

#[cfg(test)]
mod lossless_path_tests {
    use super::*;

    #[test]
    fn realpath_encodes_existing_prefix_but_keeps_missing_suffix_units() {
        let directory = tempfile::tempdir().unwrap();
        let root = JsString::from(directory.path().to_str().unwrap()) + "/root-" + JsString::from_utf16(vec![0xd800]);
        let disk = pi_js_compat::js_json::encode_node_utf8(&root);
        std::fs::create_dir(&disk).unwrap();
        let suffix = JsString::from("/missing-") + JsString::from_utf16(vec![0xdfff]);
        let actual = resolve_existing_path(&(root.clone() + &suffix)).unwrap();
        let canonical = std::fs::canonicalize(disk).unwrap();
        assert_eq!(actual, JsString::from(canonical.to_str().unwrap()) + &suffix);
        assert!(is_path_within_root(&actual, &resolve_existing_path(&root).unwrap()));
        assert!(!is_path_within_root(
            &(JsString::from("/outside-") + JsString::from_utf16(vec![0xdfff])),
            &resolve_existing_path(&root).unwrap()
        ));
    }
}
