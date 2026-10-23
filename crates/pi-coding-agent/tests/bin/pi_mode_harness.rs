//! Deterministic mode harness used by the print/RPC differential tests.
//!
//! Builds a real `AgentSessionRuntime` over the faux provider from environment
//! configuration, then runs the selected mode. stdout/stderr belong to the mode.

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
    CreateAgentSessionRuntimeResult, share_agent_session_runtime,
};
use pi_coding_agent::core::agent_session_services::{
    CreateAgentSessionFromServicesOptions, CreateAgentSessionServicesOptions, create_agent_session_from_services,
    create_agent_session_services,
};
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::model_registry::{ModelRegistry, ProviderConfigInput};
use pi_coding_agent::core::resource_loader::StaticResourceLoader;
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
        pi_ai::providers::faux::FauxContent::Text(String::new())
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

fn model_registry(handle: &FauxProviderHandle) -> ModelRegistry {
    let mut data = Vec::new();
    if std::env::var("PI_HARNESS_NO_AUTH").as_deref() != Ok("1") {
        data.push(("faux", js_json!({"type": "api_key", "key": "faux-key"})));
    }
    let registry = ModelRegistry::in_memory(AuthStorage::in_memory(data));
    let model = handle.get_model(None).expect("faux model");
    let provider = Arc::clone(&handle.provider);
    registry
        .register_provider(
            "faux",
            ProviderConfigInput {
                api: Some(model.api),
                base_url: Some(model.base_url),
                api_key: Some("faux-key".into()),
                models: Some(provider.get_models()),
                stream_simple: Some(Arc::new(move |model, context, options| {
                    provider.stream_simple(model, context, options.unwrap_or_default())
                })),
                ..Default::default()
            },
        )
        .expect("faux registration");
    registry
}

async fn build_runtime(
    handle: &FauxProviderHandle,
    cwd: &JsString,
    agent_dir: &JsString,
    session_dir: &JsString,
) -> std::sync::Arc<tokio::sync::Mutex<AgentSessionRuntime>> {
    let runtime = model_registry(handle);
    let model = handle.get_model(None).expect("faux model");
    let resource_loader: Arc<dyn pi_coding_agent::core::resource_loader::ResourceLoader> = StaticResourceLoader::new();
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
    let result = create_agent_session_from_services(CreateAgentSessionFromServicesOptions {
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
    .expect("session");

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
            let created = create_agent_session_from_services(CreateAgentSessionFromServicesOptions {
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
            .await?;
            Ok(CreateAgentSessionRuntimeResult {
                session: created.session,
                services,
                diagnostics: Vec::new(),
                model_fallback_message: None,
            })
        })
    });

    share_agent_session_runtime(AgentSessionRuntime::new(
        result.session,
        services,
        factory,
        Vec::new(),
        None,
    ))
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
    pi_coding_agent::modes::interactive::theme::init_theme(None, false).expect("theme");
    let runtime = build_runtime(&handle, &cwd, &agent_dir, &session_dir).await;
    if std::env::var("PI_HARNESS_RESUME").as_deref() == Ok("1") {
        runtime
            .lock()
            .await
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
        let runtime = runtime.lock().await;
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
                eprintln!("{}", encode_node_utf8(&error));
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
                initial_images: Vec::new(),
            };
            let exit_code = run_print_mode(runtime, options).await;
            std::process::exit(exit_code);
        }
        other => panic!("unsupported harness mode: {other}"),
    }
}
