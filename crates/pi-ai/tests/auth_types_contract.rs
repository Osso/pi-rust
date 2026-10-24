//! Frozen auth/types.ts:8–182: current store, model input and optional login contract.
use std::sync::Arc;

use pi_ai::auth::{
    ApiKeyAuth, ApiKeyResolveInput, AuthError, AuthModel, AuthResult, Credential, CredentialModifier, CredentialStore,
    ModelAuth, env_api_key_auth,
};
use pi_ai::request::BoxFuture;
use pi_js_compat::js_json::JsString;

struct FrozenStore;
impl CredentialStore for FrozenStore {
    fn read(&self, _id: &str) -> BoxFuture<'_, Result<Option<Credential>, AuthError>> {
        Box::pin(async { Ok(None) })
    }
    fn modify(&self, _id: &str, modifier: CredentialModifier) -> BoxFuture<'_, Result<Option<Credential>, AuthError>> {
        Box::pin(async move { modifier(None).await })
    }
    fn delete(&self, _id: &str) -> BoxFuture<'_, Result<(), AuthError>> {
        Box::pin(async { Ok(()) })
    }
}

struct ModelOnlyAuth;
impl ApiKeyAuth for ModelOnlyAuth {
    fn name(&self) -> JsString {
        JsString::from_utf16(vec![0x61, 0xd800])
    }
    fn resolve(&self, input: ApiKeyResolveInput) -> BoxFuture<'static, Result<Option<AuthResult>, AuthError>> {
        Box::pin(async move {
            let provider = match input.model {
                AuthModel::Chat(model) => model.provider,
                AuthModel::Images(model) => model.provider,
            };
            Ok(Some(AuthResult {
                auth: ModelAuth {
                    api_key: Some(provider.into()),
                    ..Default::default()
                },
                env: None,
                source: None,
            }))
        })
    }
}

#[tokio::test]
async fn store_has_only_frozen_read_modify_delete_operations() {
    let store: Arc<dyn CredentialStore> = Arc::new(FrozenStore);
    assert_eq!(store.read("p").await.unwrap(), None);
    assert_eq!(
        store
            .modify("p", Arc::new(|current| Box::pin(async move { Ok(current) })))
            .await
            .unwrap(),
        None
    );
    store.delete("p").await.unwrap();
}

#[test]
fn human_auth_names_and_optional_login_preserve_frozen_shape() {
    let expected = JsString::from_utf16(vec![0x61, 0xd800]);
    let ambient: Arc<dyn ApiKeyAuth> = Arc::new(ModelOnlyAuth);
    assert_eq!(ambient.name(), expected);
    assert!(ambient.login().is_none());
    let standard = env_api_key_auth(expected.clone(), &["KEY"]);
    assert_eq!(standard.name(), expected);
    assert!(standard.login().is_some());
}
