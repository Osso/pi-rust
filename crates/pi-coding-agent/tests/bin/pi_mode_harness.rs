//! Deterministic mode harness used by the print/RPC differential tests.
//!
//! Builds a real `AgentSessionRuntime` over the faux provider from environment
//! configuration, then runs the selected mode. stdout/stderr belong to the mode.

#[path = "support/print_fixture.rs"]
mod print_fixture;
#[path = "support/rpc_fixture.rs"]
mod rpc_fixture;

use std::sync::Arc;

use pi_agent_core::agent::types::{AgentTool, AgentToolResult, AgentToolUpdateCallback, ToolError};
use pi_agent_core::harness::types::ThinkingLevel;
use pi_ai::providers::faux::{
    FauxAssistantMessageOptions, FauxProviderHandle, FauxResponseStep, FauxTokenSize, RegisterFauxProviderOptions,
    faux_assistant_message, faux_provider, faux_tool_call,
};
use pi_ai::types::StopReason;
use pi_coding_agent::core::agent_session_runtime::{
    AgentSessionRuntime, CreateAgentSessionRuntimeFactory, CreateAgentSessionRuntimeOptions,
    CreateAgentSessionRuntimeResult,
};
use pi_coding_agent::core::agent_session_services::{
    CreateAgentSessionFromServicesOptions, CreateAgentSessionServicesOptions, create_agent_session_from_services,
    create_agent_session_services,
};
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::model_registry::{ModelRegistry, ProviderConfigInput};
use pi_coding_agent::core::session_manager::SessionManager;
use pi_coding_agent::core::tools::result::terminating_text_result;
use pi_coding_agent::modes::print_mode::{PrintMode, PrintModeOptions, run_print_mode};
use pi_coding_agent::modes::rpc::rpc_mode::run_rpc_mode;
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json::{JsObject, JsString, JsValue, encode_node_utf8, parse_lossless};
use pi_js_compat::{js_format, js_json};
use serde_json::Value;

/// `createEndTurnToolDefinition()` as an `AgentTool`, matching the source tool.
#[derive(Debug, Clone, Copy, Default)]
struct EndTurnAgentTool;

impl AgentTool for EndTurnAgentTool {
    fn name(&self) -> &str {
        "end_turn"
    }

    fn description(&self) -> pi_js_compat::js_json::JsString {
        "End the current model turn. Provide a concise reason explaining why the turn is finished.".into()
    }

    fn parameters(&self) -> &Value {
        pi_coding_agent::core::tools::end_turn::parameters()
    }

    fn label(&self) -> pi_js_compat::js_json::JsString {
        "end_turn".into()
    }

    fn execution_mode(&self) -> Option<pi_agent_core::harness::types::ToolExecutionMode> {
        Some(pi_agent_core::harness::types::ToolExecutionMode::Sequential)
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: JsValue,
        _signal: Option<AbortSignal>,
        _on_update: AgentToolUpdateCallback,
        _execution: Option<pi_agent_core::harness::types::AgentToolExecutionContext>,
    ) -> pi_ai::request::BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        Box::pin(async move {
            let reason = params
                .get("reason")
                .and_then(JsValue::as_string)
                .ok_or_else(|| ToolError::new("end_turn reason must be a non-empty string"))?;
            if reason.trim().is_empty() {
                return Err(ToolError::new("end_turn reason must be a non-empty string"));
            }
            Ok(terminating_text_result(
                js_format!("Turn ended: {}", reason),
                js_json!({ "reason": reason }),
            ))
        })
    }
}

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set"))
}

fn optional_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

fn parse_json_array(text: &str) -> Vec<JsValue> {
    match parse_lossless(text).expect("harness JSON") {
        JsValue::Array(values) => values,
        _ => panic!("harness JSON must be an array"),
    }
}

fn faux_handle(responses: &[JsValue]) -> FauxProviderHandle {
    let handle = faux_provider(RegisterFauxProviderOptions {
        api: Some("faux".to_owned()),
        token_size: Some(FauxTokenSize {
            min: Some(1.0),
            max: Some(1.0),
        }),
        ..RegisterFauxProviderOptions::default()
    });
    handle.set_responses(responses.iter().map(response_step).collect());
    handle
}

fn string_field<'a>(value: &'a JsValue, field: &str) -> Option<&'a JsString> {
    value.get(field).and_then(JsValue::as_string)
}

/// Opaque fixture ids/names stay scalar under the approved provider-id gap.
fn scalar_field(value: &JsValue, field: &str) -> String {
    string_field(value, field)
        .and_then(|text| text.to_utf8().ok())
        .unwrap_or_else(|| panic!("{field} must be a scalar string"))
}

