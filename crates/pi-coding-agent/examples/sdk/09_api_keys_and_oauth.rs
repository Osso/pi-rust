//! Default/custom credential locations, ephemeral keys and a built-in-only registry.
mod error;
use pi_ai::request::CallbackError;
use pi_coding_agent::core::{
    agent_session::AgentSession,
    auth_storage::AuthStorage,
    model_registry::ModelRegistry,
    sdk::{CreateAgentSessionOptions, create_agent_session},
    session_manager::SessionManager,
};
use std::io::Write;

pub async fn run(
    options: [CreateAgentSessionOptions; 4],
    mut output: impl Write,
) -> Result<[AgentSession; 4], CallbackError> {
    let [mut default, mut custom, mut overridden, mut built_in] = options;
    let registry = load_default_registry(&mut default)?;
    bind_registry_auth(&mut default, registry.clone());
    let first = print_session(
        default,
        "Session with default auth storage and model registry",
        &mut output,
    )
    .await?;
    let custom_registry = load_custom_registry(&mut custom)?;
    bind_registry_auth(&mut custom, custom_registry);
    let second = print_session(custom, "Session with custom auth storage location", &mut output).await?;
    registry
        .auth_storage()
        .set_runtime_api_key("anthropic", &"sk-my-temp-key".into());
    bind_registry_auth(&mut overridden, registry.clone());
    let third = print_session(overridden, "Session with runtime API key override", &mut output).await?;
    bind_registry_auth(
        &mut built_in,
        ModelRegistry::in_memory(registry.auth_storage().clone())?,
    );
    let fourth = print_session(built_in, "Session with only built-in models", &mut output).await?;
    Ok([first, second, third, fourth])
}

fn bind_registry_auth(options: &mut CreateAgentSessionOptions, registry: ModelRegistry) {
    options.auth_storage = Some(registry.auth_storage().clone());
    options.model_registry = Some(registry);
}

fn load_default_registry(options: &mut CreateAgentSessionOptions) -> Result<ModelRegistry, CallbackError> {
    if let Some(registry) = options.model_registry.take() {
        return Ok(registry);
    }
    let storage = AuthStorage::create(None).map_err(CallbackError::from)?;
    ModelRegistry::create(storage, None).map_err(CallbackError::from)
}

fn load_custom_registry(options: &mut CreateAgentSessionOptions) -> Result<ModelRegistry, CallbackError> {
    if let Some(registry) = options.model_registry.take() {
        return Ok(registry);
    }
    let agent_dir = options.agent_dir.clone().unwrap_or_else(|| "/tmp/my-app".into());
    let storage = AuthStorage::create(Some(&(agent_dir.clone() + "/auth.json"))).map_err(CallbackError::from)?;
    ModelRegistry::create(storage, Some(agent_dir + "/models.json")).map_err(CallbackError::from)
}

async fn print_session(
    mut options: CreateAgentSessionOptions,
    message: &str,
    output: &mut impl Write,
) -> Result<AgentSession, CallbackError> {
    options.session_manager = Some(SessionManager::in_memory(options.cwd.as_ref(), None).map_err(CallbackError::from)?);
    let session = create_agent_session(options).await?.session;
    writeln!(output, "{message}").map_err(CallbackError::from)?;
    session.dispose();
    Ok(session)
}

#[tokio::main(flavor = "current_thread")]
pub async fn main() -> Result<(), error::ExampleError> {
    run(
        std::array::from_fn(|_| CreateAgentSessionOptions::default()),
        std::io::stdout(),
    )
    .await
    .map(|_| ())
    .map_err(error::ExampleError::new)
}
