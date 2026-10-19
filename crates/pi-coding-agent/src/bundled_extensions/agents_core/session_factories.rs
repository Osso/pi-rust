//! Session factories and prompt validation
//! (`extensions/agents-core/src/session-factories.ts`).

use std::path::Path;
use std::sync::Arc;

use pi_agent_core::agent::types::AgentMessage;
use pi_agent_core::harness::types::ThinkingLevel;
use pi_ai::request::BoxFuture;
use pi_js_compat::string::{js_trim, utf16_len};
use serde::Serialize;
use serde_json::Value;

use crate::core::agent_session::{AgentSession, ExtensionBindings, PromptOptions};
use crate::core::extensions::types::{ExtensionContext, SessionStartEvent, SessionStartReason};
use crate::core::multi_agent_store::{AgentSnapshot, AgentTranscriptMetadata, MultiAgentStore};
use crate::core::permissions::policy::normalize_approval_policy;
use crate::core::permissions::presets::approval_preset_for_policy;
use crate::core::resource_loader::InlineExtension;
use crate::core::sdk::CreateAgentSessionOptions;
use crate::core::session_manager::{
    ForkEntrySelection, KnownSessionEntry, NewSessionOptions, SessionEntry, SessionManager, SharedSessionManager,
    lock_session_manager,
};
use crate::core::settings_manager::{Settings, SettingsManager, SettingsManagerCreateOptions};
use crate::core::tool_capabilities::{CHILD_DISABLED_AGENT_TOOL_NAMES, SUPERVISOR_ONLY_TOOL_NAMES};

use super::shared::{
    AttachedSessionDispatchInput, AttachedSessionFactory, ChildAgentDispatchInput, ChildAgentSession,
    ChildAgentSessionFactory,
};

pub const MAX_GOAL_OBJECTIVE_CHARS: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalObjectiveValidation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub objective: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length: Option<usize>,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<GoalObjectiveFailure>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalObjectiveFailure {
    Empty,
    TooLong,
}

impl GoalObjectiveValidation {
    pub fn reason_str(&self) -> Option<&'static str> {
        self.reason.map(|reason| match reason {
            GoalObjectiveFailure::Empty => "empty",
            GoalObjectiveFailure::TooLong => "too_long",
        })
    }
}

pub fn validate_goal_objective(prompt: &str) -> GoalObjectiveValidation {
    let objective = js_trim(prompt);
    if objective.is_empty() {
        return GoalObjectiveValidation {
            objective: None,
            length: Some(0),
            ok: false,
            reason: Some(GoalObjectiveFailure::Empty),
        };
    }
    let length = utf16_len(objective);
    if length > MAX_GOAL_OBJECTIVE_CHARS {
        return GoalObjectiveValidation {
            objective: None,
            length: Some(length),
            ok: false,
            reason: Some(GoalObjectiveFailure::TooLong),
        };
    }
    GoalObjectiveValidation {
        objective: Some(objective.to_owned()),
        length: None,
        ok: true,
        reason: None,
    }
}

pub fn is_production_child_session_factory(factory: &ChildAgentSessionFactory) -> bool {
    factory.is_production()
}

const GOAL_EXTENSION_PATH: &str = "<first-party:goal>";
const CHILD_AGENT_CONTROL_TOOL_NAMES: [&str; 3] = ["contact_parent", "send_agent_message", "end_turn"];

/// `extensionFactories?: ExtensionFactory[] | (() => ExtensionFactory[])`.
#[derive(Clone)]
pub enum ChildExtensionFactories {
    Static(Vec<InlineExtension>),
    Dynamic(Arc<dyn Fn() -> Vec<InlineExtension> + Send + Sync>),
}

/// The parts of `AgentSession` the production factory drives.
pub trait UnboundChildAgentSession: ChildAgentSession {
    fn bind_extensions(&self) -> BoxFuture<'static, Result<(), String>>;
    fn session_manager(&self) -> SharedSessionManager;
}

