//! Port of `packages/evals/src/pi-harness.ts`.
//!
//! Adapts a real `AgentSession` to the eval harness contract: resolves the
//! model selection, creates isolated workspace/agent/session directories,
//! runs prompt and reload steps, and returns transcript events plus usage.
//!
//! Rust carriers: [`PiCodingAgentHarnessOptions::model_runtime`] lets tests and
//! SDK callers inject an already-built runtime (the TypeScript source always
//! calls `ModelRuntime.create()`); [`PiCodingAgentHarnessOptions::output`] is
//! the same output transform as the source.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use pi_agent_core::agent::types::AgentMessage;
use pi_agent_core::harness::types::ThinkingLevel;
use pi_ai::request::BoxFuture;
use pi_ai::types::{AssistantContent, Model, ModelCostRates, StopReason, TextOrImageContent, UserContent};
use pi_ai::utils::text::{ContentText, content_text};
use pi_coding_agent::core::agent_session::{AgentSession, PromptOptions};
use pi_coding_agent::core::agent_session_services::{
    CreateAgentSessionFromServicesOptions, CreateAgentSessionServicesOptions, create_agent_session_from_services,
    create_agent_session_services,
};
use pi_coding_agent::core::model_runtime::{CreateModelRuntimeOptions, ModelRuntime};
use pi_coding_agent::core::resource_loader::DefaultResourceLoaderOptions;
use pi_coding_agent::core::sdk::NoTools;
use pi_coding_agent::core::session_manager::SessionManager;
use pi_coding_agent::core::settings_manager::{Settings, SettingsManager, SettingsManagerCreateOptions};
use pi_js_compat::process_env;
use pi_js_compat::string::js_trim;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tokio_util::sync::CancellationToken;

use crate::error::PiEvalError;
use crate::harness::{
    Harness, HarnessContext, HarnessRun, NormalizedError, NormalizedSession, TimingSummary, TranscriptEvent,
    TranscriptRole, UsageSummary,
};
use crate::vitest_evals::artifacts::PI_SESSION_SNAPSHOT_ARTIFACT;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelSelection {
    pub provider: String,
    pub id: String,
}

/// `PI_PROVIDER` / `PI_MODEL` defaults.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PiModelEnvironment {
    pub pi_provider: Option<String>,
    pub pi_model: Option<String>,
}

impl PiModelEnvironment {
    pub fn from_process() -> Self {
        Self {
            pi_provider: process_env::get("PI_PROVIDER"),
            pi_model: process_env::get("PI_MODEL"),
        }
    }
}

