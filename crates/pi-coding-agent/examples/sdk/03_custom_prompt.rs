//! Replace a system prompt, then append instructions to normal discovery.
mod common;
mod error;
mod resources;
use pi_coding_agent::core::{
    resource_loader::{DefaultResourceLoader, ResourceLoader},
    sdk::CreateAgentSessionOptions,
    session_manager::SessionManager,
};
use std::io::Write;
use std::sync::{Arc, Mutex};

pub async fn run(
    options: [CreateAgentSessionOptions; 2],
    output: impl Write + Send + 'static,
) -> Result<(), pi_ai::request::CallbackError> {
    let output = Arc::new(Mutex::new(output));
    for (index, mut session) in options.into_iter().enumerate() {
        let mut loader_options = resources::options(&session)?;
        if index == 0 {
            loader_options.system_prompt_override = Some(Arc::new(|_| {
                Some(
                    "You are a helpful assistant that speaks like a pirate.\nAlways end responses with \"Arrr!\""
                        .into(),
                )
            }));
            loader_options.append_system_prompt_override = Some(Arc::new(|_| Vec::new()));
        } else {
            loader_options.append_system_prompt_override = Some(Arc::new(|base| {
                let mut prompts = base.to_vec();
                prompts.push(
                    "## Additional Instructions\n- Always be concise\n- Use bullet points when listing things".into(),
                );
                prompts
            }));
        }
        let loader = Arc::new(DefaultResourceLoader::new(loader_options)?);
        loader.reload().await?;
        session.resource_loader = Some(loader);
        session.session_manager =
            Some(SessionManager::in_memory(session.cwd.as_ref(), None).map_err(pi_ai::request::CallbackError::from)?);
        let (prompt, prefix, suffix) = if index == 0 {
            ("What is 2 + 2?", "=== Replace prompt ===\n", "\n\n")
        } else {
            ("List 3 benefits of TypeScript.", "=== Modify prompt ===\n", "\n")
        };
        common::stream(session, prompt, prefix, suffix, Arc::clone(&output)).await?;
    }
    Ok(())
}
#[tokio::main(flavor = "current_thread")]
pub async fn main() -> Result<(), error::ExampleError> {
    run(
        std::array::from_fn(|_| CreateAgentSessionOptions::default()),
        std::io::stdout(),
    )
    .await
    .map_err(error::ExampleError::new)
}