/// `AgentSession` adapter for [`UnboundChildAgentSession`].
pub struct AgentSessionChildSession {
    session: AgentSession,
    session_manager: SharedSessionManager,
}

impl AgentSessionChildSession {
    pub fn new(session: AgentSession) -> Self {
        let session_manager = session.session_manager_handle();
        Self {
            session,
            session_manager,
        }
    }
}

impl ChildAgentSession for AgentSessionChildSession {
    fn abort(&self) {
        let session = self.session.clone();
        tokio::spawn(async move {
            // The source calls `void this.abort()` and drops the promise.
            drop(session.abort().await);
        });
    }

    fn dispose(&self) {
        self.session.dispose();
    }

    fn drain_runtime_coordination(&self) -> Option<BoxFuture<'static, Result<(), String>>> {
        let session = self.session.clone();
        Some(Box::pin(async move {
            session
                .drain_runtime_coordination()
                .await
                .map_err(|error| error.message().to_owned())
        }))
    }

    fn messages(&self) -> Vec<AgentMessage> {
        self.session.messages()
    }

    fn prompt(&self, text: String) -> BoxFuture<'static, Result<(), String>> {
        let session = self.session.clone();
        Box::pin(async move {
            session
                .prompt(&text, PromptOptions::default())
                .await
                .map_err(|error| error.message().to_owned())
        })
    }

    fn transcript(&self) -> Option<AgentTranscriptMetadata> {
        None
    }

    fn session_mutation_target(&self) -> Option<Arc<crate::core::extensions::types::ViewedSessionMutationTarget>> {
        use crate::core::extensions::types::{HandlerError, ViewedSessionMutationTarget};

        let session = self.session.clone();
        let set_model_session = self.session.clone();
        let set_thinking_session = self.session.clone();
        Some(Arc::new(ViewedSessionMutationTarget {
            model: Some(session.model()),
            model_registry: Arc::new(session.model_registry().clone()),
            scoped_models: session.scoped_models(),
            thinking_level: session.thinking_level(),
            set_model: Arc::new(move |model| {
                let session = set_model_session.clone();
                Box::pin(async move {
                    session
                        .set_model(model)
                        .await
                        .map(|()| true)
                        .map_err(|error| HandlerError::new(error.message()))
                })
            }),
            set_thinking_level: Arc::new(move |level| {
                set_thinking_session
                    .set_thinking_level(level)
                    .map_err(|error| HandlerError::new(error.to_string()))
            }),
        }))
    }
}

impl UnboundChildAgentSession for AgentSessionChildSession {
    fn bind_extensions(&self) -> BoxFuture<'static, Result<(), String>> {
        let session = self.session.clone();
        Box::pin(async move {
            session.bind_extensions(ExtensionBindings::default()).await;
            Ok(())
        })
    }

    fn session_manager(&self) -> SharedSessionManager {
        Arc::clone(&self.session_manager)
    }
}

pub type CreateChildSession = Arc<
    dyn Fn(CreateAgentSessionOptions) -> BoxFuture<'static, Result<Arc<dyn UnboundChildAgentSession>, String>>
        + Send
        + Sync,
>;

pub type CreateChildSessionManager =
    Arc<dyn Fn(&str, Option<&str>, NewSessionOptions) -> Result<SessionManager, String> + Send + Sync>;

/// `ProductionChildAgentSessionFactoryOptions`.
#[derive(Clone)]
pub struct ProductionChildAgentSessionFactoryOptions {
    pub agent_dir: Option<String>,
    pub create_session: CreateChildSession,
    pub create_session_manager: CreateChildSessionManager,
    pub extension_factories: Option<ChildExtensionFactories>,
    pub multi_agent_store: Option<Arc<MultiAgentStore>>,
    pub session_dir: Option<String>,
}

/// `createProductionChildAgentSessionFactory`.
pub fn create_production_child_agent_session_factory(
    options: ProductionChildAgentSessionFactoryOptions,
) -> ChildAgentSessionFactory {
    ChildAgentSessionFactory::production(move |input| {
        let options = options.clone();
        Box::pin(async move { create_production_child_session(input, &options).await })
    })
}

