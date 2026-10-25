//! Port of `packages/coding-agent/src/core/sdk.ts`.
//!
//! See `docs/porting/traceability/sdk.md`.

use std::sync::{Arc, Mutex};

use crate::core::messages::convert_to_llm;
use pi_agent_core::agent::types::{AgentError, AgentMessage, ConvertToLlm, StreamFn, TransformContext};
use pi_agent_core::agent::{Agent, AgentInitialState, AgentOptions};
use pi_agent_core::harness::types::ThinkingLevel;
use pi_ai::request::SimpleStreamOptions;
use pi_ai::types::{Message, Model, TextContent, TextOrImageContent, UserContent};
use pi_js_compat::js_json::JsString;

use crate::config::get_agent_dir;
use crate::core::agent_session::{
    AgentSession, AgentSessionConfig, MultiAgentExecutionCapability, MultiAgentRuntimeRole, ToolDefinition,
    model_thinking_level, thinking_level_from_model,
};
use crate::core::auth_guidance::format_no_models_available_message;
use crate::core::auth_storage::AuthStorage;
use crate::core::defaults::DEFAULT_THINKING_LEVEL;
use crate::core::extensions::runner::ExtensionRunner;
use crate::core::extensions::types::{AfterProviderResponseEvent, ExtensionEvent};
use crate::core::model_registry::{ModelRegistry, ResolvedRequestAuth};
use crate::core::model_resolver::{FindInitialModelOptions, find_initial_model};
use crate::core::resource_loader::{
    DefaultResourceLoader, DefaultResourceLoaderOptions, InlineExtension, ResourceLoader, RulesScope,
};
use crate::core::session_manager::{SessionManager, get_default_session_dir};
use crate::core::settings_manager::SettingsManager;
use crate::core::tools::DEFAULT_ACTIVE_TOOL_NAMES;
use crate::utils::paths::{PathInputOptions, node_join, resolve_path};

/// `noTools` option.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoTools {
    All,
    Builtin,
}

/// Provider retry notifier captured by the stream wrapper.
type RetryNotifier = Arc<dyn Fn(pi_ai::types::ProviderRetryEvent) + Send + Sync>;

/// `CreateAgentSessionOptions`.
#[derive(Default)]
pub struct CreateAgentSessionOptions {
    pub no_supervisor: Option<bool>,
    pub cwd: Option<JsString>,
    pub agent_dir: Option<JsString>,
    pub model_registry: Option<ModelRegistry>,
    pub auth_storage: Option<AuthStorage>,
    pub model: Option<Model>,
    pub thinking_level: Option<ThinkingLevel>,
    pub scoped_models: Vec<crate::core::model_resolver::ScopedModel>,
    pub no_tools: Option<NoTools>,
    pub tools: Option<Vec<String>>,
    pub exclude_tools: Option<Vec<String>>,
    pub permission_prompt_tool: Option<String>,
    pub custom_tools: Vec<ToolDefinition>,
    /// Rust carrier for SDK custom definitions executed with the runner context.
    pub custom_tool_definitions: Vec<crate::core::extensions::types::ToolDefinition>,
    /// Override the built-in tool definitions.
    pub base_tools: Vec<Arc<dyn pi_agent_core::agent::types::AgentTool>>,
    pub resource_loader: Option<Arc<dyn ResourceLoader>>,
    /// Override runtime-role rule discovery when creating the default loader.
    pub rules_scope: Option<RulesScope>,
    /// Inline extension factories loaded by the default loader.
    pub extension_factories: Vec<InlineExtension>,
    pub session_manager: Option<SessionManager>,
    pub settings_manager: Option<Arc<Mutex<SettingsManager>>>,
    pub control_db_path: Option<JsString>,
    pub multi_agent_store: Option<Arc<crate::core::multi_agent_store::MultiAgentStore>>,
    pub multi_agent_runtime_role: Option<MultiAgentRuntimeRole>,
    pub multi_agent_execution_capability: Option<MultiAgentExecutionCapability>,
    /// Current multi-agent runtime agent identity, when resuming or running as a child agent.
    pub multi_agent_agent_id: Option<JsString>,
    /// Parent runtime session ID for supervisor-directed messages from attached agents.
    pub multi_agent_parent_session_id: Option<JsString>,
    /// Whether this runtime must have an explicit multi-agent identity to send agent messages.
    pub multi_agent_requires_agent_id: bool,
    pub disable_runtime_coordination_inbound: bool,
    pub on_shared_channel_message_delivered: Option<Arc<dyn Fn(JsString) + Send + Sync>>,
    pub session_start_event: Option<crate::core::extensions::types::SessionStartEvent>,
    pub supervisor_decision_requester: Option<crate::core::agent_session::SupervisorDecisionRequester>,
}

