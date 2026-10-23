//! Merge discovered AGENTS.md files with an inline project context.
mod error;
mod resources;
use pi_coding_agent::core::{
    agent_session::AgentSession,
    resource_loader::{DefaultResourceLoader, ResourceLoader},
    sdk::{CreateAgentSessionOptions, create_agent_session},
    session_manager::SessionManager,
    system_prompt::ContextFile,
};
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::encode_node_utf8;
use std::io::Write;
use std::sync::Arc;

pub async fn run(
    mut options: CreateAgentSessionOptions,
    mut output: impl Write,
) -> Result<(Vec<ContextFile>, AgentSession), JsString> {
    let mut loader_options = resources::options(&options)?;
    loader_options.agents_files_override = Some(Arc::new(|mut current| {
        current.agents_files.push(ContextFile {path: "/virtual/AGENTS.md".into(), content: "# Project Guidelines\n\n## Code Style\n- Use TypeScript strict mode\n- No any types\n- Prefer const over let".into()});
        current
    }));
    let loader = Arc::new(DefaultResourceLoader::new(loader_options)?);
    loader.reload().await?;
    let discovered = loader.get_agents_files().agents_files;
    writeln!(output, "Discovered context files:").map_err(|e| JsString::from(e.to_string()))?;
    for file in &discovered {
        writeln!(
            output,
            "  - {} ({} chars)",
            encode_node_utf8(&file.path),
            file.content.len()
        )
        .map_err(|e| JsString::from(e.to_string()))?;
    }
    options.resource_loader = Some(loader);
    options.session_manager =
        Some(SessionManager::in_memory(options.cwd.as_ref(), None).map_err(|e| e.message().clone())?);
    let session = create_agent_session(options)
        .await
        .map_err(|error| error.message)?
        .session;
    // Source already appended the inline file, then adds one again in its label.
    writeln!(output, "Session created with {} context files", discovered.len() + 1)
        .map_err(|e| JsString::from(e.to_string()))?;
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