#[derive(Clone)]
pub struct SharedProductionSessionOptions {
    pub agent_dir: Option<String>,
    pub create_session: CreateChildSession,
    pub extension_factories: Option<ChildExtensionFactories>,
    pub multi_agent_store: Option<Arc<MultiAgentStore>>,
    pub session_dir: Option<String>,
}

/// `createProductionAttachedSessionFactory` options (no `createSessionManager`).
#[derive(Clone)]
pub struct ProductionAttachedSessionFactoryOptions {
    pub agent_dir: Option<String>,
    pub create_session: CreateChildSession,
    pub extension_factories: Option<ChildExtensionFactories>,
    pub multi_agent_store: Option<Arc<MultiAgentStore>>,
    pub session_dir: Option<String>,
}

impl From<&ProductionAttachedSessionFactoryOptions> for SharedProductionSessionOptions {
    fn from(options: &ProductionAttachedSessionFactoryOptions) -> Self {
        Self {
            agent_dir: options.agent_dir.clone(),
            create_session: Arc::clone(&options.create_session),
            extension_factories: options.extension_factories.clone(),
            multi_agent_store: options.multi_agent_store.clone(),
            session_dir: options.session_dir.clone(),
        }
    }
}

/// `createProductionAttachedSessionFactory`.
pub fn create_production_attached_session_factory(
    options: ProductionAttachedSessionFactoryOptions,
) -> AttachedSessionFactory {
    AttachedSessionFactory::new(move |input| {
        let options = options.clone();
        Box::pin(async move { create_production_attached_session(input, &options).await })
    })
}

fn resolve_child_extension_factories(extension_factories: Option<&ChildExtensionFactories>) -> Vec<InlineExtension> {
    let factories = match extension_factories {
        Some(ChildExtensionFactories::Static(factories)) => factories.clone(),
        Some(ChildExtensionFactories::Dynamic(provider)) => provider(),
        None => return Vec::new(),
    };
    factories
        .into_iter()
        .filter(|factory| factory.explicit_extension_path() != Some(GOAL_EXTENSION_PATH))
        .collect()
}

fn is_message_role(entry: &SessionEntry, role: &str) -> bool {
    matches!(entry, SessionEntry::Known(KnownSessionEntry::Message(message)) if message.message.role() == role)
}

fn find_active_tool_call_entry<'a>(branch: &'a [SessionEntry], active_tool_call_id: &str) -> Option<&'a SessionEntry> {
    branch.iter().rev().find(|entry| {
        let SessionEntry::Known(KnownSessionEntry::Message(message)) = entry else {
            return false;
        };
        let pi_agent_core::AgentMessage::Assistant(assistant) = message.message.as_ref() else {
            return false;
        };
        assistant.content.iter().any(
            |part| matches!(part, pi_ai::types::AssistantContent::ToolCall(call) if call.id == active_tool_call_id),
        )
    })
}

fn find_unresolved_turn_parent_id(branch: &[SessionEntry], active_tool_entry: &SessionEntry) -> Option<String> {
    let immediate_parent = active_tool_entry
        .parent_id()
        .and_then(|parent_id| branch.iter().find(|entry| entry.id() == Some(parent_id)));
    if let Some(parent) = immediate_parent
        && is_message_role(parent, "assistant")
    {
        return parent.id().map(str::to_owned);
    }
    let mut ancestor = immediate_parent;
    while let Some(entry) = ancestor {
        if is_message_role(entry, "user") {
            return entry.parent_id().map(str::to_owned);
        }
        ancestor = entry
            .parent_id()
            .and_then(|parent_id| branch.iter().find(|candidate| candidate.id() == Some(parent_id)));
    }
    active_tool_entry.parent_id().map(str::to_owned)
}

