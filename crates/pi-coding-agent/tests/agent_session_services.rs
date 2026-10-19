//! Behavioral tests for the `agent-session-services` extension wiring.

use std::sync::Arc;

use indexmap::IndexMap;
use pi_ai::providers::faux::{FauxTokenSize, RegisterFauxProviderOptions, faux_provider};
use pi_coding_agent::cli::args::UnknownFlagValue;
use pi_coding_agent::core::agent_session_services::{CreateAgentSessionServicesOptions, create_agent_session_services};
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::extensions::types::{FlagType, FlagValue, HandlerError};
use pi_coding_agent::core::model_runtime::{CreateModelRuntimeOptions, ModelRuntime};
use pi_coding_agent::core::resource_loader::{DefaultResourceLoaderOptions, InlineExtension};

async fn faux_runtime() -> ModelRuntime {
    let handle = faux_provider(RegisterFauxProviderOptions {
        api: Some("faux".to_owned()),
        token_size: Some(FauxTokenSize {
            min: Some(1.0),
            max: Some(1.0),
        }),
        ..Default::default()
    });
    let runtime = ModelRuntime::create(CreateModelRuntimeOptions {
        credentials: Some(Arc::new(AuthStorage::in_memory(serde_json::Map::from_iter([(
            "faux".to_owned(),
            serde_json::json!({"type": "api_key", "key": "faux-key"}),
        )])))),
        models_path: Some(None),
        ..Default::default()
    })
    .await
    .expect("model runtime");
    runtime.register_native_provider(handle.provider.clone());
    runtime
}

fn flag_extension() -> InlineExtension {
    InlineExtension::Named {
        name: "flag-test".to_owned(),
        factory: Arc::new(|pi| {
            let result = (|| {
                pi.register_flag("verbose", None, FlagType::Boolean, None)?;
                pi.register_flag("label", None, FlagType::String, None)?;
                Ok::<(), String>(())
            })();
            Box::pin(async move { result.map_err(HandlerError::new) })
        }),
        hidden: None,
    }
}

#[tokio::test]
async fn extension_flags_are_applied_and_unknown_flags_reported() {
    let root = tempfile::tempdir().expect("temp dir");
    let cwd = root.path().display().to_string();
    let runtime = faux_runtime().await;
    let mut values = IndexMap::new();
    values.insert("verbose".to_owned(), UnknownFlagValue::Boolean(true));
    values.insert("label".to_owned(), UnknownFlagValue::String("release".to_owned()));
    values.insert("missing".to_owned(), UnknownFlagValue::Boolean(true));

    let services = create_agent_session_services(CreateAgentSessionServicesOptions {
        cwd: cwd.clone(),
        agent_dir: Some(cwd.clone()),
        settings_manager: None,
        model_runtime: Some(runtime),
        model_runtime_signal: None,
        extension_flag_values: Some(values),
        resource_loader_options: Some(DefaultResourceLoaderOptions {
            extension_factories: vec![flag_extension()],
            ..Default::default()
        }),
        resource_loader: None,
        resource_loader_reload_options: None,
    })
    .await
    .expect("services");

    assert_eq!(
        services.diagnostics,
        vec![
            pi_coding_agent::core::agent_session_services::AgentSessionRuntimeDiagnostic {
                diagnostic_type: "error".to_owned(),
                message: "Unknown option: --missing".to_owned(),
            }
        ]
    );
    let extensions = services.resource_loader.get_extensions();
    assert_eq!(extensions.runtime.flag_value("verbose"), Some(FlagValue::Boolean(true)));
    assert_eq!(
        extensions.runtime.flag_value("label"),
        Some(FlagValue::String("release".to_owned()))
    );
}

#[tokio::test]
async fn string_extension_flag_without_a_value_is_rejected() {
    let root = tempfile::tempdir().expect("temp dir");
    let cwd = root.path().display().to_string();
    let runtime = faux_runtime().await;
    let mut values = IndexMap::new();
    values.insert("label".to_owned(), UnknownFlagValue::Boolean(true));

    let services = create_agent_session_services(CreateAgentSessionServicesOptions {
        cwd: cwd.clone(),
        agent_dir: Some(cwd.clone()),
        settings_manager: None,
        model_runtime: Some(runtime),
        model_runtime_signal: None,
        extension_flag_values: Some(values),
        resource_loader_options: Some(DefaultResourceLoaderOptions {
            extension_factories: vec![flag_extension()],
            ..Default::default()
        }),
        resource_loader: None,
        resource_loader_reload_options: None,
    })
    .await
    .expect("services");

    assert_eq!(
        services.diagnostics,
        vec![
            pi_coding_agent::core::agent_session_services::AgentSessionRuntimeDiagnostic {
                diagnostic_type: "error".to_owned(),
                message: "Extension flag \"--label\" requires a value".to_owned(),
            }
        ]
    );
    assert_eq!(
        services.resource_loader.get_extensions().runtime.flag_value("label"),
        None
    );
}

#[tokio::test]
async fn default_resource_loader_options_build_a_loader() {
    let root = tempfile::tempdir().expect("temp dir");
    let cwd = root.path().display().to_string();
    let runtime = faux_runtime().await;

    let services = create_agent_session_services(CreateAgentSessionServicesOptions {
        cwd: cwd.clone(),
        agent_dir: Some(cwd.clone()),
        settings_manager: None,
        model_runtime: Some(runtime),
        model_runtime_signal: None,
        extension_flag_values: None,
        resource_loader_options: Some(DefaultResourceLoaderOptions {
            no_extensions: true,
            no_skills: true,
            no_prompt_templates: true,
            no_themes: true,
            no_context_files: true,
            ..Default::default()
        }),
        resource_loader: None,
        resource_loader_reload_options: None,
    })
    .await
    .expect("services");

    assert!(services.diagnostics.is_empty());
    assert!(services.resource_loader.get_extensions().extensions.is_empty());
    assert!(services.resource_loader.get_skills().skills.is_empty());
}
