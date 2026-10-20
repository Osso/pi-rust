//! Filter discovered skills, then append a synthetic SDK skill.
mod error;
mod resources;
use pi_coding_agent::core::{
    resource_loader::{DefaultResourceLoader, ResourceLoader},
    sdk::{CreateAgentSessionOptions, create_agent_session},
    session_manager::SessionManager,
    skills::Skill,
    source_info::{SyntheticSourceInfoOptions, create_synthetic_source_info},
};
use std::io::Write;
use std::sync::Arc;

pub async fn run(
    mut options: CreateAgentSessionOptions,
    mut output: impl Write + Send + 'static,
) -> Result<(), String> {
    let mut loader_options = resources::options(&options)?;
    loader_options.skills_override = Some(Arc::new(|mut current| {
        current
            .skills
            .retain(|skill| skill.name.contains("browser") || skill.name.contains("search"));
        current.skills.push(Skill {
            name: "my-skill".into(),
            description: "Custom project instructions".into(),
            file_path: "/virtual/SKILL.md".into(),
            base_dir: "/virtual".into(),
            source_info: create_synthetic_source_info(
                "/virtual/SKILL.md",
                &SyntheticSourceInfoOptions {
                    source: "sdk".into(),
                    scope: None,
                    origin: None,
                    base_dir: None,
                },
            ),
            disable_model_invocation: false,
        });
        current
    }));
    let loader = Arc::new(DefaultResourceLoader::new(loader_options).map_err(|e| e.to_string())?);
    loader.reload().await?;
    let discovered = loader.get_skills();
    let names: Vec<_> = discovered.skills.iter().map(|skill| &skill.name).collect();
    writeln!(
        output,
        "Discovered skills: {}",
        serde_json::to_string(&names).map_err(|e| e.to_string())?
    )
    .map_err(|e| e.to_string())?;
    if !discovered.diagnostics.is_empty() {
        writeln!(
            output,
            "Warnings: {}",
            serde_json::to_string(&discovered.diagnostics).map_err(|e| e.to_string())?
        )
        .map_err(|e| e.to_string())?;
    }
    options.resource_loader = Some(loader);
    options.session_manager = Some(SessionManager::in_memory(options.cwd.as_deref(), None).map_err(|e| e.to_string())?);
    let session = create_agent_session(options).await?.session;
    writeln!(output, "Session created with filtered skills").map_err(|e| e.to_string())?;
    session.dispose();
    Ok(())
}
#[tokio::main(flavor = "current_thread")]
pub async fn main() -> Result<(), error::ExampleError> {
    run(CreateAgentSessionOptions::default(), std::io::stdout())
        .await
        .map_err(error::ExampleError)
}
