//! Default discovery with the same private state seam as the session factory.
use pi_coding_agent::{
    config::get_agent_dir,
    core::{resource_loader::DefaultResourceLoaderOptions, sdk::CreateAgentSessionOptions},
};
use std::sync::Arc;

pub fn options(
    session: &CreateAgentSessionOptions,
) -> Result<DefaultResourceLoaderOptions, pi_ai::request::CallbackError> {
    let cwd = match &session.cwd {
        Some(cwd) => cwd.clone(),
        None => pi_js_compat::process_cwd::cwd()
            .map_err(|error| pi_ai::request::CallbackError::from_error(error.message.clone(), error))?,
    };
    let extensions_override = session.resource_loader.as_ref().map(|loader| {
        let extensions = loader.get_extensions();
        Arc::new(move |_| extensions.clone()) as pi_coding_agent::core::resource_loader::ExtensionsOverride
    });
    Ok(DefaultResourceLoaderOptions {
        cwd,
        agent_dir: match &session.agent_dir {
            Some(directory) => directory.clone(),
            None => get_agent_dir().map_err(pi_ai::request::CallbackError::from)?,
        },
        settings_manager: session.settings_manager.clone(),
        extensions_override,
        ..Default::default()
    })
}
