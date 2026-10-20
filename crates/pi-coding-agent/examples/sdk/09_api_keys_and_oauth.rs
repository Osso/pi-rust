//! Default/custom credential locations and an ephemeral runtime key override.
mod error;
use pi_coding_agent::core::{
    agent_session::AgentSession,
    model_runtime::{CreateModelRuntimeOptions, ModelRuntime},
    sdk::{CreateAgentSessionOptions, create_agent_session},
    session_manager::SessionManager,
};
use std::io::Write;

pub async fn run(options: [CreateAgentSessionOptions; 3], mut output: impl Write) -> Result<[AgentSession; 3], String> {
    let [mut default, mut custom, mut overridden] = options;
    let runtime = match default.model_runtime.take() {
        Some(runtime) => runtime,
        None => ModelRuntime::create(CreateModelRuntimeOptions::default())
            .await
            .map_err(|e| e.to_string())?,
    };
    default.model_runtime = Some(runtime.clone());
    let first = print_session(default, "Session with default model runtime", &mut output).await?;
    if custom.model_runtime.is_none() {
        let agent_dir = custom.agent_dir.as_deref().unwrap_or("/tmp/my-app");
        custom.model_runtime = Some(
            ModelRuntime::create(CreateModelRuntimeOptions {
                auth_path: Some(format!("{agent_dir}/auth.json")),
                models_path: Some(Some(format!("{agent_dir}/models.json"))),
                ..Default::default()
            })
            .await
            .map_err(|e| e.to_string())?,
        );
    }
    let second = print_session(custom, "Session with custom auth and models locations", &mut output).await?;
    runtime
        .set_runtime_api_key("anthropic", "sk-my-temp-key", Default::default())
        .await
        .map_err(|e| e.to_string())?;
    overridden.model_runtime = Some(runtime);
    let third = print_session(overridden, "Session with runtime API key override", &mut output).await?;
    Ok([first, second, third])
}
async fn print_session(
    mut options: CreateAgentSessionOptions,
    message: &str,
    output: &mut impl Write,
) -> Result<AgentSession, String> {
    options.session_manager = Some(SessionManager::in_memory(options.cwd.as_deref(), None).map_err(|e| e.to_string())?);
    let session = create_agent_session(options).await?.session;
    writeln!(output, "{message}").map_err(|e| e.to_string())?;
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
    .map_err(error::ExampleError)
}
