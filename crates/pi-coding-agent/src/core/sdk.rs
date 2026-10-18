//! Port of `packages/coding-agent/src/core/sdk.ts`.
//!
//! Default resource discovery and tool factories are owned by parallel slices;
//! callers must inject a resource loader and base tools until those land. See
//! `docs/porting/traceability/sdk.md`.

use std::path::Path;
use std::sync::{Arc, Mutex};

use pi_agent_core::agent::types::{AgentMessage, ConvertToLlm, StreamFn};
use pi_agent_core::agent::{Agent, AgentInitialState, AgentOptions};
use pi_agent_core::harness::messages::convert_to_llm;
use pi_agent_core::harness::types::ThinkingLevel;
use pi_ai::request::{CallbackError, SimpleStreamOptions};
use pi_ai::types::{Message, Model, ProviderHeaders, TextContent, TextOrImageContent, UserContent};
use pi_ai::utils::event_stream::StreamFailure;
use serde_json::Value;

use crate::config::get_agent_dir;
use crate::core::agent_session::{
    AgentSession, AgentSessionConfig, MultiAgentExecutionCapability, MultiAgentRuntimeRole, ToolDefinition,
};
use crate::core::auth_guidance::format_no_models_available_message;
use crate::core::defaults::DEFAULT_THINKING_LEVEL;
use crate::core::model_resolver::{FindInitialModelOptions, find_initial_model};
use crate::core::model_runtime::{CreateModelRuntimeOptions, ModelRuntime};
use crate::core::provider_attribution::merge_provider_attribution_headers;
use crate::core::resource_loader::ResourceLoader;
use crate::core::session_manager::{SessionManager, get_default_session_dir};
use crate::core::settings_manager::SettingsManager;
use crate::core::tools::DEFAULT_ACTIVE_TOOL_NAMES;
use crate::utils::paths::{PathInputOptions, resolve_path};

/// `noTools` option.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoTools {
    All,
    Builtin,
}

/// `CreateAgentSessionOptions`.
pub struct CreateAgentSessionOptions {
    pub cwd: Option<String>,
    pub agent_dir: Option<String>,
    pub model_runtime: Option<ModelRuntime>,
    pub model: Option<Model>,
    pub thinking_level: Option<ThinkingLevel>,
    pub scoped_models: Vec<crate::core::model_resolver::ScopedModel>,
    pub no_tools: Option<NoTools>,
    pub tools: Option<Vec<String>>,
    pub exclude_tools: Option<Vec<String>>,
    pub permission_prompt_tool: Option<String>,
    pub custom_tools: Vec<ToolDefinition>,
    /// Rust carrier for the unported `createAllToolDefinitions` default.
    pub base_tools: Vec<Arc<dyn pi_agent_core::agent::types::AgentTool>>,
    pub resource_loader: Option<Arc<dyn ResourceLoader>>,
    pub session_manager: Option<SessionManager>,
    pub settings_manager: Option<Arc<Mutex<SettingsManager>>>,
    pub control_db_path: Option<String>,
    pub multi_agent_runtime_role: Option<MultiAgentRuntimeRole>,
    pub multi_agent_execution_capability: Option<MultiAgentExecutionCapability>,
}

impl Default for CreateAgentSessionOptions {
    fn default() -> Self {
        Self {
            cwd: None,
            agent_dir: None,
            model_runtime: None,
            model: None,
            thinking_level: None,
            scoped_models: Vec::new(),
            no_tools: None,
            tools: None,
            exclude_tools: None,
            permission_prompt_tool: None,
            custom_tools: Vec::new(),
            base_tools: Vec::new(),
            resource_loader: None,
            session_manager: None,
            settings_manager: None,
            control_db_path: None,
            multi_agent_runtime_role: None,
            multi_agent_execution_capability: None,
        }
    }
}

/// `CreateAgentSessionResult`.
pub struct CreateAgentSessionResult {
    pub session: AgentSession,
    /// Placeholder for `LoadExtensionsResult` (owned by `ca-extensions-api`).
    pub extensions_result: (),
    pub model_fallback_message: Option<String>,
}

/// Creates an [`AgentSession`] with the supplied options.
pub async fn create_agent_session(options: CreateAgentSessionOptions) -> Result<CreateAgentSessionResult, String> {
    create_agent_session_internal(options).await
}