/// `CreateAgentSessionResult`.
pub struct CreateAgentSessionResult {
    pub session: AgentSession,
    pub extensions_result: crate::core::extensions::types::LoadExtensionsResult,
    pub model_fallback_message: Option<JsString>,
}

fn rules_scope_for_runtime_role(role: Option<MultiAgentRuntimeRole>) -> RulesScope {
    match role {
        Some(MultiAgentRuntimeRole::Child) => RulesScope::Child,
        Some(MultiAgentRuntimeRole::Observer) => RulesScope::Shared,
        _ => RulesScope::Main,
    }
}

/// Creates an [`AgentSession`] with the supplied options.
pub async fn create_agent_session(
    options: CreateAgentSessionOptions,
) -> Result<CreateAgentSessionResult, pi_ai::request::CallbackError> {
    create_agent_session_internal(options).await
}

/// Internal-options entry point mirrored from the source.
pub async fn create_agent_session_with_internal_options(
    options: CreateAgentSessionOptions,
) -> Result<CreateAgentSessionResult, pi_ai::request::CallbackError> {
    create_agent_session_internal(options).await
}

fn read_process_cwd() -> Result<JsString, pi_ai::request::CallbackError> {
    pi_js_compat::process_cwd::cwd()
        .map_err(|error| pi_ai::request::CallbackError::from_error(error.message.clone(), error))
}