fn resolve_inherited_leaf_selection(
    context: super::shared::SpawnContext,
    active_tool_call_id: Option<&str>,
    leaf_id: Option<&str>,
    branch: &[SessionEntry],
) -> Result<ForkEntrySelection, String> {
    if let Some(active_tool_call_id) = active_tool_call_id
        && js_trim(active_tool_call_id).is_empty()
    {
        return Err("Cannot inherit context: active tool call identity must be non-empty".to_owned());
    }
    let Some(active_tool_call_id) = active_tool_call_id else {
        return Ok(leaf_id.map_or(ForkEntrySelection::All, |id| ForkEntrySelection::From(id.to_owned())));
    };
    if let Some(entry) = find_active_tool_call_entry(branch, active_tool_call_id) {
        return Ok(
            find_unresolved_turn_parent_id(branch, entry).map_or(ForkEntrySelection::None, ForkEntrySelection::From)
        );
    }
    if context == super::shared::SpawnContext::Inherit {
        return Err(format!(
            "Cannot inherit context: active tool call {active_tool_call_id} is not in the parent branch"
        ));
    }
    Ok(leaf_id.map_or(ForkEntrySelection::All, |id| ForkEntrySelection::From(id.to_owned())))
}

struct ParentSessionSnapshot {
    branch: Vec<SessionEntry>,
    leaf_id: Option<String>,
    session_dir: String,
    session_file: Option<String>,
    session_id: String,
}

fn parent_session_snapshot(ctx: &ExtensionContext) -> Result<ParentSessionSnapshot, String> {
    let manager = ctx.session_manager().map_err(|error| error.to_string())?;
    let manager = lock_session_manager(&manager);
    Ok(ParentSessionSnapshot {
        branch: manager.get_branch(None),
        leaf_id: manager.get_leaf_id().map(str::to_owned),
        session_dir: manager.get_session_dir().to_owned(),
        session_file: manager.get_session_file().map(str::to_owned),
        session_id: manager.get_session_id().to_owned(),
    })
}

fn validate_persisted_inheritance(
    context: super::shared::SpawnContext,
    parent_session_file: Option<&str>,
) -> Result<(), String> {
    if context == super::shared::SpawnContext::Inherit && parent_session_file.is_none() {
        return Err("Cannot inherit context from an unpersisted parent session".to_owned());
    }
    Ok(())
}

fn create_child_session_manager_options(agent: &AgentSnapshot, parent_session: &str) -> NewSessionOptions {
    NewSessionOptions {
        parent_session: Some(parent_session.to_owned()),
        is_subagent: true,
        subagent_name: Some(agent.display_name.clone()),
        ..Default::default()
    }
}

fn non_empty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}

fn create_child_session_manager(
    input: &ChildAgentDispatchInput,
    options: &ProductionChildAgentSessionFactoryOptions,
) -> Result<(Option<String>, SessionManager), String> {
    let parent = parent_session_snapshot(&input.ctx)?;
    validate_persisted_inheritance(input.context, parent.session_file.as_deref())?;
    let session_dir = options.session_dir.clone().or_else(|| non_empty(&parent.session_dir));
    let manager_options = create_child_session_manager_options(
        &input.agent,
        parent.session_file.as_deref().unwrap_or(&parent.session_id),
    );
    let session_manager = if input.context == super::shared::SpawnContext::Inherit && parent.session_file.is_some() {
        let parent_session_file = parent.session_file.as_deref().unwrap_or_default();
        let selection = resolve_inherited_leaf_selection(
            input.context,
            input.active_tool_call_id.as_deref(),
            parent.leaf_id.as_deref(),
            &parent.branch,
        )?;
        SessionManager::fork_from(
            parent_session_file,
            &input.agent.cwd,
            session_dir.as_deref(),
            Some(manager_options),
            Some(selection),
        )
        .map_err(|error| error.to_string())?
    } else {
        (options.create_session_manager)(&input.agent.cwd, session_dir.as_deref(), manager_options)?
    };
    Ok((parent.session_file, session_manager))
}

