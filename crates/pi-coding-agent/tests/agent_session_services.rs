//! Behavioral tests for the `agent-session-services` extension wiring.

use pi_js_compat::js_json::JsString;
use std::sync::Arc;

use indexmap::IndexMap;
use pi_ai::providers::faux::{FauxTokenSize, RegisterFauxProviderOptions, faux_provider};
use pi_coding_agent::cli::args::UnknownFlagValue;
use pi_coding_agent::core::agent_session_services::{CreateAgentSessionServicesOptions, create_agent_session_services};
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::extensions::types::{FlagType, FlagValue, HandlerError};
use pi_coding_agent::core::model_registry::ModelRegistry;
use pi_coding_agent::core::resource_loader::{DefaultResourceLoaderOptions, InlineExtension};

async fn faux_runtime() -> ModelRegistry {
    let handle = faux_provider(RegisterFauxProviderOptions {
        api: Some("faux".to_owned()),
        token_size: Some(FauxTokenSize {
            min: Some(1.0),
            max: Some(1.0),
        }),
        ..Default::default()
    });
    let runtime = ModelRegistry::in_memory(AuthStorage::in_memory(serde_json::Map::from_iter([(
        "faux".to_owned(),
        serde_json::json!({"type": "api_key", "key": "faux-key"}),
    )])))
    .unwrap();
    scripted_registry::register_scripted_provider(&runtime, handle.provider.clone());
    runtime
}

fn flag_extension() -> InlineExtension {
    InlineExtension::Named {
        name: JsString::from("flag-test"),
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
        cwd: JsString::from(cwd.clone()),
        agent_dir: Some(JsString::from(cwd.clone())),
        settings_manager: None,
        model_registry: Some(runtime),
        auth_storage: None,
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
                message: JsString::from("Unknown option: --missing"),
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
        cwd: JsString::from(cwd.clone()),
        agent_dir: Some(JsString::from(cwd.clone())),
        settings_manager: None,
        model_registry: Some(runtime),
        auth_storage: None,
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
                message: JsString::from("Extension flag \"--label\" requires a value"),
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
        cwd: JsString::from(cwd.clone()),
        agent_dir: Some(JsString::from(cwd.clone())),
        settings_manager: None,
        model_registry: Some(runtime),
        auth_storage: None,
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

use crate::shared_oracle as oracle;
use crate::shared_scripted_registry as scripted_registry;

#[tokio::test]
async fn services_paths_and_flag_diagnostics_match_frozen() {
    let mut oracle = oracle::TsOracle::spawn(
        "agent-session-services-oracle.mts",
        "packages/coding-agent/src/core/agent-session-services.ts",
    );
    let root = tempfile::tempdir().expect("temp dir");
    let cwd = root.path().display().to_string();
    for agent_dir in [Some(""), None, Some(cwd.as_str())] {
        let flags = serde_json::json!([
            ["verbose", false],
            ["label", true],
            ["first-missing", true],
            ["second-missing", "value"]
        ]);
        let expected = oracle.request(&serde_json::json!({
            "cwd": cwd, "agentDir": agent_dir, "flags": flags
        }));
        let services = create_agent_session_services(CreateAgentSessionServicesOptions {
            cwd: cwd.clone().into(),
            agent_dir: agent_dir.map(JsString::from),
            settings_manager: Some(pi_coding_agent::core::settings_manager::SettingsManager::in_memory(
                Default::default(),
                Default::default(),
            )),
            model_registry: Some(faux_runtime().await),
            auth_storage: Some(AuthStorage::in_memory(serde_json::Map::new())),
            extension_flag_values: Some(IndexMap::from([
                ("verbose".to_owned(), UnknownFlagValue::Boolean(false)),
                ("label".to_owned(), UnknownFlagValue::Boolean(true)),
                ("first-missing".to_owned(), UnknownFlagValue::Boolean(true)),
                (
                    "second-missing".to_owned(),
                    UnknownFlagValue::String("value".to_owned()),
                ),
            ])),
            resource_loader_options: Some(DefaultResourceLoaderOptions {
                extension_factories: vec![flag_extension()],
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
        let runtime = services.resource_loader.get_extensions().runtime;
        assert_eq!(
            serde_json::json!({
                "cwd": services.cwd,
                "agentDir": services.agent_dir,
                "diagnostics": services.diagnostics,
                "flags": {"verbose": runtime.flag_value("verbose").map(|value| match value {
                    FlagValue::Boolean(value) => value,
                    FlagValue::String(_) => panic!("verbose must be boolean"),
                }).expect("verbose value")}
            }),
            expected,
            "agentDir={agent_dir:?}"
        );
    }
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
#[path = "support/scripted_registry.rs"]
pub mod shared_scripted_registry;
