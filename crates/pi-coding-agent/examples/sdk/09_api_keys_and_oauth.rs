//! Default/custom credential locations and an ephemeral runtime key override.
mod error;
use pi_coding_agent::core::{
    agent_session::AgentSession,
    model_registry::ModelRegistry,
    sdk::{CreateAgentSessionOptions, create_agent_session},
    session_manager::SessionManager,
};
use pi_js_compat::js_json::JsString;
use std::io::Write;

pub async fn run(
    options: [CreateAgentSessionOptions; 3],
    mut output: impl Write,
) -> Result<[AgentSession; 3], pi_ai::request::CallbackError> {
    let [mut default, mut custom, mut overridden] = options;
    let runtime = match default.model_registry.take() {
        Some(runtime) => runtime,
        None => ModelRegistry::create(
            pi_coding_agent::core::auth_storage::AuthStorage::create(None)
                .map_err(pi_ai::request::CallbackError::from)?,
            None,
        )
        .map_err(pi_ai::request::CallbackError::from)?,
    };
    default.model_registry = Some(runtime.clone());
    let first = print_session(default, "Session with default model runtime", &mut output).await?;
    if custom.model_registry.is_none() {
        let agent_dir = custom
            .agent_dir
            .clone()
            .unwrap_or_else(|| JsString::from("/tmp/my-app"));
        custom.model_registry = Some(
            ModelRegistry::create(
                pi_coding_agent::core::auth_storage::AuthStorage::create(Some(&(agent_dir.clone() + "/auth.json")))
                    .map_err(pi_ai::request::CallbackError::from)?,
                Some(agent_dir.clone() + "/models.json"),
            )
            .map_err(pi_ai::request::CallbackError::from)?,
        );
    }
    let second = print_session(custom, "Session with custom auth and models locations", &mut output).await?;
    runtime
        .auth_storage()
        .set_runtime_api_key("anthropic", &JsString::from("sk-my-temp-key"));
    overridden.model_registry = Some(runtime);
    let third = print_session(overridden, "Session with runtime API key override", &mut output).await?;
    Ok([first, second, third])
}
async fn print_session(
    mut options: CreateAgentSessionOptions,
    message: &str,
    output: &mut impl Write,
) -> Result<AgentSession, pi_ai::request::CallbackError> {
    options.session_manager =
        Some(SessionManager::in_memory(options.cwd.as_ref(), None).map_err(pi_ai::request::CallbackError::from)?);
    let session = create_agent_session(options)
        .await
        .map_err(pi_ai::request::CallbackError::from)?
        .session;
    writeln!(output, "{message}").map_err(|e| JsString::from(e.to_string()))?;
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