fn response_step(value: &JsValue) -> FauxResponseStep {
    if value["pending"] == JsValue::Bool(true) {
        return FauxResponseStep::Factory(Arc::new(|_, options, _, _| {
            Box::pin(async move {
                let signal = options
                    .and_then(|options| options.stream.request.signal)
                    .expect("pending fixture signal");
                print_fixture::mark_pending();
                signal.wait().await;
                Ok(faux_assistant_message("Never delivered", Default::default()))
            })
        }));
    }
    if let Some(content) = value.get("content") {
        let blocks = pi_js_compat::js_json::from_value(content).expect("typed assistant fixture content");
        return faux_assistant_message(
            pi_ai::providers::faux::FauxContent::Blocks(blocks),
            FauxAssistantMessageOptions::default(),
        )
        .into();
    }
    if let Some(tool) = value.get("tool") {
        let arguments = tool
            .get("arguments")
            .and_then(JsValue::as_object)
            .cloned()
            .unwrap_or_default();
        return faux_assistant_message(
            faux_tool_call(scalar_field(tool, "name"), arguments, Some(scalar_field(tool, "id"))),
            FauxAssistantMessageOptions::default(),
        )
        .into();
    }
    let mut blocks = Vec::new();
    if let Some(text) = string_field(value, "text").filter(|text| !text.is_empty()) {
        blocks.push(pi_ai::types::AssistantContent::Text(pi_ai::types::TextContent::new(
            text,
        )));
    }
    if let Some(reason) = string_field(value, "endTurn") {
        let mut arguments = JsObject::new();
        arguments.insert("reason".into(), JsValue::String(reason.clone()));
        blocks.push(faux_tool_call(
            "end_turn",
            arguments,
            Some(scalar_field(value, "toolId")),
        ));
    }
    let stop_reason = string_field(value, "stopReason").map(|reason| match reason.to_utf8().as_deref() {
        Ok("error") => StopReason::Error,
        Ok("aborted") => StopReason::Aborted,
        Ok("length") => StopReason::Length,
        Ok("toolUse") => StopReason::ToolUse,
        _ => panic!("unsupported stop reason"),
    });
    let content = if blocks.is_empty() {
        pi_ai::providers::faux::FauxContent::Text(Default::default())
    } else {
        pi_ai::providers::faux::FauxContent::Blocks(blocks)
    };
    faux_assistant_message(
        content,
        FauxAssistantMessageOptions {
            stop_reason,
            error_message: string_field(value, "errorMessage").cloned(),
            ..FauxAssistantMessageOptions::default()
        },
    )
    .into()
}

fn capture_context(context: &pi_ai::types::Context) {
    use std::io::Write;
    if let Some(path) = optional_env("PI_HARNESS_CONTEXT_LOG") {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .expect("context capture");
        writeln!(
            file,
            "{}",
            pi_js_compat::js_json::stringify_lossless(&js_json!({"runtime":"rust","context":context}))
        )
        .expect("write context capture");
    }
}

fn done_error_fixture(model: pi_ai::types::Model) -> pi_ai::utils::event_stream::AssistantMessageEventStream {
    use pi_ai::types::{AssistantMessage, AssistantMessageEvent, DoneReason, StopReason, Usage};
    let (sender, stream) = pi_ai::utils::event_stream::create_assistant_message_event_stream();
    let mut message = AssistantMessage::empty(model.api, model.provider, model.id, 1700000000000.0);
    sender.push(AssistantMessageEvent::Start {
        partial: message.clone(),
    });
    message.stop_reason = StopReason::Error;
    message.usage = Usage {
        input: 11.0,
        output: 7.0,
        cache_read: 5.0,
        cache_write: 3.0,
        total_tokens: 26.0,
        ..Default::default()
    };
    sender.push(AssistantMessageEvent::Done {
        reason: DoneReason::Error,
        message,
    });
    sender.end(None);
    stream
}

fn model_registry(handle: &FauxProviderHandle) -> ModelRegistry {
    let mut data = Vec::new();
    if std::env::var("PI_HARNESS_NO_AUTH").as_deref() != Ok("1") {
        data.push(("faux", js_json!({"type": "api_key", "key": "faux-key"})));
    }
    let registry = ModelRegistry::in_memory(AuthStorage::in_memory(data)).unwrap();
    let model = handle.get_model(None).expect("faux model");
    let provider = Arc::clone(&handle.provider);
    registry
        .register_provider(
            "faux",
            ProviderConfigInput {
                api: Some(model.api),
                base_url: Some(model.base_url),
                api_key: Some("faux-key".into()),
                models: Some(provider.get_models().unwrap().into_iter().map(Into::into).collect()),
                stream_simple: Some(Arc::new(move |model, context, options| {
                    capture_context(&context);
                    if optional_env("PI_HARNESS_DONE_ERROR").as_deref() == Some("1") {
                        Ok(done_error_fixture(model))
                    } else {
                        provider.stream_simple(model, context, options.unwrap_or_default())
                    }
                })),
                ..Default::default()
            },
        )
        .expect("faux registration");
    registry
}