async fn create_agent_session_internal(
    options: CreateAgentSessionOptions,
) -> Result<CreateAgentSessionResult, pi_ai::request::CallbackError> {
    crate::core::agent_session::assert_standalone_worker_options(
        &crate::core::agent_session::StandaloneWorkerOptions {
            no_supervisor: options.no_supervisor.unwrap_or(false),
            session_is_persisted: options
                .session_manager
                .as_ref()
                .is_some_and(SessionManager::is_persisted),
            has_multi_agent_store: options.multi_agent_store.is_some(),
            multi_agent_runtime_role: options.multi_agent_runtime_role,
            has_execution_capability: options.multi_agent_execution_capability.is_some(),
            multi_agent_agent_id: options.multi_agent_agent_id.as_ref(),
            multi_agent_parent_session_id: options.multi_agent_parent_session_id.as_ref(),
            multi_agent_requires_agent_id: options.multi_agent_requires_agent_id,
        },
    )
    .map_err(|error| error.message().clone())?;
    let process_cwd = read_process_cwd()?;
    let cwd_input = options
        .cwd
        .clone()
        .or_else(|| {
            options
                .session_manager
                .as_ref()
                .map(|manager| manager.get_cwd().to_owned())
        })
        .unwrap_or_else(|| process_cwd.clone());
    let cwd = resolve_path(&cwd_input, &process_cwd, &PathInputOptions::default())
        .map_err(pi_ai::request::CallbackError::from)?;
    let agent_dir = match options.agent_dir.as_ref() {
        Some(dir) => resolve_path(dir, &process_cwd, &PathInputOptions::default())
            .map_err(pi_ai::request::CallbackError::from)?,
        None => get_agent_dir().map_err(pi_ai::request::CallbackError::from)?,
    };

    let auth_path = options.agent_dir.as_ref().map(|_| node_join(&agent_dir, "auth.json"));
    let models_path = options.agent_dir.as_ref().map(|_| node_join(&agent_dir, "models.json"));
    let auth_storage = options
        .auth_storage
        .map(Ok)
        .unwrap_or_else(|| AuthStorage::create(auth_path.as_ref()))
        .map_err(pi_ai::request::CallbackError::from)?;
    let model_registry = options
        .model_registry
        .map(Ok)
        .unwrap_or_else(|| ModelRegistry::create(auth_storage, models_path))?;

    let settings_manager = match options.settings_manager {
        Some(manager) => manager,
        None => Arc::new(Mutex::new(
            SettingsManager::create(&cwd, Some(&agent_dir), Default::default())
                .map_err(pi_ai::request::CallbackError::from)?,
        )),
    };
    let session_manager = match options.session_manager {
        Some(manager) => manager,
        None if options.no_supervisor == Some(true) => {
            SessionManager::in_memory(Some(&cwd), None).map_err(pi_ai::request::CallbackError::from)?
        }
        None => {
            let session_dir =
                get_default_session_dir(&cwd, Some(&agent_dir)).map_err(pi_ai::request::CallbackError::from)?;
            SessionManager::create(&cwd, Some(&session_dir), None).map_err(pi_ai::request::CallbackError::from)?
        }
    };
    let persisted_settings = session_manager
        .read_persisted_session_settings()
        .map_err(pi_ai::request::CallbackError::from)?;
    lock(&settings_manager).set_session_sandbox_profile(
        persisted_settings
            .as_ref()
            .and_then(|settings| settings.sandbox_profile),
    );
    let resource_loader = match options.resource_loader.clone() {
        Some(loader) => loader,
        None => {
            let loader = Arc::new(DefaultResourceLoader::new(DefaultResourceLoaderOptions {
                cwd: cwd.clone(),
                agent_dir: agent_dir.clone(),
                settings_manager: Some(Arc::clone(&settings_manager)),
                extension_factories: options.extension_factories.clone(),
                rules_scope: Some(
                    options
                        .rules_scope
                        .unwrap_or_else(|| rules_scope_for_runtime_role(options.multi_agent_runtime_role)),
                ),
                ..Default::default()
            })?);
            loader.reload().await?;
            loader
        }
    };

    let (existing_session, restored_identities) = session_manager.build_session_context_with_identities();
    let has_existing_session = !existing_session.messages.is_empty();

    let mut model = options.model.clone();
    let mut model_fallback_message: Option<JsString> = None;

    let saved_model = persisted_settings
        .as_ref()
        .and_then(|settings| settings.model.as_ref())
        .map(|model| (model.provider.clone(), model.model_id.clone()))
        .or_else(|| {
            existing_session
                .model
                .as_ref()
                .map(|model| (model.provider.clone(), model.model_id.clone()))
        });
    if model.is_none()
        && let Some((saved_provider, saved_model_id)) = saved_model
    {
        let restored = model_registry.find(&saved_provider, &saved_model_id);
        if let Some(restored) = restored
            && model_registry.has_configured_auth(&restored)?
        {
            model = Some(restored);
        }
        if model.is_none() {
            model_fallback_message = Some(pi_js_compat::js_format!(
                "Could not restore model {}/{}",
                &saved_provider,
                &saved_model_id
            ));
        }
    }

    if model.is_none() {
        let initial_model_options = {
            let settings = lock(&settings_manager);
            FindInitialModelOptions {
                scoped_models: Vec::new(),
                is_continuing: has_existing_session,
                default_provider: settings
                    .get_default_provider()
                    .map(|value| {
                        value
                            .to_utf8()
                            .map_err(|_| JsString::from("Default provider ID contains unpaired UTF-16 units"))
                    })
                    .transpose()?,
                default_model_id: settings
                    .get_default_model()
                    .map(|value| {
                        value
                            .to_utf8()
                            .map_err(|_| JsString::from("Default model ID contains unpaired UTF-16 units"))
                    })
                    .transpose()?,
                default_thinking_level: settings.get_default_thinking_level(),
                ..Default::default()
            }
        };
        let result = find_initial_model(&initial_model_options, &model_registry).await?;
        model = result.model;
        match &model {
            Some(model) => {
                if let Some(message) = model_fallback_message.as_mut() {
                    *message = message.clone() + pi_js_compat::js_format!(". Using {}/{}", &model.provider, &model.id);
                }
            }
            None => {
                model_fallback_message = Some(
                    format_no_models_available_message()
                        .map_err(pi_ai::request::CallbackError::from)?
                        .into(),
                )
            }
        }
    }

    let default_thinking_level = lock(&settings_manager).get_default_thinking_level();
    let persisted_thinking_level = persisted_settings
        .as_ref()
        .and_then(|settings| settings.thinking_level.as_ref())
        .and_then(|level| serde_json::from_value::<ThinkingLevel>(serde_json::json!(level)).ok());
    let mut thinking_level = options
        .thinking_level
        .or(persisted_thinking_level)
        .or(default_thinking_level)
        .unwrap_or(DEFAULT_THINKING_LEVEL);
    thinking_level = match &model {
        Some(model) => thinking_level_from_model(pi_ai::models::clamp_thinking_level(
            model,
            model_thinking_level(thinking_level),
        )),
        None => ThinkingLevel::Off,
    };

    let allowed_tool_names = options
        .tools
        .clone()
        .or_else(|| (options.no_tools == Some(NoTools::All)).then(Vec::new));
    let excluded_tool_names = options.exclude_tools.clone();
    let excluded_set: Option<std::collections::HashSet<String>> = excluded_tool_names
        .as_ref()
        .map(|names| names.iter().cloned().collect());
    let initial_active_tool_names: Vec<String> = match &options.tools {
        Some(tools) => tools.clone(),
        None if options.no_tools.is_some() => Vec::new(),
        None => DEFAULT_ACTIVE_TOOL_NAMES
            .iter()
            .map(|name| name.as_str().to_owned())
            .collect(),
    }
    .into_iter()
    .filter(|name| !excluded_set.as_ref().is_some_and(|set| set.contains(name)))
    .collect();

    let settings = Arc::clone(&settings_manager);
    let session_manager = Arc::new(Mutex::new(session_manager));
    let session_ref: Arc<Mutex<Option<RetryNotifier>>> = Arc::new(Mutex::new(None));
    let session_id = lock(&session_manager).get_session_id().to_owned();

    let convert_to_llm_with_block_images: ConvertToLlm = {
        let settings = Arc::clone(&settings);
        Arc::new(move |messages: Vec<AgentMessage>| {
            let settings = Arc::clone(&settings);
            Box::pin(async move {
                let converted = convert_to_llm(&messages)?;
                if !lock(&settings).get_block_images() {
                    return Ok(converted);
                }
                Ok(converted.into_iter().map(block_images).collect())
            })
        })
    };

    let extension_runner_ref: Arc<Mutex<Option<ExtensionRunner>>> = Arc::new(Mutex::new(None));

    let stream_fn: StreamFn = {
        let settings = Arc::clone(&settings);
        let model_registry = model_registry.clone();
        let cwd = cwd.clone();
        let session_ref = Arc::clone(&session_ref);
        let extension_runner_ref = Arc::clone(&extension_runner_ref);
        Arc::new(move |model: Model, context, mut options: SimpleStreamOptions| {
            let settings = Arc::clone(&settings);
            let model_registry = model_registry.clone();
            let cwd = cwd.clone();
            let session_ref = Arc::clone(&session_ref);
            let extension_runner_ref = Arc::clone(&extension_runner_ref);
            Box::pin(async move {
                let auth = model_registry
                    .get_api_key_and_headers(&model)
                    .await
                    .map_err(AgentError::from)?;
                let (api_key, headers, env) = match auth {
                    ResolvedRequestAuth::Ok { api_key, headers, env } => (api_key, headers, env),
                    ResolvedRequestAuth::Err { error } => return Err(AgentError::new(error)),
                };
                let env = match (env, options.stream.request.env.take()) {
                    (None, None) => None,
                    (auth, request) => Some(
                        auth.into_iter()
                            .flatten()
                            .chain(request.into_iter().flatten())
                            .collect(),
                    ),
                };
                options.stream.request.api_key = api_key;
                options.stream.request.env = env;
                options.stream.request.headers = crate::core::provider_attribution::merge_provider_attribution_headers(
                    &model,
                    &lock(&settings),
                    options.stream.session_id.as_ref(),
                    &[headers, options.stream.request.headers.take()],
                );
                options.stream.cwd = Some(cwd);
                let provider_retry = lock(&settings).get_provider_retry_settings();
                let http_idle_timeout_ms = lock(&settings)
                    .get_http_idle_timeout_ms()
                    .map_err(|error| AgentError::new(error.message().clone()))?;
                let effective_timeout_ms = if http_idle_timeout_ms == 0 {
                    2_147_483_647
                } else {
                    http_idle_timeout_ms
                };
                let timeout_ms = options
                    .stream
                    .request
                    .timeout_ms
                    .or(provider_retry.timeout_ms)
                    .unwrap_or(effective_timeout_ms as f64);
                let websocket_connect_timeout_ms = match options.stream.websocket_connect_timeout_ms {
                    Some(timeout) => Some(timeout),
                    None => lock(&settings)
                        .get_websocket_connect_timeout_ms()
                        .map_err(|error| AgentError::new(error.message().clone()))?
                        .map(|timeout| timeout as f64),
                };
                options.stream.request.timeout_ms = Some(timeout_ms);
                options.stream.websocket_connect_timeout_ms = websocket_connect_timeout_ms;
                options.stream.request.max_retries = options.stream.request.max_retries.or(provider_retry.max_retries);
                options.stream.request.max_retry_delay_ms = Some(
                    options
                        .stream
                        .request
                        .max_retry_delay_ms
                        .unwrap_or(provider_retry.max_retry_delay_ms),
                );
                let runner_for_payload = Arc::clone(&extension_runner_ref);
                options.stream.request.on_payload = Some(Arc::new(
                    move |payload: pi_js_compat::js_json::JsValue, _model: Model| {
                        let runner = lock(&runner_for_payload).clone();
                        Box::pin(async move {
                            match runner {
                                Some(runner) if runner.has_handlers("before_provider_request") => {
                                    Ok(Some(runner.emit_before_provider_request(payload.into()).await))
                                }
                                _ => Ok(None),
                            }
                        })
                    },
                ));
                let runner_for_response = Arc::clone(&extension_runner_ref);
                options.stream.request.on_response = Some(Arc::new(
                    move |response: pi_ai::types::ProviderResponse, _model: Model| {
                        let runner = lock(&runner_for_response).clone();
                        Box::pin(async move {
                            if let Some(runner) = runner
                                && runner.has_handlers("after_provider_response")
                            {
                                let mut event = ExtensionEvent::AfterProviderResponse(AfterProviderResponseEvent {
                                    status: response.status as u16,
                                    headers: response.headers,
                                });
                                runner.emit(&mut event).await;
                            }
                            Ok(())
                        })
                    },
                ));
                let session_for_retry = Arc::clone(&session_ref);
                options.stream.on_retry = Some(Arc::new(
                    move |event: pi_ai::types::ProviderRetryEvent, _model: &Model| {
                        let notifier = lock(&session_for_retry).clone();
                        if let Some(notifier) = notifier {
                            notifier(event);
                        }
                        Ok(())
                    },
                ));
                pi_ai::compat::stream_simple(model, context, options).map_err(AgentError::from)
            })
        })
    };

    let steering_mode = lock(&settings).get_steering_mode();
    let follow_up_mode = lock(&settings).get_follow_up_mode();
    let transport = lock(&settings).get_transport();
    let thinking_budgets = lock(&settings)
        .get_thinking_budgets()
        .and_then(|budgets| serde_json::to_value(budgets).ok())
        .and_then(|value| serde_json::from_value(value).ok());
    let max_retry_delay_ms = lock(&settings).get_provider_retry_settings().max_retry_delay_ms;
    let transform_context: TransformContext = {
        let extension_runner_ref = Arc::clone(&extension_runner_ref);
        Arc::new(move |messages, _signal| {
            let runner = lock(&extension_runner_ref).clone();
            Box::pin(async move {
                match runner {
                    Some(runner) => runner
                        .emit_context(messages)
                        .await
                        .map_err(|error| AgentError::new(error.message)),
                    None => Ok(messages),
                }
            })
        })
    };
    let agent = Agent::new(AgentOptions {
        initial_state: Some(AgentInitialState {
            system_prompt: Some(Default::default()),
            model: model.clone(),
            thinking_level: Some(thinking_level),
            tools: Some(Vec::new()),
            messages: None,
        }),
        convert_to_llm: Some(convert_to_llm_with_block_images),
        stream_fn: Some(stream_fn),
        session_id: Some(session_id),
        steering_mode: Some(steering_mode),
        follow_up_mode: Some(follow_up_mode),
        transport: Some(transport),
        thinking_budgets,
        max_retry_delay_ms: Some(max_retry_delay_ms),
        transform_context: Some(transform_context),
        ..Default::default()
    })
    .map_err(|error| JsString::from(error.message()))?;

    let mut restored_messages = existing_session.messages.clone();
    if has_existing_session && !restored_messages.is_empty() {
        let agent_messages = agent.messages();
        if agent_messages.is_empty() {
            agent
                .set_messages_with_identities(std::mem::take(&mut restored_messages), restored_identities)
                .map_err(|error| JsString::from(error.message()))?;
        }
    }

    let session = AgentSession::new(AgentSessionConfig {
        agent,
        session_manager,
        settings_manager: settings,
        cwd,
        scoped_models: options.scoped_models,
        resource_loader: Arc::clone(&resource_loader),
        model_registry,
        initial_active_tool_names: Some(initial_active_tool_names),
        allowed_tool_names,
        excluded_tool_names,
        permission_prompt_tool: options.permission_prompt_tool,
        agent_dir: Some(agent_dir),
        control_db_path: options.control_db_path,
        base_tools: options.base_tools,
        custom_tools: options.custom_tools,
        custom_tool_definitions: options.custom_tool_definitions,
        multi_agent_runtime_role: options.multi_agent_runtime_role,
        multi_agent_execution_capability: options.multi_agent_execution_capability,
        thinking_phase_timeout_ms: None,
        no_supervisor: options.no_supervisor,
        supervisor_decision_requester: options.supervisor_decision_requester,
        multi_agent_store: options.multi_agent_store,
        multi_agent_agent_id: options.multi_agent_agent_id,
        multi_agent_parent_session_id: options.multi_agent_parent_session_id,
        multi_agent_requires_agent_id: options.multi_agent_requires_agent_id,
        disable_runtime_coordination_inbound: options.disable_runtime_coordination_inbound,
        on_shared_channel_message_delivered: options.on_shared_channel_message_delivered,
        extension_runner_ref: Some(extension_runner_ref),
        session_start_event: options.session_start_event,
    })
    .map_err(pi_ai::request::CallbackError::from)?;
    let notifier_session = session.clone();
    *lock(&session_ref) = Some(Arc::new(move |event: pi_ai::types::ProviderRetryEvent| {
        notifier_session.notify_provider_retry(event);
    }));

    let extensions_result = resource_loader.get_extensions();
    Ok(CreateAgentSessionResult {
        session,
        extensions_result,
        model_fallback_message,
    })
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn block_images(message: Message) -> Message {
    match message {
        Message::User(mut user) => {
            if let UserContent::Blocks(blocks) = &user.content
                && blocks.iter().any(|block| matches!(block, TextOrImageContent::Image(_)))
            {
                user.content = UserContent::Blocks(block_image_blocks(blocks));
            }
            Message::User(user)
        }
        Message::ToolResult(mut result) => {
            if result
                .content
                .iter()
                .any(|block| matches!(block, TextOrImageContent::Image(_)))
            {
                result.content = block_image_blocks(&result.content);
            }
            Message::ToolResult(result)
        }
        other => other,
    }
}

fn block_image_blocks(blocks: &[TextOrImageContent]) -> Vec<TextOrImageContent> {
    let mut filtered: Vec<TextOrImageContent> = Vec::new();
    for block in blocks {
        match block {
            TextOrImageContent::Image(_) => push_placeholder(&mut filtered),
            TextOrImageContent::Text(text) if text.text == "Image reading is disabled." => {
                push_placeholder(&mut filtered);
            }
            TextOrImageContent::Text(text) => filtered.push(TextOrImageContent::Text(text.clone())),
        }
    }
    filtered
}

fn push_placeholder(blocks: &mut Vec<TextOrImageContent>) {
    let is_consecutive = blocks.last().is_some_and(
        |block| matches!(block, TextOrImageContent::Text(text) if text.text == "Image reading is disabled."),
    );
    if !is_consecutive {
        blocks.push(TextOrImageContent::Text(TextContent::new("Image reading is disabled.")));
    }
}
