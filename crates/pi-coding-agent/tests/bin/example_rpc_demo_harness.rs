#[path = "../../examples/extensions/rpc_demo.rs"]
mod example;
use pi_coding_agent::core::{
    agent_session::AgentSession,
    agent_session_runtime::*,
    auth_storage::AuthStorage,
    extensions::*,
    model_registry::ModelRegistry,
    resource_loader::StaticResourceLoader,
    sdk::{CreateAgentSessionOptions, create_agent_session},
    session_manager::SessionManager,
};
use pi_js_compat::js_format;
use pi_js_compat::js_json::{self, JsString, JsValue, encode_node_utf8};
use std::sync::{Arc, Mutex};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let cwd_arg = std::env::args().nth(1).unwrap();
    let agent_path = std::path::Path::new(&cwd_arg).join("rust-agent");
    std::fs::create_dir_all(&agent_path).unwrap();
    let cwd = JsString::from(cwd_arg.as_str());
    let agent = JsString::from(agent_path.display().to_string());
    let slot: Arc<Mutex<Option<AgentSession>>> = Arc::default();
    let captured = slot.clone();
    let factory: ExtensionFactory = Arc::new(move |api| {
        let result = example::rpc_demo_extension(api);
        let captured = captured.clone();
        let registered = api.register_command(
            "fixture-event",
            Some("Private RPC event driver".into()),
            None,
            Arc::new(move |args, ctx| {
                let session = captured.lock().unwrap().clone().unwrap();
                Box::pin(async move {
                    let input = js_json::parse_lossless(&encode_node_utf8(&args)).unwrap();
                    let mut runner = session.extension_runner();
                    let ui = ctx.ui()?;
                    if input.get("noUi") == Some(&JsValue::Bool(true)) {
                        runner.set_ui_context(None, ExtensionMode::Rpc);
                    }
                    let result = if input
                        .get("type")
                        .and_then(JsValue::as_string)
                        .is_some_and(|kind| *kind == "tool_call")
                    {
                        let mut event = js_json::from_value(&input).unwrap();
                        js_json::to_value(&runner.emit_tool_call(&mut event).await?).unwrap()
                    } else {
                        let mut event = js_json::from_value(&input).unwrap();
                        match runner.emit(&mut event).await {
                            EmitResult::None => JsValue::Null,
                            EmitResult::SessionBeforeSwitch(result) => js_json::to_value(&result).unwrap(),
                            other => panic!("unsupported fixture result: {other:?}"),
                        }
                    };
                    runner.set_ui_context(Some(ui.clone()), ExtensionMode::Rpc);
                    ui.notify(
                        &js_format!("fixture result: {}", js_json::stringify_lossless(&result)),
                        Some(NotifyType::Info),
                    )?;
                    Ok(())
                })
            }),
        );
        Box::pin(async move {
            result?;
            registered.map_err(HandlerError::new)
        })
    });
    let runtime = create_extension_runtime();
    let mut registry = ExtensionRegistry::new();
    registry.register("rpc-demo", factory);
    let extension = load_extension_from_factory(
        registry.get(&"rpc-demo".into()).unwrap().clone(),
        &cwd,
        Arc::new(pi_coding_agent::core::event_bus::create_event_bus()),
        runtime.clone(),
        &"/fixture/rpc-demo".into(),
    )
    .await
    .unwrap();
    let loader = Arc::new(StaticResourceLoader {
        extensions: Some(LoadExtensionsResult {
            extensions: vec![extension],
            errors: vec![],
            runtime,
        }),
        ..Default::default()
    });
    let auth_storage = AuthStorage::in_memory(Vec::<(JsString, JsValue)>::new());
    let models = ModelRegistry::in_memory(auth_storage.clone()).unwrap();
    let settings = Arc::new(Mutex::new(
        pi_coding_agent::core::settings_manager::SettingsManager::in_memory(Default::default(), Default::default()),
    ));
    let services = AgentSessionServices {
        cwd: cwd.clone(),
        agent_dir: agent.clone(),
        model_registry: models.clone(),
        auth_storage: auth_storage.clone(),
        settings_manager: settings.clone(),
        resource_loader: loader.clone(),
        diagnostics: vec![],
    };
    let manager = SessionManager::in_memory(Some(&cwd), None).unwrap();
    let options = |manager, reason| CreateAgentSessionOptions {
        cwd: Some(cwd.clone()),
        agent_dir: Some(agent.clone()),
        model_registry: Some(models.clone()),
        resource_loader: Some(loader.clone()),
        settings_manager: Some(settings.clone()),
        session_manager: Some(manager),
        tools: Some(vec![]),
        disable_runtime_coordination_inbound: true,
        session_start_event: Some(SessionStartEvent {
            reason,
            previous_session_file: None,
        }),
        ..Default::default()
    };
    let initial = create_agent_session(options(manager, SessionStartReason::Startup))
        .await
        .unwrap();
    *slot.lock().unwrap() = Some(initial.session.clone());
    let services_next = services.clone();
    let factory: CreateAgentSessionRuntimeFactory = Arc::new(move |input| {
        let services = services_next.clone();
        let slot = slot.clone();
        Box::pin(async move {
            let result = create_agent_session(CreateAgentSessionOptions {
                cwd: Some(input.cwd),
                agent_dir: Some(input.agent_dir),
                model_registry: Some(services.model_registry.clone()),
                resource_loader: Some(services.resource_loader.clone()),
                settings_manager: Some(services.settings_manager.clone()),
                session_manager: Some(input.session_manager),
                tools: Some(vec![]),
                disable_runtime_coordination_inbound: true,
                session_start_event: input.session_start_event,
                ..Default::default()
            })
            .await
            .map_err(|error| error.message)?;
            *slot.lock().unwrap() = Some(result.session.clone());
            Ok(CreateAgentSessionRuntimeResult {
                session: result.session,
                services,
                diagnostics: vec![],
                model_fallback_message: result.model_fallback_message,
            })
        })
    });
    let runtime = AgentSessionRuntime::new(initial.session, services, factory, vec![], None);
    if let Err(error) = pi_coding_agent::modes::rpc::rpc_mode::run_rpc_mode(runtime).await {
        eprintln!("{}", encode_node_utf8(&error.message));
        std::process::exit(1);
    }
}
