//! Replace the active session and rebind session-local subscriptions.
mod error;
mod resources;
use pi_coding_agent::core::{
    agent_session::{AgentSession, AgentSessionEvent, ExtensionBindings},
    agent_session_runtime::{
        AgentSessionRuntime, CreateAgentSessionRuntimeFactory, CreateAgentSessionRuntimeOptions,
        CreateAgentSessionRuntimeResult, create_agent_session_runtime,
    },
    agent_session_services::{
        CreateAgentSessionFromServicesOptions, CreateAgentSessionServicesOptions, create_agent_session_from_services,
        create_agent_session_services,
    },
    extensions::types::SessionStartEvent,
    sdk::CreateAgentSessionOptions,
    session_manager::{SessionManager, get_default_session_dir},
    settings_manager::SettingsManager,
};
use pi_js_compat::js_json::JsString;
use std::io::Write;
use std::sync::{Arc, Mutex};

type Subscription = (AgentSession, u64, Arc<Mutex<Option<String>>>);

fn create_factory(options: CreateAgentSessionOptions) -> CreateAgentSessionRuntimeFactory {
    let runtime = options.model_runtime;
    let model = options.model;
    let loader = options.resource_loader;
    let agent_dir = options.agent_dir;
    let settings = options
        .settings_manager
        .map(|s| s.lock().expect("settings mutex poisoned").get_global_settings());
    Arc::new(move |request: CreateAgentSessionRuntimeOptions| {
        let runtime = runtime.clone();
        let model = model.clone();
        let loader = loader.clone();
        let agent_dir = agent_dir.clone();
        let settings = settings.clone();
        Box::pin(async move {
            let services = create_agent_session_services(CreateAgentSessionServicesOptions {
                cwd: request.cwd,
                agent_dir,
                model_runtime: runtime,
                settings_manager: settings.map(|s| SettingsManager::in_memory(s, Default::default())),
                model_runtime_signal: None,
                extension_flag_values: None,
                resource_loader_options: None,
                resource_loader: loader,
                resource_loader_reload_options: None,
            })
            .await?;
            let diagnostics = services.diagnostics.clone();
            let reason = serde_json::from_value(serde_json::Value::String(request.session_start_reason))
                .map_err(|e| e.to_string())?;
            let result = create_agent_session_from_services(CreateAgentSessionFromServicesOptions {
                services: services.clone(),
                session_manager: request.session_manager,
                model,
                thinking_level: None,
                scoped_models: Vec::new(),
                tools: None,
                exclude_tools: None,
                no_tools: None,
                permission_prompt_tool: None,
                multi_agent_runtime_role: None,
                multi_agent_execution_capability: None,
                custom_tools: Vec::new(),
                base_tools: Vec::new(),
                multi_agent_store: None,
                on_shared_channel_message_delivered: None,
                session_start_event: Some(SessionStartEvent {
                    reason,
                    previous_session_file: request.previous_session_file,
                }),
            })
            .await?;
            Ok(CreateAgentSessionRuntimeResult {
                session: result.session,
                services,
                diagnostics,
                model_fallback_message: result.model_fallback_message,
            })
        })
    })
}

pub async fn create_runtime(mut options: CreateAgentSessionOptions) -> Result<AgentSessionRuntime, String> {
    let paths = resources::options(&options)?;
    let manager = match options.session_manager.take() {
        Some(manager) => manager,
        None => {
            let dir = get_default_session_dir(&paths.cwd, Some(&paths.agent_dir)).map_err(|e| e.to_string())?;
            SessionManager::create(&paths.cwd, Some(&dir), None).map_err(|e| e.to_string())?
        }
    };
    create_agent_session_runtime(create_factory(options), &paths.cwd, &paths.agent_dir, manager).await
}

pub async fn bind_session<W: Write + Send + 'static>(
    runtime: &AgentSessionRuntime,
    previous: Option<Subscription>,
    output: Arc<Mutex<W>>,
) -> Subscription {
    if let Some((session, id, _)) = previous {
        session.unsubscribe(id);
    }
    let session = runtime.session().clone();
    session.bind_extensions(ExtensionBindings::default()).await;
    let failure = Arc::new(Mutex::new(None));
    let write_failure = Arc::clone(&failure);
    let id = session.subscribe(Arc::new(move |event| {
        if let AgentSessionEvent::QueueUpdate(event) = event {
            let result = writeln!(
                output.lock().expect("output mutex poisoned"),
                "Queued: {}",
                event.steering.len() + event.follow_up.len()
            );
            if let Err(error) = result {
                *write_failure.lock().expect("failure mutex poisoned") = Some(error.to_string());
            }
        }
    }));
    (session, id, failure)
}

pub async fn run(
    options: CreateAgentSessionOptions,
    output: impl Write + Send + 'static,
) -> Result<Vec<Option<String>>, String> {
    let mut runtime = create_runtime(options).await?;
    let output = Arc::new(Mutex::new(output));
    let mut subscription = bind_session(&runtime, None, Arc::clone(&output)).await;
    let original = subscription.0.session_file();
    writeln!(
        output.lock().expect("output mutex poisoned"),
        "Initial session: {}",
        original.as_ref().unwrap_or(&JsString::from("undefined"))
    )
    .map_err(|e| e.to_string())?;
    let mut files = vec![original.clone()];
    runtime.new_session(Default::default()).await?;
    if let Some(error) = subscription.2.lock().expect("failure mutex poisoned").take() {
        return Err(error);
    }
    subscription = bind_session(&runtime, Some(subscription), Arc::clone(&output)).await;
    files.push(subscription.0.session_file());
    writeln!(
        output.lock().expect("output mutex poisoned"),
        "After newSession(): {}",
        files[1].as_ref().unwrap_or(&JsString::from("undefined"))
    )
    .map_err(|e| e.to_string())?;
    if let Some(original) = original {
        runtime.switch_session(&original, None, None).await?;
        if let Some(error) = subscription.2.lock().expect("failure mutex poisoned").take() {
            return Err(error);
        }
        subscription = bind_session(&runtime, Some(subscription), Arc::clone(&output)).await;
        files.push(subscription.0.session_file());
        writeln!(
            output.lock().expect("output mutex poisoned"),
            "After switchSession(): {}",
            files[2].as_ref().unwrap_or(&JsString::from("undefined"))
        )
        .map_err(|e| e.to_string())?;
    }
    subscription.0.unsubscribe(subscription.1);
    runtime.dispose().await;
    Ok(files)
}
#[tokio::main(flavor = "current_thread")]
pub async fn main() -> Result<(), error::ExampleError> {
    run(CreateAgentSessionOptions::default(), std::io::stdout())
        .await
        .map(|_| ())
        .map_err(error::ExampleError)
}
