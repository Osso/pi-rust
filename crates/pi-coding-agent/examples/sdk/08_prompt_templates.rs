//! Append a synthetic deploy template to normal prompt discovery.
mod error;
mod resources;
use pi_coding_agent::core::{
    agent_session::AgentSession,
    prompt_templates::PromptTemplate,
    resource_loader::{DefaultResourceLoader, ResourceLoader},
    sdk::{CreateAgentSessionOptions, create_agent_session},
    session_manager::SessionManager,
    source_info::{SyntheticSourceInfoOptions, create_synthetic_source_info},
};
use pi_js_compat::js_json::JsString;
use std::io::Write;
use std::sync::Arc;

pub async fn run(
    mut options: CreateAgentSessionOptions,
    mut output: impl Write,
) -> Result<(Vec<PromptTemplate>, AgentSession), String> {
    let mut loader_options = resources::options(&options)?;
    loader_options.prompts_override = Some(Arc::new(|mut current| {
        current.prompts.push(PromptTemplate {
            name: "deploy".into(),
            description: "Deploy the application".into(),
            file_path: "/virtual/prompts/deploy.md".into(),
            argument_hint: None,
            source_info: create_synthetic_source_info(
                &JsString::from("/virtual/prompts/deploy.md"),
                &SyntheticSourceInfoOptions {
                    source: "sdk".into(),
                    scope: None,
                    origin: None,
                    base_dir: None,
                },
            ),
            content: "# Deploy Instructions\n\n1. Build: npm run build\n2. Test: npm test\n3. Deploy: npm run deploy"
                .into(),
        });
        current
    }));
    let loader = Arc::new(DefaultResourceLoader::new(loader_options).map_err(|e| e.to_string())?);
    loader.reload().await?;
    let discovered = loader.get_prompts().prompts;
    writeln!(output, "Discovered prompt templates:").map_err(|e| e.to_string())?;
    for template in &discovered {
        writeln!(output, "  /{}: {}", template.name, template.description).map_err(|e| e.to_string())?;
    }
    options.resource_loader = Some(loader);
    options.session_manager = Some(SessionManager::in_memory(options.cwd.as_ref(), None).map_err(|e| e.to_string())?);
    let session = create_agent_session(options).await?.session;
    // Preserve the source's double-counted inline template in the printed label.
    writeln!(output, "Session created with {} prompt templates", discovered.len() + 1).map_err(|e| e.to_string())?;
    session.dispose();
    Ok((discovered, session))
}
#[tokio::main(flavor = "current_thread")]
pub async fn main() -> Result<(), error::ExampleError> {
    run(CreateAgentSessionOptions::default(), std::io::stdout())
        .await
        .map(|_| ())
        .map_err(error::ExampleError)
}
