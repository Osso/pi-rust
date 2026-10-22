//! DRIFT: frozen model-registry.ts:811-820 propagates credential lookup rejection.
//! RED intent, unexecuted until coding-agent library compiles.

use std::sync::Arc;

use pi_ai::{
    auth::types::{AuthError, AuthOperationOptions, Credential, CredentialInfo, CredentialModifier, CredentialStore},
    request::BoxFuture,
};
use pi_coding_agent::core::{
    model_registry::ModelRegistry,
    model_runtime::{CreateModelRuntimeOptions, ModelRuntime},
};
use pi_js_compat::js_json::JsString;

struct RejectingCredentials;
impl CredentialStore for RejectingCredentials {
    fn read(&self, _: &str, _: AuthOperationOptions) -> BoxFuture<'_, Result<Option<Credential>, AuthError>> {
        Box::pin(async {
            Err(AuthError::failed(
                JsString::from("credential rejected ") + JsString::from_utf16(vec![0xd800]),
            ))
        })
    }
    fn list(&self, _: AuthOperationOptions) -> BoxFuture<'_, Result<Vec<CredentialInfo>, AuthError>> {
        Box::pin(async { Ok(vec![]) })
    }
    fn modify(
        &self,
        _: &str,
        _: CredentialModifier,
        _: AuthOperationOptions,
    ) -> BoxFuture<'_, Result<Option<Credential>, AuthError>> {
        Box::pin(async { Err(AuthError::failed("unexpected write")) })
    }
    fn delete(&self, _: &str, _: AuthOperationOptions) -> BoxFuture<'_, Result<(), AuthError>> {
        Box::pin(async { Err(AuthError::failed("unexpected deletion")) })
    }
}

#[tokio::test]
async fn provider_key_lookup_propagates_the_native_public_auth_rejection() {
    let runtime = ModelRuntime::create(CreateModelRuntimeOptions {
        credentials: Some(Arc::new(RejectingCredentials)),
        models_path: Some(None),
        refresh_on_create: Some(false),
        ..Default::default()
    })
    .await
    .unwrap();
    let registry = ModelRegistry::new(runtime);
    let expected = registry.get_provider_auth("openai").await.unwrap_err();
    let error = registry.get_api_key_for_provider("openai").await.unwrap_err();
    assert_eq!(error, expected);
    assert!(error.message().contains(JsString::from_utf16(vec![0xd800])));
}