/// Internal-options entry point mirrored from the source.
pub async fn create_agent_session_with_internal_options(
    options: CreateAgentSessionOptions,
) -> Result<CreateAgentSessionResult, String> {
    create_agent_session_internal(options).await
}

fn resolve(input: &str) -> String {
    resolve_path(
        input,
        &std::env::current_dir()
            .map(|path| path.display().to_string())
            .unwrap_or_default(),
        &PathInputOptions::default(),
    )
    .unwrap_or_else(|_| input.to_owned())
}

async fn create_agent_session_internal(options: CreateAgentSessionOptions) -> Result<CreateAgentSessionResult, String> {
    let cwd = resolve(
        &options
            .cwd
            .clone()
            .or_else(|| {
                options
                    .session_manager
                    .as_ref()
                    .map(|manager| manager.get_cwd().to_owned())
            })
            .unwrap_or_else(|| {
                std::env::current_dir()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default()
            }),
    );
    let agent_dir = options
        .agent_dir
        .as_ref()
        .map(|dir| resolve(dir))
        .unwrap_or_else(|| get_agent_dir().display().to_string());

    let auth_path = options
        .agent_dir
        .as_ref()
        .map(|_| Path::new(&agent_dir).join("auth.json").display().to_string());
    let models_path = options
        .agent_dir
        .as_ref()
        .map(|_| Path::new(&agent_dir).join("models.json").display().to_string());
    let model_runtime = match options.model_runtime {
        Some(runtime) => runtime,
        None => ModelRuntime::create(CreateModelRuntimeOptions {
            auth_path,
            models_path: models_path.map(Some),
            ..Default::default()
        })
        .await
        .map_err(|error| error.to_string())?,
    };

    let settings_manager = options.settings_manager.unwrap_or_else(|| {
        Arc::new(Mutex::new(
            SettingsManager::create(&cwd, Some(Path::new(&agent_dir)), Default::default())
                .expect("settings manager must initialize"),
        ))
    });
    let session_manager = match options.session_manager {
        Some(manager) => manager,
        None => {
            let session_dir = get_default_session_dir(&cwd, Some(&agent_dir)).map_err(|error| error.to_string())?;
            SessionManager::create(&cwd, Some(&session_dir), None).map_err(|error| error.to_string())?
        }
    };
    let resource_loader = options
        .resource_loader
        .clone()
        .ok_or_else(|| "DefaultResourceLoader is not ported; pass resourceLoader".to_owned())?;

    let existing_session = session_manager.build_session_context();
    let has_existing_session = !existing_session.messages.is_empty();

    let mut model = options.model.clone();
    let mut model_fallback_message: Option<String> = None;

    if model.is_none() {
        if let Some(saved_model) = existing_session.model.as_ref() {
            let restored = model_runtime.get_model(&saved_model.provider, &saved_model.model_id);
            if let Some(restored) = restored {
                if model_runtime.has_configured_auth(&restored.provider) {
                    model = Some(restored);
                }
            }
            if model.is_none() {
                model_fallback_message = Some(format!(
                    "Could not restore model {}/{}",
                    saved_model.provider, saved_model.model_id
                ));
            }
        }
    }

    if model.is_none() {
        let initial_model_options = {
            let settings = lock(&settings_manager);
            FindInitialModelOptions {
                scoped_models: Vec::new(),
                is_continuing: has_existing_session,
                default_provider: settings.get_default_provider().map(str::to_owned),
                default_model_id: settings.get_default_model().map(str::to_owned),
                default_thinking_level: settings.get_default_thinking_level(),
                ..Default::default()
            }
        };
        let result = find_initial_model(&initial_model_options, &model_runtime).await?;
        model = result.model;
        match &model {
            Some(model) => {
                if let Some(message) = model_fallback_message.as_mut() {
                    message.push_str(&format!(". Using {}/{}", model.provider, model.id));
                }
            }
            None => model_fallback_message = Some(format_no_models_available_message()),
        }
    }

    let Some(model) = model else {
        return Err(format_no_models_available_message());
    };

    let default_thinking_level = lock(&settings_manager).get_default_thinking_level();
    let mut thinking_level = options
        .thinking_level
        .or(default_thinking_level)
        .unwrap_or(DEFAULT_THINKING_LEVEL);
    thinking_level = thinking_level_from_model(pi_ai::models::clamp_thinking_level(
        &model,
        model_thinking_level(thinking_level),
    ));

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
            .map(|name| (*name).to_owned())
            .collect(),
    }
    .into_iter()
    .filter(|name| !excluded_set.as_ref().is_some_and(|set| set.contains(name)))
    .collect();

    let settings = Arc::clone(&settings_manager);
    let session_manager = Arc::new(Mutex::new(session_manager));
    let session_ref: Arc<Mutex<Option<Arc<dyn Fn(pi_ai::types::ProviderRetryEvent) + Send + Sync>>>> =
        Arc::new(Mutex::new(None));
    let session_id = lock(&session_manager).get_session_id().to_owned();

    let convert_to_llm_with_block_images: ConvertToLlm = {
        let settings = Arc::clone(&settings);
        Arc::new(move |messages: Vec<AgentMessage>| {
            let settings = Arc::clone(&settings);
            Box::pin(async move {
                let converted = convert_to_llm(&messages);
                if !lock(&settings).get_block_images() {
                    return Ok(converted);
                }
                Ok(converted.into_iter().map(block_images).collect())
            })
        })
    };

    let stream_fn: StreamFn = {
        let settings = Arc::clone(&settings);
        let model_runtime = model_runtime.clone();
        let session_ref = Arc::clone(&session_ref);
        let session_id = session_id.clone();
        Arc::new(move |model: Model, context, mut options: SimpleStreamOptions| {
            let settings = Arc::clone(&settings);
            let model_runtime = model_runtime.clone();
            let session_ref = Arc::clone(&session_ref);
            let session_id = session_id.clone();
            Box::pin(async move {
                let provider_retry = lock(&settings).get_provider_retry_settings();
                let http_idle_timeout_ms = match lock(&settings).get_http_idle_timeout_ms() {
                    Ok(timeout) => timeout,
                    Err(error) => {
                        let (sender, stream) = pi_ai::utils::event_stream::create_assistant_message_event_stream();
                        sender.fail(StreamFailure::new(error.message().to_owned()));
                        return stream;
                    }
                };
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
                let websocket_connect_timeout_ms = options.stream.websocket_connect_timeout_ms.or_else(|| {
                    lock(&settings)
                        .get_websocket_connect_timeout_ms()
                        .ok()
                        .flatten()
                        .map(|timeout| timeout as f64)
                });
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
                let model_for_headers = model.clone();
                let settings_for_headers = Arc::clone(&settings);
                let session_id_for_headers = options.stream.session_id.clone();
                options.stream.request.transform_headers = Some(Arc::new(move |request_headers: ProviderHeaders| {
                    let model = model_for_headers.clone();
                    let settings = Arc::clone(&settings_for_headers);
                    let session_id = session_id_for_headers.clone();
                    Box::pin(async move {
                        let model_headers = model.headers.as_ref().map(|headers| {
                            headers
                                .iter()
                                .map(|(key, value)| (key.clone(), Some(value.clone())))
                                .collect::<ProviderHeaders>()
                        });
                        let merged = merge_provider_attribution_headers(
                            &model,
                            &lock(&settings),
                            session_id.as_deref(),
                            &[model_headers, Some(request_headers)],
                        )
                        .unwrap_or_default();
                        Ok(merged)
                    })
                }));
                let session_for_retry = Arc::clone(&session_ref);
                options.stream.on_retry = Some(Arc::new(
                    move |event: pi_ai::types::ProviderRetryEvent, _model: &Model| {
                        let notifier = lock(&session_for_retry).clone();
                        if let Some(notifier) = notifier {
                            notifier(event);
                        }
                    },
                ));
                let _ = session_id;
                model_runtime.stream_simple(model, context, options)
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
    let agent = Agent::new(AgentOptions {
        initial_state: Some(AgentInitialState {
            system_prompt: Some(String::new()),
            model: Some(model),
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
        ..Default::default()
    })
    .map_err(|error| error.message().to_owned())?;

    let mut restored_messages = existing_session.messages.clone();
    if has_existing_session && !restored_messages.is_empty() {
        let agent_messages = agent.messages();
        if agent_messages.is_empty() {
            agent.set_messages(std::mem::take(&mut restored_messages));
        }
    }

    let session = AgentSession::new(AgentSessionConfig {
        agent,
        session_manager,
        settings_manager: settings,
        cwd,
        scoped_models: options.scoped_models,
        resource_loader,
        model_runtime,
        initial_active_tool_names: Some(initial_active_tool_names),
        allowed_tool_names,
        excluded_tool_names,
        permission_prompt_tool: options.permission_prompt_tool,
        agent_dir: Some(agent_dir),
        control_db_path: options.control_db_path,
        base_tools: options.base_tools,
        custom_tools: options.custom_tools,
        multi_agent_runtime_role: options.multi_agent_runtime_role,
        multi_agent_execution_capability: options.multi_agent_execution_capability,
        thinking_phase_timeout_ms: None,
        extension_runner_ref: None,
        session_start_event: None,
    })
    .map_err(|error| error.message().to_owned())?;
    let notifier_session = session.clone();
    *lock(&session_ref) = Some(Arc::new(move |event: pi_ai::types::ProviderRetryEvent| {
        notifier_session.notify_provider_retry(event);
    }));

    Ok(CreateAgentSessionResult {
        session,
        extensions_result: (),
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
                let mut filtered: Vec<TextOrImageContent> = Vec::new();
                for block in blocks {
                    match block {
                        TextOrImageContent::Image(_) => {
                            push_placeholder(&mut filtered);
                        }
                        TextOrImageContent::Text(text) => {
                            if text.text == "Image reading is disabled." {
                                push_placeholder(&mut filtered);
                            } else {
                                filtered.push(TextOrImageContent::Text(text.clone()));
                            }
                        }
                    }
                }
                user.content = UserContent::Blocks(filtered);
            }
            Message::User(user)
        }
        Message::ToolResult(mut result) => {
            if result
                .content
                .iter()
                .any(|block| matches!(block, TextOrImageContent::Image(_)))
            {
                let mut filtered: Vec<TextOrImageContent> = Vec::new();
                for block in result.content {
                    match block {
                        TextOrImageContent::Image(_) => push_placeholder(&mut filtered),
                        TextOrImageContent::Text(text) => {
                            if text.text == "Image reading is disabled." {
                                push_placeholder(&mut filtered);
                            } else {
                                filtered.push(TextOrImageContent::Text(text));
                            }
                        }
                    }
                }
                result.content = filtered;
            }
            Message::ToolResult(result)
        }
        other => other,
    }
}

fn push_placeholder(blocks: &mut Vec<TextOrImageContent>) {
    let is_consecutive = blocks.last().is_some_and(
        |block| matches!(block, TextOrImageContent::Text(text) if text.text == "Image reading is disabled."),
    );
    if !is_consecutive {
        blocks.push(TextOrImageContent::Text(TextContent::new("Image reading is disabled.")));
    }
}

fn thinking_level_from_model(level: pi_ai::types::ModelThinkingLevel) -> ThinkingLevel {
    match level {
        pi_ai::types::ModelThinkingLevel::Off => ThinkingLevel::Off,
        pi_ai::types::ModelThinkingLevel::Minimal => ThinkingLevel::Minimal,
        pi_ai::types::ModelThinkingLevel::Low => ThinkingLevel::Low,
        pi_ai::types::ModelThinkingLevel::Medium => ThinkingLevel::Medium,
        pi_ai::types::ModelThinkingLevel::High => ThinkingLevel::High,
        pi_ai::types::ModelThinkingLevel::Xhigh => ThinkingLevel::XHigh,
        pi_ai::types::ModelThinkingLevel::Max => ThinkingLevel::Max,
        pi_ai::types::ModelThinkingLevel::Ultra => ThinkingLevel::Ultra,
    }
}

fn model_thinking_level(level: ThinkingLevel) -> pi_ai::types::ModelThinkingLevel {
    match level {
        ThinkingLevel::Off => pi_ai::types::ModelThinkingLevel::Off,
        ThinkingLevel::Minimal => pi_ai::types::ModelThinkingLevel::Minimal,
        ThinkingLevel::Low => pi_ai::types::ModelThinkingLevel::Low,
        ThinkingLevel::Medium => pi_ai::types::ModelThinkingLevel::Medium,
        ThinkingLevel::High => pi_ai::types::ModelThinkingLevel::High,
        ThinkingLevel::XHigh => pi_ai::types::ModelThinkingLevel::Xhigh,
        ThinkingLevel::Max => pi_ai::types::ModelThinkingLevel::Max,
        ThinkingLevel::Ultra => pi_ai::types::ModelThinkingLevel::Ultra,
    }
}

/// Marker so partial-port imports stay visible during review.
#[allow(dead_code)]
fn _unused_markers() {
    let _ = CallbackError::new("");
    let _: Option<Value> = None;
}