fn parse_thinking_level(value: &str) -> Option<ThinkingLevel> {
    match value {
        "off" => Some(ThinkingLevel::Off),
        "minimal" => Some(ThinkingLevel::Minimal),
        "low" => Some(ThinkingLevel::Low),
        "medium" => Some(ThinkingLevel::Medium),
        "high" => Some(ThinkingLevel::High),
        "xhigh" => Some(ThinkingLevel::XHigh),
        "max" => Some(ThinkingLevel::Max),
        "ultra" => Some(ThinkingLevel::Ultra),
        _ => None,
    }
}

struct ChildAgentProfile {
    model: Option<pi_ai::types::Model>,
    thinking_level: Option<ThinkingLevel>,
    tools: Option<Vec<String>>,
}

fn resolve_child_agent_profile(agent: &AgentSnapshot, ctx: &ExtensionContext) -> Result<ChildAgentProfile, String> {
    let configured_model = match agent.model.as_ref() {
        Some(model) => {
            let registry = ctx.model_registry().map_err(|error| error.to_string())?;
            registry.find(&model.provider_id, &model.model_id)
        }
        None => None,
    };
    let thinking_level = agent
        .model
        .as_ref()
        .and_then(|model| model.thinking_level.as_deref())
        .and_then(parse_thinking_level);
    let tools = ctx
        .settings_manager()
        .map_err(|error| error.to_string())?
        .and_then(|settings| settings.lock().expect("settings").get_agent_profile(&agent.agent_type))
        .and_then(|profile| profile.tools);
    Ok(ChildAgentProfile {
        model: configured_model,
        thinking_level,
        tools,
    })
}

fn resolve_child_agent_tools(tools: Option<Vec<String>>) -> Option<Vec<String>> {
    let tools = tools?;
    let mut resolved: Vec<String> = Vec::new();
    for name in tools
        .into_iter()
        .chain(CHILD_AGENT_CONTROL_TOOL_NAMES.iter().map(|name| (*name).to_owned()))
    {
        if !resolved.contains(&name) {
            resolved.push(name);
        }
    }
    Some(resolved)
}

fn create_session_start_event(reason: SessionStartReason, previous_session_file: Option<String>) -> SessionStartEvent {
    SessionStartEvent {
        reason,
        previous_session_file,
    }
}

struct CreateBoundChildSessionInput {
    agent: AgentSnapshot,
    ctx: ExtensionContext,
    options: SharedProductionSessionOptions,
    persist_for_recovery: bool,
    session_manager: SessionManager,
    session_start_event: SessionStartEvent,
    settings_manager: Option<Arc<std::sync::Mutex<SettingsManager>>>,
}

fn create_child_agent_session_options(
    input: &CreateBoundChildSessionInput,
) -> Result<CreateAgentSessionOptions, String> {
    let profile = resolve_child_agent_profile(&input.agent, &input.ctx)?;
    let session_manager = input.ctx.session_manager().map_err(|error| error.to_string())?;
    let parent_session_id = lock_session_manager(&session_manager).get_session_id().to_owned();
    Ok(CreateAgentSessionOptions {
        agent_dir: input.options.agent_dir.clone(),
        cwd: Some(input.agent.cwd.clone()),
        exclude_tools: Some(
            CHILD_DISABLED_AGENT_TOOL_NAMES
                .iter()
                .chain(SUPERVISOR_ONLY_TOOL_NAMES.iter())
                .map(|name| (*name).to_owned())
                .collect(),
        ),
        extension_factories: resolve_child_extension_factories(input.options.extension_factories.as_ref()),
        model: profile.model.or_else(|| input.ctx.model().ok().flatten()),
        tools: resolve_child_agent_tools(profile.tools),
        multi_agent_agent_id: Some(input.agent.id.clone()),
        multi_agent_parent_session_id: Some(parent_session_id),
        multi_agent_requires_agent_id: true,
        multi_agent_runtime_role: Some(crate::core::agent_session::MultiAgentRuntimeRole::Child),
        multi_agent_store: input.options.multi_agent_store.clone(),
        session_manager: Some(input.session_manager.clone()),
        session_start_event: Some(input.session_start_event.clone()),
        settings_manager: input.settings_manager.clone(),
        thinking_level: profile.thinking_level,
        ..Default::default()
    })
}

