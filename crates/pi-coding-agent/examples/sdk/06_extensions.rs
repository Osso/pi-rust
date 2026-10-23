//! Load registered native extensions and an inline agent-start hook.
mod common;
mod error;
mod resources;
use pi_coding_agent::core::{
    extensions::{
        loader::ExtensionRegistry,
        types::{AgentStartEvent, BoxFuture, ExtensionContext, HandlerError},
    },
    resource_loader::{DefaultResourceLoader, InlineExtension, ResourceLoader},
    sdk::CreateAgentSessionOptions,
    session_manager::SessionManager,
};
use std::io::Write;
use std::sync::{Arc, Mutex};

pub async fn run(
    options: CreateAgentSessionOptions,
    registry: Arc<ExtensionRegistry>,
    output: impl Write + Send + 'static,
) -> Result<(), pi_ai::request::CallbackError> {
    let output = Arc::new(Mutex::new(output));
    let inline_output = Arc::clone(&output);
    let factory = Arc::new(move |api: &pi_coding_agent::core::extensions::types::ExtensionApi| {
        let output = Arc::clone(&inline_output);
        let result = api.on_agent_start(
            move |_: AgentStartEvent, _: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
                let output = Arc::clone(&output);
                Box::pin(async move {
                    writeln!(
                        output.lock().expect("output mutex poisoned"),
                        "[Inline Extension] Agent starting"
                    )
                    .map_err(|e| HandlerError::new(e.to_string()))?;
                    Ok(None)
                })
            },
        );
        Box::pin(async move { result.map_err(HandlerError::from) }) as BoxFuture<'static, Result<(), HandlerError>>
    });
    let mut loader_options = resources::options(&options)?;
    loader_options.extensions_override = None;
    loader_options.extension_registry = Some(registry);
    // Native registry IDs replace dynamic TypeScript loading (approved native API).
    loader_options.additional_extension_paths =
        vec!["./my-logging-extension.ts".into(), "./my-safety-extension.ts".into()];
    loader_options.extension_factories = vec![InlineExtension::Factory {
        extension_path: None,
        factory,
    }];
    let loader = Arc::new(DefaultResourceLoader::new(loader_options)?);
    loader.reload().await?;
    let mut options = options;
    options.resource_loader = Some(loader);
    options.session_manager =
        Some(SessionManager::in_memory(options.cwd.as_ref(), None).map_err(pi_ai::request::CallbackError::from)?);
    common::stream(options, "List files in the current directory.", "", "\n", output).await
}
#[tokio::main(flavor = "current_thread")]
pub async fn main() -> Result<(), error::ExampleError> {
    run(
        CreateAgentSessionOptions::default(),
        Arc::new(ExtensionRegistry::new()),
        std::io::stdout(),
    )
    .await
    .map_err(error::ExampleError::new)
}