/// `resolveModelSelection`.
pub fn resolve_model_selection(
    explicit_model: Option<&ModelSelection>,
    environment: &PiModelEnvironment,
) -> Result<ModelSelection, PiEvalError> {
    let provider = explicit_model
        .map(|model| model.provider.clone())
        .or_else(|| environment.pi_provider.clone());
    let id = explicit_model
        .map(|model| model.id.clone())
        .or_else(|| environment.pi_model.clone());
    let provider = provider.as_deref().map(js_trim).unwrap_or_default();
    let id = id.as_deref().map(js_trim).unwrap_or_default();
    if provider.is_empty() || id.is_empty() {
        return Err(PiEvalError::error(
            "Select a harness model explicitly or set both PI_PROVIDER and PI_MODEL as defaults.",
        ));
    }
    Ok(ModelSelection {
        provider: provider.to_owned(),
        id: id.to_owned(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PiCodingAgentStep {
    Prompt { content: String },
    Reload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PiCodingAgentInput {
    Prompt(String),
    Steps(Vec<PiCodingAgentStep>),
}

impl PiCodingAgentInput {
    fn steps(self) -> Vec<PiCodingAgentStep> {
        match self {
            Self::Prompt(content) => vec![PiCodingAgentStep::Prompt { content }],
            Self::Steps(steps) => steps,
        }
    }
}

pub struct PiHarnessOutputArgs {
    pub response: String,
    pub session: AgentSession,
}

pub type PiHarnessOutputFn =
    Arc<dyn Fn(PiHarnessOutputArgs) -> BoxFuture<'static, Result<Value, PiEvalError>> + Send + Sync>;

pub type PiSystemPromptTransform = Arc<dyn Fn(&str) -> Result<String, PiEvalError> + Send + Sync>;

/// `PiCodingAgentHarnessOptions` plus the Rust runtime carrier.
#[derive(Clone, Default)]
pub struct PiCodingAgentHarnessOptions {
    pub name: Option<String>,
    pub model: Option<ModelSelection>,
    pub no_tools: Option<NoTools>,
    pub transform_system_prompt: Option<PiSystemPromptTransform>,
    pub output: Option<PiHarnessOutputFn>,
    /// Rust carrier for tests and SDK callers; defaults to `ModelRuntime::create()`.
    pub model_runtime: Option<ModelRuntime>,
}

struct PiHarnessResult {
    output: Value,
    events: Vec<TranscriptEvent>,
    usage: UsageSummary,
    timings: TimingSummary,
}

#[derive(Clone)]
pub struct PiCodingAgentHarness {
    pub name: String,
    options: PiCodingAgentHarnessOptions,
}

impl PiCodingAgentHarness {
    pub fn new(options: PiCodingAgentHarnessOptions) -> Self {
        Self {
            name: options.name.clone().unwrap_or_else(|| "pi-coding-agent".to_owned()),
            options,
        }
    }

    pub fn run(
        &self,
        input: PiCodingAgentInput,
        context: HarnessContext,
    ) -> BoxFuture<'static, Result<HarnessRun, PiEvalError>> {
        let options = self.options.clone();
        Box::pin(async move {
            let signal = context.signal.clone();
            let mut context = context;
            let result = run_pi_coding_agent(input, signal, &mut context, &options).await?;
            Ok(HarnessRun {
                output: result.output,
                session: NormalizedSession { events: result.events },
                usage: result.usage,
                timings: Some(result.timings),
                artifacts: (!context.artifacts.is_empty()).then_some(context.artifacts),
                errors: Vec::new(),
            })
        })
    }

    pub fn into_harness(self) -> Harness<PiCodingAgentInput> {
        let harness = self;
        Harness::new(harness.name.clone(), move |input, context| harness.run(input, context))
    }
}

/// `createPiCodingAgentHarness`.
pub fn create_pi_coding_agent_harness(options: PiCodingAgentHarnessOptions) -> PiCodingAgentHarness {
    PiCodingAgentHarness::new(options)
}

fn stop_reason_label(reason: StopReason) -> &'static str {
    match reason {
        StopReason::Pending => "pending",
        StopReason::Stop => "stop",
        StopReason::Length => "length",
        StopReason::ToolUse => "toolUse",
        StopReason::Error => "error",
        StopReason::Aborted => "aborted",
        StopReason::Deferred => "deferred",
    }
}

fn throw_if_aborted(signal: Option<&CancellationToken>) -> Result<(), PiEvalError> {
    if signal.is_some_and(CancellationToken::is_cancelled) {
        return Err(PiEvalError::Abort("This operation was aborted".to_owned()));
    }
    Ok(())
}

fn user_message_text(content: &UserContent) -> String {
    match content {
        UserContent::Text(text) => text.clone(),
        UserContent::Blocks(blocks) => content_text(ContentText::TextOrImage(blocks), "\n"),
    }
}

/// `toTranscriptEvents`.
fn to_transcript_events(messages: &[AgentMessage]) -> Vec<TranscriptEvent> {
    let mut events = Vec::new();
    for message in messages {
        match message {
            AgentMessage::User(user) => events.push(TranscriptEvent::Message {
                role: TranscriptRole::User,
                content: Some(Value::String(user_message_text(&user.content))),
                metadata: None,
            }),
            AgentMessage::Assistant(assistant) => {
                let text = content_text(ContentText::Assistant(&assistant.content), "\n");
                if !text.is_empty() {
                    events.push(TranscriptEvent::Message {
                        role: TranscriptRole::Assistant,
                        content: Some(Value::String(text)),
                        metadata: None,
                    });
                }
                for part in &assistant.content {
                    if let AssistantContent::ToolCall(call) = part {
                        events.push(TranscriptEvent::ToolCall {
                            id: call.id.clone(),
                            name: call.name.clone(),
                            arguments: Some(call.arguments.clone()),
                            started_at: None,
                            finished_at: None,
                            duration_ms: None,
                            metadata: None,
                        });
                    }
                }
            }
            AgentMessage::ToolResult(result) => {
                let text = content_text(ContentText::TextOrImage(&result.content), "\n");
                let all_text = result
                    .content
                    .iter()
                    .all(|part| matches!(part, TextOrImageContent::Text(_)));
                let content = if all_text {
                    Value::String(text.clone())
                } else {
                    serde_json::to_value(&result.content).expect("tool result content must serialize to JSON")
                };
                let error = result.is_error.then(|| {
                    NormalizedError::new(if text.is_empty() {
                        "Tool failed".to_owned()
                    } else {
                        text
                    })
                });
                events.push(TranscriptEvent::ToolResult {
                    tool_call_id: result.tool_call_id.clone(),
                    name: Some(result.tool_name.clone()),
                    content: Some(content),
                    error,
                    started_at: None,
                    finished_at: None,
                    duration_ms: None,
                    metadata: None,
                });
            }
            AgentMessage::Custom(_) => {}
        }
    }
    events
}

/// `promptAgent`.
async fn prompt_agent(
    session: &AgentSession,
    input: &str,
    signal: Option<&CancellationToken>,
) -> Result<String, PiEvalError> {
    throw_if_aborted(signal)?;
    let previous_message_count = session.messages().len();
    session
        .prompt(input, PromptOptions::default())
        .await
        .map_err(|error| PiEvalError::error(error.to_string()))?;
    let messages = session.messages();
    let start = previous_message_count.min(messages.len());
    let assistant = messages[start..].iter().rev().find_map(|message| match message {
        AgentMessage::Assistant(assistant) => Some(assistant),
        _ => None,
    });
    let Some(assistant) = assistant else {
        return Err(PiEvalError::error("Agent run completed without an assistant message."));
    };
    if assistant.stop_reason != StopReason::Stop {
        return Err(PiEvalError::error(assistant.error_message.clone().unwrap_or_else(
            || {
                format!(
                    "Agent run ended with unexpected stop reason: {}.",
                    stop_reason_label(assistant.stop_reason)
                )
            },
        )));
    }
    session
        .get_last_assistant_text()
        .filter(|output| !output.is_empty())
        .ok_or_else(|| PiEvalError::error("Agent run produced no assistant text."))
}

fn has_pricing(rates: &ModelCostRates) -> bool {
    rates.input > 0.0 || rates.output > 0.0 || rates.cache_read > 0.0 || rates.cache_write > 0.0
}

fn model_has_pricing(model: &Model) -> bool {
    has_pricing(&model.cost.rates)
        || model
            .cost
            .tiers
            .as_ref()
            .is_some_and(|tiers| tiers.iter().any(|tier| has_pricing(&tier.rates)))
}

async fn run_pi_coding_agent(
    input: PiCodingAgentInput,
    signal: Option<CancellationToken>,
    context: &mut HarnessContext,
    options: &PiCodingAgentHarnessOptions,
) -> Result<PiHarnessResult, PiEvalError> {
    let started_at = Instant::now();
    throw_if_aborted(signal.as_ref())?;
    let selection = resolve_model_selection(options.model.as_ref(), &PiModelEnvironment::from_process())?;
    let model_runtime = match options.model_runtime.clone() {
        Some(runtime) => runtime,
        None => ModelRuntime::create(CreateModelRuntimeOptions::default())
            .await
            .map_err(|error| PiEvalError::error(error.to_string()))?,
    };
    let Some(model) = model_runtime.get_model(&selection.provider, &selection.id) else {
        return Err(PiEvalError::error(format!(
            "Eval model not found: {}/{}",
            selection.provider, selection.id
        )));
    };

    let root = tempfile::Builder::new()
        .prefix("pi-eval-")
        .tempdir_in(std::env::temp_dir())
        .map_err(|error| PiEvalError::error(error.to_string()))?;
    let cwd = root.path().join("workspace");
    let agent_dir = root.path().join("agent");
    let mut session_slot: Option<AgentSession> = None;
    let mut snapshot_manager_slot: Option<SessionManager> = None;
    let mut abort_slot: Option<(
        CancellationToken,
        tokio::task::JoinHandle<Result<(), pi_coding_agent::core::agent_session::AgentSessionError>>,
    )> = None;
    let outcome: Result<PiHarnessResult, PiEvalError> = async {
        tokio::fs::create_dir(&cwd)
            .await
            .map_err(|error| PiEvalError::error(error.to_string()))?;
        tokio::fs::create_dir(&agent_dir)
            .await
            .map_err(|error| PiEvalError::error(error.to_string()))?;

        let transformed_system_prompt: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let resource_loader_options = options.transform_system_prompt.as_ref().map(|_| {
            let transformed = Arc::clone(&transformed_system_prompt);
            DefaultResourceLoaderOptions {
                system_prompt_override: Some(Arc::new(move |_base: Option<&str>| {
                    transformed
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .clone()
                })),
                ..Default::default()
            }
        });
        let services = create_agent_session_services(CreateAgentSessionServicesOptions {
            cwd: cwd.display().to_string(),
            agent_dir: Some(agent_dir.display().to_string()),
            settings_manager: Some(SettingsManager::in_memory(
                Settings::default(),
                SettingsManagerCreateOptions::default(),
            )),
            model_runtime: Some(model_runtime),
            model_runtime_signal: signal.clone(),
            extension_flag_values: None,
            resource_loader_options,
            resource_loader: None,
            resource_loader_reload_options: None,
        })
        .await
        .map_err(PiEvalError::error)?;
        throw_if_aborted(signal.as_ref())?;

        let session_manager = SessionManager::create(
            &cwd.display().to_string(),
            Some(&root.path().join("sessions").display().to_string()),
            None,
        )
        .map_err(|error| PiEvalError::error(error.to_string()))?;
        snapshot_manager_slot = Some(session_manager.clone());
        context.set_artifact("runId", Value::String(session_manager.get_session_id().to_owned()));
        let session = create_agent_session_from_services(CreateAgentSessionFromServicesOptions {
            services,
            session_manager,
            model: Some(model.clone()),
            thinking_level: Some(ThinkingLevel::Off),
            scoped_models: Vec::new(),
            tools: None,
            exclude_tools: None,
            no_tools: options.no_tools,
            permission_prompt_tool: None,
            multi_agent_runtime_role: None,
            multi_agent_execution_capability: None,
            custom_tools: Vec::new(),
            base_tools: Vec::new(),
            multi_agent_store: None,
            on_shared_channel_message_delivered: None,
            session_start_event: None,
        })
        .await
        .map_err(PiEvalError::error)?
        .session;
        session_slot = Some(session.clone());

        if let Some(transform) = options.transform_system_prompt.as_ref() {
            let transformed = transform(&session.system_prompt())?;
            if js_trim(&transformed).is_empty() {
                return Err(PiEvalError::error("Transformed eval system prompt must not be empty."));
            }
            *transformed_system_prompt
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(transformed);
            session
                .reload(None)
                .await
                .map_err(|error| PiEvalError::error(error.to_string()))?;
        }

        abort_slot = signal.as_ref().map(|token| {
            let token = token.clone();
            let session = session.clone();
            (
                token.clone(),
                tokio::spawn(async move {
                    token.cancelled().await;
                    session.abort().await
                }),
            )
        });

        throw_if_aborted(signal.as_ref())?;
        if !session.extension_runner().get_extension_paths().is_empty() {
            return Err(PiEvalError::error(
                "Expected an isolated eval session to start without extensions.",
            ));
        }
        let mut response: Option<String> = None;
        for step in input.steps() {
            match step {
                PiCodingAgentStep::Prompt { content } => {
                    response = Some(prompt_agent(&session, &content, signal.as_ref()).await?);
                }
                PiCodingAgentStep::Reload => session
                    .reload(None)
                    .await
                    .map_err(|error| PiEvalError::error(error.to_string()))?,
            }
        }
        let Some(response) = response else {
            return Err(PiEvalError::error(
                "Pi eval input must include at least one prompt step.",
            ));
        };
        let output = match options.output.as_ref() {
            None => Value::String(response),
            Some(output) => {
                output(PiHarnessOutputArgs {
                    response,
                    session: session.clone(),
                })
                .await?
            }
        };
        let stats = session.get_session_stats();
        let mut metadata = Map::new();
        metadata.insert(
            "cacheReadTokens".to_owned(),
            pi_js_compat::number::js_number_value(stats.tokens.cache_read),
        );
        metadata.insert(
            "cacheWriteTokens".to_owned(),
            pi_js_compat::number::js_number_value(stats.tokens.cache_write),
        );
        if model_has_pricing(&model) {
            metadata.insert(
                "estimatedCostUsd".to_owned(),
                pi_js_compat::number::js_number_value(stats.cost),
            );
        }
        Ok(PiHarnessResult {
            output,
            events: to_transcript_events(&session.messages()),
            usage: UsageSummary {
                provider: Some(model.provider.clone()),
                model: Some(model.id.clone()),
                input_tokens: Some(stats.tokens.input),
                output_tokens: Some(stats.tokens.output),
                reasoning_tokens: None,
                total_tokens: Some(stats.tokens.total),
                tool_calls: Some(stats.tool_calls as f64),
                retries: None,
                metadata: Some(metadata),
            },
            timings: TimingSummary {
                total_ms: Some(started_at.elapsed().as_secs_f64() * 1000.0),
                metadata: None,
            },
        })
    }
    .await;

    if let Some((token, handle)) = abort_slot {
        if token.is_cancelled() {
            let result = handle.await.map_err(|error| PiEvalError::error(error.to_string()))?;
            result.map_err(|error| PiEvalError::error(error.to_string()))?;
        } else {
            handle.abort();
        }
    }

    let mut cleanup_errors = Vec::new();
    if let Some(session_snapshot_manager) = snapshot_manager_slot.as_ref()
        && let Some(session_file) = session_snapshot_manager.get_session_file()
        && Path::new(session_file).exists()
    {
        match tokio::fs::read_to_string(session_file).await {
            Ok(snapshot) => context.set_artifact(PI_SESSION_SNAPSHOT_ARTIFACT, Value::String(snapshot)),
            Err(error) => cleanup_errors.push(PiEvalError::error(error.to_string())),
        }
    }
    if let Some(session) = session_slot.as_ref() {
        session.dispose();
    }
    if let Err(error) = root.close() {
        cleanup_errors.push(PiEvalError::error(error.to_string()));
    }

    match outcome {
        Err(error) => {
            if cleanup_errors.is_empty() {
                return Err(error);
            }
            let mut errors = vec![error];
            errors.extend(cleanup_errors);
            Err(PiEvalError::aggregate(
                "Agent run failed and cleanup also failed.",
                errors,
            ))
        }
        Ok(result) => match cleanup_errors.len() {
            0 => Ok(result),
            1 => Err(cleanup_errors.remove(0)),
            _ => Err(PiEvalError::aggregate("Agent cleanup failed.", cleanup_errors)),
        },
    }
}
