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
    CreateAgentSessionRuntimeResult,
};
use pi_coding_agent::core::agent_session_services::{
    CreateAgentSessionFromServicesOptions, CreateAgentSessionServicesOptions, create_agent_session_from_services,
    create_agent_session_services,
};
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::model_runtime::{CreateModelRuntimeOptions, ModelRuntime};
use pi_coding_agent::core::resource_loader::StaticResourceLoader;
use pi_coding_agent::core::session_manager::SessionManager;
use pi_coding_agent::core::tools::result::terminating_text_result;
use pi_coding_agent::modes::print_mode::{PrintMode, PrintModeOptions, run_print_mode};
use pi_coding_agent::modes::rpc::rpc_mode::run_rpc_mode;
use serde_json::{Map, Value, json};
use tokio_util::sync::CancellationToken;

/// `createEndTurnToolDefinition()` as an `AgentTool`, matching the source tool.
#[derive(Debug, Clone, Copy, Default)]
struct EndTurnAgentTool;

impl AgentTool for EndTurnAgentTool {
    fn name(&self) -> &str {
        "end_turn"
    }

    fn description(&self) -> &str {
        "End the current model turn. Provide a concise reason explaining why the turn is finished."
    }

    fn parameters(&self) -> &Value {
        pi_coding_agent::core::tools::end_turn::parameters()
    }

    fn label(&self) -> &str {
        "end_turn"
    }