struct TranscriptChildSession {
    inner: Arc<dyn UnboundChildAgentSession>,
    transcript: AgentTranscriptMetadata,
}

impl ChildAgentSession for TranscriptChildSession {
    fn abort(&self) {
        self.inner.abort();
    }

    fn dispose(&self) {
        self.inner.dispose();
    }

    fn drain_runtime_coordination(&self) -> Option<BoxFuture<'static, Result<(), String>>> {
        self.inner.drain_runtime_coordination()
    }

    fn messages(&self) -> Vec<AgentMessage> {
        self.inner.messages()
    }

    fn prompt(&self, text: String) -> BoxFuture<'static, Result<(), String>> {
        self.inner.prompt(text)
    }

    fn transcript(&self) -> Option<AgentTranscriptMetadata> {
        Some(self.transcript.clone())
    }
}

async fn create_bound_child_session(input: CreateBoundChildSessionInput) -> Result<Arc<dyn ChildAgentSession>, String> {
    let session_options = create_child_agent_session_options(&input)?;
    let session = (input.options.create_session)(session_options).await?;
    session.bind_extensions().await?;
    let session_manager = session.session_manager();
    let transcript = {
        let mut manager = lock_session_manager(&session_manager);
        let transcript = AgentTranscriptMetadata {
            path: manager.get_session_file().map(str::to_owned),
            session_id: manager.get_session_id().to_owned(),
        };
        if input.persist_for_recovery {
            manager.persist_for_recovery().map_err(|error| error.to_string())?;
        }
        transcript
    };
    Ok(Arc::new(TranscriptChildSession {
        inner: session,
        transcript,
    }) as Arc<dyn ChildAgentSession>)
}

async fn create_production_child_session(
    input: ChildAgentDispatchInput,
    options: &ProductionChildAgentSessionFactoryOptions,
) -> Result<Arc<dyn ChildAgentSession>, String> {
    let validation = validate_goal_objective(&input.prompt);
    if !validation.ok {
        return Err(spawn_prompt_validation_message(&validation));
    }
    let (parent_session_file, session_manager) = create_child_session_manager(&input, options)?;
    create_bound_child_session(CreateBoundChildSessionInput {
        agent: input.agent,
        ctx: input.ctx,
        options: SharedProductionSessionOptions {
            agent_dir: options.agent_dir.clone(),
            create_session: Arc::clone(&options.create_session),
            extension_factories: options.extension_factories.clone(),
            multi_agent_store: options.multi_agent_store.clone(),
            session_dir: options.session_dir.clone(),
        },
        persist_for_recovery: true,
        session_manager,
        session_start_event: create_session_start_event(SessionStartReason::Fork, parent_session_file),
        settings_manager: None,
    })
    .await
}

fn validate_attached_session_identity(agent: &AgentSnapshot, session_manager: &SessionManager) -> Result<(), String> {
    let Some(expected_session_id) = agent.transcript.as_ref().map(|transcript| &transcript.session_id) else {
        return Ok(());
    };
    let actual_session_id = session_manager.get_session_id();
    if actual_session_id != expected_session_id {
        return Err(format!(
            "Child agent transcript session mismatch: expected {expected_session_id}, found {actual_session_id}"
        ));
    }
    Ok(())
}

