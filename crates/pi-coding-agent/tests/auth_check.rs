//! Ports `packages/coding-agent/test/auth-check.test.ts`.

use pi_js_compat::js_json::JsString;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

use pi_ai::auth::types::CredentialStore;
use pi_coding_agent::cli::args::parse_args;
use pi_coding_agent::cli::{
    AuthCheckReason, AuthCheckStatus, CheckProviderAuthOptions, check_provider_auth, create_auth_check_model_runtime,
    get_provider_credential,
};
use pi_coding_agent::core::auth_storage::{AuthStorage, ReadOnlyAuthStorage};
use pi_coding_agent::core::model_runtime::{CreateModelRuntimeOptions, ModelRuntime};
use serde_json::{Map, Value};

fn temp_dir(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("pi-auth-check-{name}-{}-{unique}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn storage_with(entries: &[(&str, Value)]) -> Arc<AuthStorage> {
    let mut data = Map::new();
    for (provider, credential) in entries {
        data.insert((*provider).to_owned(), credential.clone());
    }
    Arc::new(AuthStorage::in_memory(data))
}

async fn create_runtime(credentials: Arc<dyn CredentialStore>) -> ModelRuntime {
    ModelRuntime::create(CreateModelRuntimeOptions {
        credentials: Some(credentials),
        models_path: Some(None),
        allow_model_network: false,
        refresh_on_create: Some(false),
        ..Default::default()
    })
    .await
    .unwrap()
}

fn parse(values: &[&str]) -> pi_coding_agent::cli::args::Args {
    parse_args(&values.iter().map(|value| (*value).to_owned()).collect::<Vec<_>>())
}

#[test]
fn reports_a_configured_provider_as_ready() {
    runtime().block_on(async {
        let credentials = storage_with(&[("openai", serde_json::json!({"type": "api_key", "key": "test-key"}))]);
        let model_runtime = create_runtime(credentials).await;
        let result = check_provider_auth(&parse(&["--provider", "openai"]), &model_runtime, Default::default())
            .await
            .unwrap();

        assert_eq!(result.status, AuthCheckStatus::Ready);
        assert_eq!(result.provider, "openai");
        assert_eq!(result.reason, None);
        assert_eq!(result.auth_type.map(|auth| auth.as_str()), Some("api_key"));
    });
}

#[test]
fn resolves_the_provider_from_the_model_flag() {
    runtime().block_on(async {
        let credentials = storage_with(&[("openai", serde_json::json!({"type": "api_key", "key": "test-key"}))]);
        let model_runtime = create_runtime(credentials).await;

        let result = check_provider_auth(
            &parse(&["--model", "openai/gpt-4.1"]),
            &model_runtime,
            Default::default(),
        )
        .await
        .unwrap();
        assert_eq!(result.status, AuthCheckStatus::Ready);
        assert_eq!(result.provider, "openai");

        let result = check_provider_auth(
            &parse(&["--provider", "openai", "--model", "gpt-4.1"]),
            &model_runtime,
            Default::default(),
        )
        .await
        .unwrap();
        assert_eq!(result.status, AuthCheckStatus::Ready);
        assert_eq!(result.provider, "openai");
    });
}

#[test]
fn reports_an_unknown_provider_as_not_ready() {
    runtime().block_on(async {
        let model_runtime = create_runtime(storage_with(&[])).await;
        let result = check_provider_auth(
            &parse(&["--provider", "not-installed"]),
            &model_runtime,
            Default::default(),
        )
        .await
        .unwrap();

        assert_eq!(result.status, AuthCheckStatus::NotReady);
        assert_eq!(result.provider, "not-installed");
        assert_eq!(result.reason, Some(AuthCheckReason::ProviderNotFound));
    });
}

#[test]
fn does_not_treat_an_unresolved_stored_environment_reference_as_configured() {
    runtime().block_on(async {
        let dir = temp_dir("missing-env");
        let auth_path = dir.join("auth.json");
        fs::write(
            &auth_path,
            serde_json::json!({"openai": {"type": "api_key", "key": "$MISSING_AUTH_CHECK_KEY"}}).to_string(),
        )
        .unwrap();
        let credentials = Arc::new(ReadOnlyAuthStorage::new(Some(&JsString::from(
            &auth_path.display().to_string(),
        ))));
        let model_runtime = create_runtime(credentials).await;
        let result = check_provider_auth(&parse(&["--provider", "openai"]), &model_runtime, Default::default())
            .await
            .unwrap();

        assert_eq!(result.status, AuthCheckStatus::NotReady);
        assert_eq!(result.reason, Some(AuthCheckReason::CredentialsNotConfigured));
        fs::remove_dir_all(dir).unwrap();
    });
}

#[test]
fn reports_malformed_auth_state_as_invalid() {
    runtime().block_on(async {
        let dir = temp_dir("invalid");
        let auth_path = dir.join("auth.json");
        fs::write(&auth_path, "{invalid-json").unwrap();
        let credentials = Arc::new(ReadOnlyAuthStorage::new(Some(&JsString::from(
            &auth_path.display().to_string(),
        ))));
        let model_runtime = create_runtime(credentials).await;
        let result = check_provider_auth(&parse(&["--provider", "openai"]), &model_runtime, Default::default())
            .await
            .unwrap();

        assert_eq!(result.status, AuthCheckStatus::Invalid);
        assert_eq!(result.reason, Some(AuthCheckReason::InvalidState));
        fs::remove_dir_all(dir).unwrap();
    });
}

#[test]
fn does_not_create_an_auth_file_or_its_parent_directory() {
    runtime().block_on(async {
        let dir = temp_dir("no-create");
        let auth_path = dir.join("agent").join("auth.json");
        let credentials = Arc::new(ReadOnlyAuthStorage::new(Some(&JsString::from(
            &auth_path.display().to_string(),
        ))));
        let model_runtime = create_runtime(credentials).await;
        let result = check_provider_auth(&parse(&["--provider", "openai"]), &model_runtime, Default::default())
            .await
            .unwrap();

        assert_eq!(result.status, AuthCheckStatus::NotReady);
        assert_eq!(result.reason, Some(AuthCheckReason::CredentialsNotConfigured));
        assert!(!auth_path.exists());
        assert!(!dir.join("agent").exists());
        fs::remove_dir_all(dir).unwrap();
    });
}

#[test]
fn reads_api_key_credentials_without_refresh() {
    runtime().block_on(async {
        let credentials = storage_with(&[("openai", serde_json::json!({"type": "api_key", "key": "test-key"}))]);
        let model_runtime = create_runtime(credentials.clone()).await;
        let credential = get_provider_credential(
            "openai",
            &model_runtime,
            credentials,
            CheckProviderAuthOptions { refresh: false },
        )
        .await
        .unwrap();
        assert_eq!(credential.as_ref(), Some(&JsString::from("test-key")));
    });
}

#[test]
fn reads_stored_oauth_access_tokens_without_refreshing() {
    runtime().block_on(async {
        let credentials = storage_with(&[(
            "openai-codex",
            serde_json::json!({"type": "oauth", "access": "old-token", "refresh": "refresh-token", "expires": 0}),
        )]);
        let model_runtime = create_runtime(credentials.clone()).await;
        let credential = get_provider_credential(
            "openai-codex",
            &model_runtime,
            credentials,
            CheckProviderAuthOptions { refresh: false },
        )
        .await
        .unwrap();
        assert_eq!(credential.as_ref(), Some(&JsString::from("old-token")));
    });
}

#[test]
fn creates_an_auth_check_runtime_with_builtin_providers() {
    runtime().block_on(async {
        let model_runtime = create_auth_check_model_runtime(storage_with(&[])).await.unwrap();
        assert!(model_runtime.get_provider("openai").is_some());
    });
}