fn build_timed_session(
    services: &pi_coding_agent::core::agent_session_services::AgentSessionServices,
    session_manager: SessionManager,
    model: pi_ai::Model,
    custom_tools: Vec<pi_coding_agent::core::agent_session::ToolDefinition>,
    timeout: f64,
) -> pi_coding_agent::core::agent_session::AgentSession {
    use pi_coding_agent::core::agent_session::{AgentSession, AgentSessionConfig};
    let agent = pi_agent_core::agent::Agent::new(pi_agent_core::agent::AgentOptions {
        initial_state: Some(pi_agent_core::agent::AgentInitialState {
            model: Some(model),
            ..Default::default()
        }),
        get_api_key: Some(Arc::new(|_| Box::pin(async { Some("faux-key".into()) }))),
        ..Default::default()
    })
    .unwrap();
    AgentSession::new(AgentSessionConfig {
        no_supervisor: None,
        agent,
        session_manager: Arc::new(std::sync::Mutex::new(session_manager)),
        settings_manager: services.settings_manager.clone(),
        cwd: services.cwd.clone(),
        scoped_models: Vec::new(),
        resource_loader: services.resource_loader.clone(),
        model_registry: services.model_registry.clone(),
        initial_active_tool_names: Some(Vec::new()),
        allowed_tool_names: None,
        excluded_tool_names: None,
        permission_prompt_tool: None,
        agent_dir: Some(services.agent_dir.clone()),
        control_db_path: None,
        base_tools: Vec::new(),
        custom_tool_definitions: Vec::new(),
        custom_tools,
        multi_agent_runtime_role: None,
        multi_agent_execution_capability: None,
        thinking_phase_timeout_ms: Some(timeout),
        supervisor_decision_requester: None,
        multi_agent_store: None,
        multi_agent_agent_id: None,
        multi_agent_parent_session_id: None,
        multi_agent_requires_agent_id: false,
        disable_runtime_coordination_inbound: false,
        on_shared_channel_message_delivered: None,
        extension_runner_ref: None,
        session_start_event: None,
    })
    .unwrap()
}