    fn execution_mode(&self) -> Option<pi_agent_core::harness::types::ToolExecutionMode> {
        Some(pi_agent_core::harness::types::ToolExecutionMode::Sequential)
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: Value,
        _signal: Option<CancellationToken>,
        _on_update: AgentToolUpdateCallback,
        _execution: Option<pi_agent_core::harness::types::AgentToolExecutionContext>,
    ) -> pi_ai::request::BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        Box::pin(async move {
            let reason = params
                .get("reason")
                .and_then(Value::as_str)
                .ok_or_else(|| ToolError::new("end_turn reason must be a non-empty string"))?;
            if pi_js_compat::string::js_trim(reason).is_empty() {
                return Err(ToolError::new("end_turn reason must be a non-empty string"));
            }
            Ok(terminating_text_result(
                format!("Turn ended: {reason}"),
                json!({ "reason": reason }),
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

fn faux_handle(responses: &[Value]) -> FauxProviderHandle {
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

fn response_step(value: &Value) -> FauxResponseStep {
    if let Some(tool) = value.get("tool") {
        let name = tool["name"].as_str().expect("tool name").to_owned();
        let id = tool["id"].as_str().expect("tool id").to_owned();
        let arguments = tool["arguments"].as_object().cloned().unwrap_or_else(Map::new);
        return faux_assistant_message(
            faux_tool_call(name, arguments, Some(id)),
            FauxAssistantMessageOptions::default(),
        )
        .into();
    }
    let mut blocks = Vec::new();
    if let Some(text) = value["text"].as_str().filter(|text| !text.is_empty()) {
        blocks.push(pi_ai::types::AssistantContent::Text(pi_ai::types::TextContent::new(
            text,
        )));
    }
    if let Some(reason) = value["endTurn"].as_str() {
        let id = value["toolId"].as_str().expect("endTurn toolId");
        let mut arguments = Map::new();
        arguments.insert("reason".to_owned(), Value::String(reason.to_owned()));
        blocks.push(faux_tool_call("end_turn", arguments, Some(id.to_owned())));
    }
    let text = value["text"].as_str().unwrap_or_default();
    let stop_reason = value["stopReason"].as_str().map(|reason| match reason {
        "error" => StopReason::Error,
        "aborted" => StopReason::Aborted,
        "length" => StopReason::Length,
        "toolUse" => StopReason::ToolUse,
        other => panic!("unsupported stop reason: {other}"),
    });
    let content = if blocks.is_empty() {
        pi_ai::providers::faux::FauxContent::Text(text.to_owned())
    } else {
        pi_ai::providers::faux::FauxContent::Blocks(blocks)
    };
    faux_assistant_message(
        content,
        FauxAssistantMessageOptions {
            stop_reason,
            error_message: value["errorMessage"].as_str().map(str::to_owned),
            ..FauxAssistantMessageOptions::default()
        },
    )
    .into()
}

async fn model_runtime(handle: &FauxProviderHandle) -> ModelRuntime {
    let mut data = Map::new();
    if std::env::var("PI_HARNESS_NO_AUTH").as_deref() != Ok("1") {
        data.insert("faux".to_owned(), json!({"type": "api_key", "key": "faux-key"}));
    }
    let runtime = ModelRuntime::create(CreateModelRuntimeOptions {
        credentials: Some(Arc::new(AuthStorage::in_memory(data))),
        models_path: Some(None),
        ..Default::default()
    })
    .await
    .expect("model runtime");
    runtime.register_native_provider(Arc::clone(&handle.provider));
    runtime
        .refresh(pi_ai::models::ModelsRefreshOptions {
            allow_network: Some(false),
            ..Default::default()
        })
        .await
        .expect("availability refresh");
    runtime
}

async fn build_runtime(
    handle: &FauxProviderHandle,
    cwd: &str,
    agent_dir: &str,
    session_dir: &str,
) -> AgentSessionRuntime {
    let runtime = model_runtime(handle).await;
    let model = handle.get_model(None).expect("faux model");
    let resource_loader: Arc<dyn pi_coding_agent::core::resource_loader::ResourceLoader> = StaticResourceLoader::new();
    let services = create_agent_session_services(CreateAgentSessionServicesOptions {
        cwd: cwd.to_owned(),
        agent_dir: Some(agent_dir.to_owned()),
        settings_manager: None,
        model_runtime: Some(runtime),
        resource_loader: Some(Arc::clone(&resource_loader)),
    })
    .await
    .expect("services");
    let end_turn = pi_coding_agent::core::agent_session::ToolDefinition {
        name: "end_turn".to_owned(),
        description: "End the current model turn. Provide a concise reason explaining why the turn is finished."
            .to_owned(),
        parameters: pi_coding_agent::core::tools::end_turn::parameters().clone(),
        prompt_snippet: Some("End the current model turn with a required reason".to_owned()),
        prompt_guidelines: vec![
            "Call end_turn only when the task is complete, progress requires user input, or the user explicitly asks you to stop. If work remains and progress is possible, continue working instead of calling end_turn. Assistant text alone does not finish the turn.".to_owned(),
            "Provide one concise, non-empty reason.".to_owned(),
        ],
        approval_required: std::env::var("PI_HARNESS_APPROVAL").as_deref() == Ok("1"),
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
        custom_tools: tools.2.clone(),
        base_tools: tools.1.clone(),
    })
    .await
    .expect("session");

    let factory_runtime = services.model_runtime.clone();
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
                model_runtime: Some(runtime),
                resource_loader: Some(loader),
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
                custom_tools: tools.2.clone(),
                base_tools: tools.1.clone(),
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

    AgentSessionRuntime::new(result.session, services, factory, Vec::new(), None)
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let mode = env("PI_HARNESS_MODE");
    let cwd = env("PI_HARNESS_CWD");
    let agent_dir = env("PI_HARNESS_AGENT_DIR");
    let session_dir = env("PI_HARNESS_SESSION_DIR");
    let responses: Vec<Value> = serde_json::from_str(&env("PI_HARNESS_RESPONSES")).expect("responses JSON");
    let handle = faux_handle(&responses);
    pi_coding_agent::modes::interactive::theme::init_theme(None, false);
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
                    timestamp: 1,
                },
            )]);
    }
    if std::env::var("PI_HARNESS_DEBUG").is_ok() {
        eprintln!(
            "debug: tools={:?} pending={} prompt={:?}",
            runtime.session().get_active_tool_names(),
            handle.get_pending_response_count(),
            runtime.session().system_prompt()
        );
    }

    match mode.as_str() {
        "rpc" => {
            run_rpc_mode(runtime).await;
        }
        "print-text" | "print-json" => {
            let prompts: Vec<String> =
                serde_json::from_str(&optional_env("PI_HARNESS_PROMPTS").unwrap_or_else(|| "[]".to_owned()))
                    .expect("prompts JSON");
            let mut prompts = prompts.into_iter();
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
