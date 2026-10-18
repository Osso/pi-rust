//! Ports the deterministic cases of `packages/coding-agent/test/credential-print.test.ts`.

use std::sync::Arc;

use pi_coding_agent::cli::args::parse_args;
use pi_coding_agent::cli::{CredentialPrintError, CredentialPrintKind, resolve_credential_for_print};
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::model_runtime::{CreateModelRuntimeOptions, ModelRuntime};
use serde_json::{Map, Value};

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

async fn create_runtime(credentials: Arc<AuthStorage>) -> ModelRuntime {
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

fn command_message(error: &CredentialPrintError) -> &str {
    assert!(error.is_command_error(), "expected a command error, got {error:?}");
    error.message()
}

#[test]
fn prints_a_resolved_api_key() {
    runtime().block_on(async {
        let model_runtime = create_runtime(storage_with(&[(
            "openai",
            serde_json::json!({"type": "api_key", "key": "test-api-key"}),
        )]))
        .await;
        let credential = resolve_credential_for_print(
            &parse(&["--provider", "openai"]),
            &model_runtime,
            CredentialPrintKind::ApiKey,
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(credential, "test-api-key");
    });
}

#[test]
fn prints_bearer_tokens_resolved_from_an_authorization_header() {
    runtime().block_on(async {
        let model_runtime = create_runtime(storage_with(&[(
            "kimi-coding",
            serde_json::json!({
                "type": "oauth",
                "access": "header-test-token",
                "refresh": "test-refresh-token",
                "expires": 4102444800000.0,
            }),
        )]))
        .await;
        let credential = resolve_credential_for_print(
            &parse(&["--provider", "kimi-coding"]),
            &model_runtime,
            CredentialPrintKind::BearerToken,
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(credential, "header-test-token");
    });
}

#[test]
fn reports_unknown_providers() {
    runtime().block_on(async {
        let model_runtime = create_runtime(storage_with(&[])).await;
        let error = resolve_credential_for_print(
            &parse(&["--provider", "not-installed"]),
            &model_runtime,
            CredentialPrintKind::ApiKey,
            None,
            None,
        )
        .await
        .unwrap_err();
        assert_eq!(
            command_message(&error),
            "Unknown provider \"not-installed\". Use --list-models to see available providers."
        );
    });
}

#[test]
fn reports_provider_credential_type_mismatches() {
    runtime().block_on(async {
        let model_runtime = create_runtime(storage_with(&[(
            "openai-codex",
            serde_json::json!({
                "type": "oauth",
                "access": "token",
                "refresh": "refresh",
                "expires": 4102444800000.0,
            }),
        )]))
        .await;
        let error = resolve_credential_for_print(
            &parse(&["--provider", "openai-codex"]),
            &model_runtime,
            CredentialPrintKind::ApiKey,
            None,
            None,
        )
        .await
        .unwrap_err();
        assert_eq!(
            command_message(&error),
            "Provider \"openai-codex\" is configured with OAuth, not an API key"
        );
    });

    runtime().block_on(async {
        let model_runtime = create_runtime(storage_with(&[(
            "openai",
            serde_json::json!({"type": "api_key", "key": "key"}),
        )]))
        .await;
        let error = resolve_credential_for_print(
            &parse(&["--provider", "openai"]),
            &model_runtime,
            CredentialPrintKind::BearerToken,
            None,
            None,
        )
        .await
        .unwrap_err();
        assert_eq!(
            command_message(&error),
            "Provider \"openai\" is not configured with an OAuth bearer token"
        );
    });
}

#[test]
fn reports_missing_targets_and_unknown_options() {
    runtime().block_on(async {
        let model_runtime = create_runtime(storage_with(&[])).await;
        let error = resolve_credential_for_print(&parse(&[]), &model_runtime, CredentialPrintKind::ApiKey, None, None)
            .await
            .unwrap_err();
        assert_eq!(
            command_message(&error),
            "Credential printing requires --provider <provider> or --model <model>"
        );

        let error = resolve_credential_for_print(
            &parse(&["--provider", "openai", "--credentails"]),
            &model_runtime,
            CredentialPrintKind::ApiKey,
            None,
            None,
        )
        .await
        .unwrap_err();
        assert_eq!(
            command_message(&error),
            "Unknown option --credentails for \"auth print-api-key\"."
        );
    });
}

#[test]
fn reports_unmatched_models_without_a_provider() {
    runtime().block_on(async {
        let model_runtime = create_runtime(storage_with(&[])).await;
        let error = resolve_credential_for_print(
            &parse(&["--model", "does-not-exist"]),
            &model_runtime,
            CredentialPrintKind::ApiKey,
            None,
            None,
        )
        .await
        .unwrap_err();
        assert_eq!(
            command_message(&error),
            "Model \"does-not-exist\" not found. Use --list-models to see available models."
        );
    });
}

#[test]
fn reports_multiple_matching_providers() {
    runtime().block_on(async {
        let model_runtime = create_runtime(storage_with(&[
            (
                "anthropic",
                serde_json::json!({"type": "api_key", "key": "anthropic-key"}),
            ),
            (
                "opencode",
                serde_json::json!({"type": "api_key", "key": "opencode-key"}),
            ),
        ]))
        .await;
        let error = resolve_credential_for_print(
            &parse(&["--model", "claude-haiku-4-5"]),
            &model_runtime,
            CredentialPrintKind::ApiKey,
            None,
            None,
        )
        .await
        .unwrap_err();
        let message = command_message(&error);
        assert!(
            message.starts_with("Multiple configured providers matched (")
                && message.ends_with("). Specify --provider."),
            "unexpected message: {message}"
        );
        assert!(
            message.contains("anthropic") && message.contains("opencode"),
            "{message}"
        );
    });
}

#[test]
fn resolves_a_model_for_an_explicit_provider() {
    runtime().block_on(async {
        let model_runtime = create_runtime(storage_with(&[(
            "openai",
            serde_json::json!({"type": "api_key", "key": "key"}),
        )]))
        .await;
        let credential = resolve_credential_for_print(
            &parse(&["--provider", "openai", "--model", "gpt-4.1"]),
            &model_runtime,
            CredentialPrintKind::ApiKey,
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(credential, "key");
    });
}