fn open_attached_session_manager(
    input: &AttachedSessionDispatchInput,
    options: &SharedProductionSessionOptions,
) -> Result<SessionManager, String> {
    if input.session_path.is_empty() {
        return Err("Cannot resume attached session without a session path".to_owned());
    }
    if !Path::new(&input.session_path).exists() {
        return Err(format!("Child agent transcript does not exist: {}", input.session_path));
    }
    let ctx_cwd = input.ctx.cwd().map_err(|error| error.to_string())?;
    if input.agent.cwd != ctx_cwd && !Path::new(&input.agent.cwd).exists() {
        return Err(format!(
            "Agent {} working directory does not exist: {}",
            input.agent.id, input.agent.cwd
        ));
    }
    let session_manager = input.ctx.session_manager().map_err(|error| error.to_string())?;
    let parent_session_dir = lock_session_manager(&session_manager).get_session_dir().to_owned();
    let session_dir = options.session_dir.clone().or_else(|| non_empty(&parent_session_dir));
    let mut session_manager = SessionManager::open(&input.session_path, session_dir.as_deref(), Some(&input.agent.cwd))
        .map_err(|error| error.to_string())?;
    validate_attached_session_identity(&input.agent, &session_manager)?;
    let control_db_path = input.ctx.control_db_path().map_err(|error| error.to_string())?;
    session_manager
        .set_metadata_control_db_path(control_db_path.as_deref(), Default::default())
        .map_err(|error| error.to_string())?;
    Ok(session_manager)
}

fn create_attached_settings_manager(agent: &AgentSnapshot, agent_dir: Option<&str>) -> Result<SettingsManager, String> {
    let mut settings_manager = SettingsManager::create(
        &agent.cwd,
        agent_dir.map(Path::new),
        SettingsManagerCreateOptions {
            project_trusted: Some(false),
        },
    )
    .map_err(|error| error.message().to_owned())?;
    let approval_policy = normalize_approval_policy(Some(&agent.permission.policy));
    let mut overrides = Settings::new();
    overrides.insert(
        "approvalPolicy".to_owned(),
        Value::String(approval_policy.as_str().to_owned()),
    );
    overrides.insert(
        "approvalPreset".to_owned(),
        Value::String(approval_preset_for_policy(approval_policy).as_str().to_owned()),
    );
    settings_manager.apply_overrides(&overrides);
    Ok(settings_manager)
}

async fn create_production_attached_session(
    input: AttachedSessionDispatchInput,
    options: &ProductionAttachedSessionFactoryOptions,
) -> Result<Arc<dyn ChildAgentSession>, String> {
    let shared = SharedProductionSessionOptions::from(options);
    let session_manager = open_attached_session_manager(&input, &shared)?;
    let parent_session_file = {
        let parent_manager = input.ctx.session_manager().map_err(|error| error.to_string())?;
        lock_session_manager(&parent_manager)
            .get_session_file()
            .map(str::to_owned)
    };
    let settings_manager = create_attached_settings_manager(&input.agent, options.agent_dir.as_deref())?;
    create_bound_child_session(CreateBoundChildSessionInput {
        agent: input.agent,
        ctx: input.ctx,
        options: shared,
        persist_for_recovery: false,
        session_manager,
        session_start_event: create_session_start_event(SessionStartReason::Resume, parent_session_file),
        settings_manager: Some(Arc::new(std::sync::Mutex::new(settings_manager))),
    })
    .await
}

pub fn spawn_prompt_validation_message(validation: &GoalObjectiveValidation) -> String {
    match validation.reason {
        Some(GoalObjectiveFailure::Empty) => "spawn_agent requires a non-empty prompt".to_owned(),
        Some(GoalObjectiveFailure::TooLong) | None => format!(
            "spawn_agent prompt too long ({} > {MAX_GOAL_OBJECTIVE_CHARS} chars)",
            validation
                .length
                .map_or_else(|| "undefined".to_owned(), |length| length.to_string())
        ),
    }
}

pub fn background_prompt_validation_message(validation: &GoalObjectiveValidation) -> String {
    match validation.reason {
        Some(GoalObjectiveFailure::Empty) => "Usage: /bg <prompt>".to_owned(),
        Some(GoalObjectiveFailure::TooLong) | None => format!(
            "Objective too long ({} > {MAX_GOAL_OBJECTIVE_CHARS} chars)",
            validation
                .length
                .map_or_else(|| "undefined".to_owned(), |length| length.to_string())
        ),
    }
}