async fn build_runtime(
    handle: &FauxProviderHandle,
    cwd: &JsString,
    agent_dir: &JsString,
    session_dir: &JsString,
) -> AgentSessionRuntime {
    let runtime = model_registry(handle);
    let model = handle.get_model(None).expect("faux model");
    let resource_loader = if std::env::var_os("PI_HARNESS_RPC_FIXTURE").is_some() {
        rpc_fixture::resource_loader(cwd).await
    } else {
        print_fixture::resource_loader(cwd).await
    };
    let services = create_agent_session_services(CreateAgentSessionServicesOptions {
        cwd: cwd.clone(),
        agent_dir: Some(agent_dir.clone()),
        settings_manager: None,
        model_registry: Some(runtime),
        auth_storage: None,
        extension_flag_values: None,
        resource_loader_options: None,
        resource_loader: Some(Arc::clone(&resource_loader)),
        resource_loader_reload_options: None,
    })
    .await
    .expect("services");
    let end_turn = pi_coding_agent::core::agent_session::ToolDefinition {
        name: "end_turn".to_owned(),
        description: "End the current model turn. Provide a concise reason explaining why the turn is finished."
            .into(),
        parameters: pi_coding_agent::core::tools::end_turn::parameters().clone(),
        prompt_snippet: Some("End the current model turn with a required reason".into()),
        prompt_guidelines: vec![
            "Call end_turn only when the task is complete, progress requires user input, or the user explicitly asks you to stop. If work remains and progress is possible, continue working instead of calling end_turn. Assistant text alone does not finish the turn.".into(),
            "Provide one concise, non-empty reason.".into(),
        ],
        approval_required: std::env::var("PI_HARNESS_APPROVAL").as_deref() == Ok("1"),
        approval_kind: None,
        tool: Arc::new(EndTurnAgentTool),
    };
    let tools = (Vec::new(), Vec::new(), vec![end_turn]);
    let session_manager = SessionManager::create(cwd, Some(session_dir), None).expect("session manager");
    let session = if let Some(timeout) = optional_env("PI_HARNESS_THINKING_TIMEOUT_MS") {
        build_timed_session(
            &services,
            session_manager,
            model.clone(),
            tools.2.clone(),
            timeout.parse().unwrap(),
        )
    } else {
        create_agent_session_from_services(CreateAgentSessionFromServicesOptions {
            no_supervisor: None,
            services: services.clone(),
            session_manager,
            model: Some(model.clone()),
            thinking_level: Some(ThinkingLevel::Off),
            scoped_models: Vec::new(),
            tools: Some(tools.0.clone()),
            exclude_tools: None,
            no_tools: None,
            permission_prompt_tool: None,
            multi_agent_runtime_role: None,
            multi_agent_execution_capability: None,
            custom_tools: tools.2.clone(),
            base_tools: tools.1.clone(),
            multi_agent_store: None,
            on_shared_channel_message_delivered: None,
            session_start_event: None,
        })
        .await
        .expect("session")
        .session
    };

    if let Some(messages) = optional_env("PI_HARNESS_INITIAL_MESSAGES") {
        let messages = parse_lossless(&messages).expect("initial message fixture JSON");
        session
            .agent()
            .set_messages(pi_js_compat::js_json::from_value(&messages).expect("initial message fixture"));
    }
    let factory_runtime = services.model_registry.clone();
    let factory_loader = resource_loader;
    let factory: CreateAgentSessionRuntimeFactory = Arc::new(move |options: CreateAgentSessionRuntimeOptions| {
        let runtime = factory_runtime.clone();
        let loader = Arc::clone(&factory_loader);
        let model = model.clone();
        let tools = tools.clone();
        Box::pin(async move {
            let services = create_agent_session_services(CreateAgentSessionServicesOptions {
                cwd: options.cwd,
                agent_dir: Some(options.agent_dir),
                settings_manager: None,
                model_registry: Some(runtime),
                auth_storage: None,
                extension_flag_values: None,
                resource_loader_options: None,
                resource_loader: Some(loader),
                resource_loader_reload_options: None,
            })
            .await?;
            let session = if let Some(timeout) = optional_env("PI_HARNESS_THINKING_TIMEOUT_MS") {
                build_timed_session(
                    &services,
                    options.session_manager,
                    model.clone(),
                    tools.2.clone(),
                    timeout.parse().unwrap(),
                )
            } else {
                create_agent_session_from_services(CreateAgentSessionFromServicesOptions {
                    no_supervisor: None,
                    services: services.clone(),
                    session_manager: options.session_manager,
                    model: Some(model),
                    thinking_level: Some(ThinkingLevel::Off),
                    scoped_models: Vec::new(),
                    tools: Some(tools.0.clone()),
                    exclude_tools: None,
                    no_tools: None,
                    permission_prompt_tool: None,
                    multi_agent_runtime_role: None,
                    multi_agent_execution_capability: None,
                    custom_tools: tools.2.clone(),
                    base_tools: tools.1.clone(),
                    multi_agent_store: None,
                    on_shared_channel_message_delivered: None,
                    session_start_event: None,
                })
                .await?
                .session
            };
            print_fixture::observe_replacement(&session);
            Ok(CreateAgentSessionRuntimeResult {
                session,
                services,
                diagnostics: Vec::new(),
                model_fallback_message: None,
            })
        })
    });

    AgentSessionRuntime::new(session, services, factory, Vec::new(), None)
}

async fn run_main_fixture(handle: &FauxProviderHandle) -> i32 {
    use pi_coding_agent::core::extensions::types::{ExtensionContext, HandlerError, SessionStartEvent};
    let provider: Arc<dyn pi_ai::models::Provider> = handle.provider.clone();
    let factory: pi_coding_agent::core::extensions::ExtensionFactory = Arc::new(move |api| {
        let provider = provider.clone();
        let models = provider.get_models().unwrap();
        let first = models.first().unwrap();
        let result = (|| {
            api.register_provider(
                provider.id(),
                ProviderConfigInput {
                    name: Some(provider.name().into()),
                    api: Some(first.api.clone()),
                    base_url: Some(first.base_url.clone()),
                    api_key: Some("fixture".into()),
                    models: Some(models.clone().into_iter().map(Into::into).collect()),
                    stream_simple: Some(Arc::new({
                        let provider = provider.clone();
                        move |model, context, options| {
                            provider.stream_simple(model, context, options.unwrap_or_default())
                        }
                    })),
                    ..Default::default()
                },
            )
            .map_err(HandlerError::from)?;
            api.on_session_start(|_:SessionStartEvent,context:ExtensionContext| {
                Box::pin(async move {
                    let manager=context.session_manager()?;
                    let persisted=manager.get_session_file().is_some();
                    let state=js_json!({"noSupervisor":context.no_supervisor()?,"persisted":persisted,"hasStore":context.multi_agent_store()?.is_some(),"hasDetach":context.tool_detach_registry()?.is_some()});
                    std::fs::write(env("PI_HARNESS_STANDALONE_STATE"),pi_js_compat::js_json::stringify_lossless(&state)).map_err(|error| HandlerError::new(error.to_string()))?;
                    Ok(None)
                }) as pi_ai::request::BoxFuture<'static,Result<Option<()>,HandlerError>>
            }).map_err(HandlerError::new)?;
            Ok(())
        })();
        Box::pin(async move { result })
    });
    pi_coding_agent::main_runner::run_main_or_report_error(
        &std::env::args().skip(1).collect::<Vec<_>>(),
        pi_coding_agent::main_runner::MainOptions {
            extension_factories: vec![pi_coding_agent::core::resource_loader::InlineExtension::Factory {
                extension_path: Some("<fixture:standalone-main>".into()),
                factory,
            }],
        },
    )
    .await
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let mode = env("PI_HARNESS_MODE");
    let cwd = JsString::from(env("PI_HARNESS_CWD"));
    let agent_dir = JsString::from(env("PI_HARNESS_AGENT_DIR"));
    let session_dir = JsString::from(env("PI_HARNESS_SESSION_DIR"));
    let responses = parse_json_array(&env("PI_HARNESS_RESPONSES"));
    let handle = faux_handle(&responses);
    if mode == "main" {
        std::process::exit(run_main_fixture(&handle).await);
    }
    pi_coding_agent::modes::interactive::theme::init_theme(None, false).expect("theme");
    let runtime = build_runtime(&handle, &cwd, &agent_dir, &session_dir).await;
    if std::env::var("PI_HARNESS_RESUME").as_deref() == Ok("1") {
        runtime
            .session()
            .agent()
            .set_messages(vec![pi_agent_core::agent::types::AgentMessage::User(
                pi_ai::types::UserMessage {
                    role: pi_ai::types::UserRole::Tag,
                    content: pi_ai::types::UserContent::Blocks(vec![pi_ai::types::TextOrImageContent::Text(
                        pi_ai::types::TextContent::new("resume me"),
                    )]),
                    input_source: None,
                    provider_native: None,
                    timestamp: 1.0,
                },
            )]);
    }
    if std::env::var("PI_HARNESS_DEBUG").is_ok() {
        let runtime = runtime.clone();
        eprintln!(
            "debug: tools={:?} pending={} prompt={:?}",
            runtime.session().get_active_tool_names(),
            handle.get_pending_response_count(),
            runtime.session().system_prompt()
        );
    }

    match mode.as_str() {
        "rpc" => {
            if let Err(error) = run_rpc_mode(runtime).await {
                eprintln!("{}", encode_node_utf8(&error.message));
                std::process::exit(1);
            }
        }
        "print-text" | "print-json" => {
            let prompts = parse_json_array(&optional_env("PI_HARNESS_PROMPTS").unwrap_or_else(|| "[]".to_owned()));
            let mut prompts = prompts
                .into_iter()
                .map(|prompt| prompt.as_string().cloned().expect("prompt string"));
            let initial_message = prompts.next();
            let options = PrintModeOptions {
                mode: if mode == "print-json" {
                    PrintMode::Json
                } else {
                    PrintMode::Text
                },
                messages: prompts.collect(),
                initial_message,
                initial_images: optional_env("PI_HARNESS_INITIAL_IMAGES")
                    .map(|images| {
                        pi_js_compat::js_json::from_value(&parse_lossless(&images).expect("initial images JSON"))
                            .expect("initial image fixtures")
                    })
                    .unwrap_or_default(),
            };
            let fixture = print_fixture::PrintFixture::configure(&runtime);
            let result = run_print_mode(runtime, options).await;
            if let Some(fixture) = fixture
                && fixture.capture()
            {
                match result {
                    Ok(code) => fixture.returned(code),
                    Err(error) => fixture.rejected(error),
                }
                std::process::exit(0);
            }
            let exit_code =
                result.unwrap_or_else(|error| pi_coding_agent::utils::uncaught::throw_callback_uncaught(error));
            std::process::exit(exit_code);
        }
        other => panic!("unsupported harness mode: {other}"),
    }
}
