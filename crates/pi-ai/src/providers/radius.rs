//! Port of `packages/ai/src/providers/radius.ts`.

use std::sync::{Arc, Mutex};

use crate::api::pi_messages::pi_messages_api;
use crate::auth::oauth::load::load_radius_oauth;
use crate::auth::oauth::radius::RadiusOAuthOptions;
use crate::auth::{Credential, LazyOAuthInput, ProviderAuth, env_api_key_auth, lazy_oauth};
use crate::models::{ModelsPublication, PersistAction, Provider, PublishFn, RefreshModelsContext};
use crate::models_store::ModelsStoreEntry;
use crate::providers::radius_config::{
    DEFAULT_RADIUS_GATEWAY, get_radius_models, get_radius_models_from_config, load_radius_gateway_config,
    normalize_radius_gateway_url,
};
use crate::request::{
    BoxFuture, CallbackError, Fetch, ProviderStreams, SimpleStreamOptions, StreamOptions, now_millis,
};
use crate::types::{Context, Model};
use crate::utils::event_stream::AssistantMessageEventStream;

/// `RadiusProviderOptions`.
#[derive(Debug, Clone, Default)]
pub struct RadiusProviderOptions {
    pub id: Option<String>,
    pub name: Option<String>,
    pub gateway: Option<String>,
}

/// Catalog state shared by `getModels` and `refreshModels`.
struct RadiusState {
    id: String,
    gateway: String,
    models: Arc<Mutex<Vec<Model>>>,
    fetch: Arc<dyn Fetch>,
}

impl RadiusState {
    fn current_models(&self) -> Vec<Model> {
        self.models
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    async fn publish_update(&self, publish: &PublishFn, models: Vec<Model>) -> Result<bool, CallbackError> {
        let update = Arc::clone(&self.models);
        publish(ModelsPublication::default().with_update(move || {
            *update.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = models.clone();
        }))
        .await
    }

    async fn publish_persist(&self, publish: &PublishFn, models: Vec<Model>) -> Result<bool, CallbackError> {
        let update = Arc::clone(&self.models);
        let entry = ModelsStoreEntry {
            models: models.clone(),
            last_modified: None,
            checked_at: Some(now_millis() as f64),
            etag: None,
        };
        publish(
            ModelsPublication::new(PersistAction::Write(entry)).with_update(move || {
                *update.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = models.clone();
            }),
        )
        .await
    }

    /// `refreshModels(context)`.
    async fn refresh(&self, context: RefreshModelsContext) -> Result<(), CallbackError> {
        if let Some(stored) = context.stored.as_ref() {
            let restored: Vec<Model> = stored
                .models
                .iter()
                .filter(|model| model.provider == self.id)
                .cloned()
                .collect();
            if !self.publish_update(&context.publish, restored).await? {
                return Ok(());
            }
        }

        // Import catalogs cached by the pre-ModelsStore Radius implementation.
        if context.stored.is_none()
            && let Some(Credential::OAuth(credential)) = context.credential.as_ref()
        {
            let legacy = get_radius_models(&self.id, Some(credential));
            if !legacy.is_empty() && !self.publish_persist(&context.publish, legacy).await? {
                return Ok(());
            }
        }

        if !context.allow_network || context.signal.is_cancelled() {
            return Ok(());
        }
        let api_key = match context.credential.as_ref() {
            Some(Credential::OAuth(credential)) => Some(credential.access.clone()),
            Some(Credential::ApiKey(credential)) => credential.key.clone(),
            None => None,
        };
        let config = load_radius_gateway_config(
            &self.fetch,
            &self.gateway,
            api_key.as_deref(),
            Some(context.signal.clone()),
        )
        .await?;
        if context.signal.is_cancelled() {
            return Ok(());
        }
        let refreshed = get_radius_models_from_config(&self.id, &config);
        self.publish_persist(&context.publish, refreshed).await?;
        Ok(())
    }
}

/// Radius gateway provider with a persisted, dynamically refreshed catalog.
pub struct RadiusProvider {
    id: String,
    name: String,
    auth: ProviderAuth,
    state: Arc<RadiusState>,
    streams: Arc<dyn ProviderStreams>,
}

/// `radiusProvider(options)`.
pub fn radius_provider(options: RadiusProviderOptions) -> Arc<dyn Provider> {
    radius_provider_with_fetch(options, crate::http::default_fetch())
}

/// [`radius_provider`] with an injected HTTP implementation (the TypeScript
/// module uses `globalThis.fetch`).
pub fn radius_provider_with_fetch(options: RadiusProviderOptions, fetch: Arc<dyn Fetch>) -> Arc<dyn Provider> {
    let id = options.id.unwrap_or_else(|| "radius".to_owned());
    let name = options.name.unwrap_or_else(|| "Radius".to_owned());
    let gateway = normalize_radius_gateway_url(options.gateway.as_deref().unwrap_or(DEFAULT_RADIUS_GATEWAY));
    let models = get_radius_models(&id, None);
    let oauth_name = name.clone();
    let oauth_gateway = gateway.clone();
    let auth = ProviderAuth {
        api_key: Some(Arc::new(env_api_key_auth("Radius API key", &["RADIUS_API_KEY"]))),
        oauth: Some(Arc::new(lazy_oauth(LazyOAuthInput {
            name: name.clone(),
            is_subscription: None,
            login_label: None,
            load: Arc::new(move || {
                let options = RadiusOAuthOptions {
                    name: oauth_name.clone(),
                    gateway: oauth_gateway.clone(),
                };
                Box::pin(async move { Ok(load_radius_oauth(options)) })
            }),
        }))),
    };
    Arc::new(RadiusProvider {
        state: Arc::new(RadiusState {
            id: id.clone(),
            gateway,
            models: Arc::new(Mutex::new(models)),
            fetch,
        }),
        id,
        name,
        auth,
        streams: Arc::new(pi_messages_api()),
    })
}

impl Provider for RadiusProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn auth(&self) -> &ProviderAuth {
        &self.auth
    }

    fn get_models(&self) -> Vec<Model> {
        self.state.current_models()
    }

    fn supports_refresh_models(&self) -> bool {
        true
    }

    fn refresh_models(&self, context: RefreshModelsContext) -> Option<BoxFuture<'static, Result<(), CallbackError>>> {
        let state = Arc::clone(&self.state);
        Some(Box::pin(async move { state.refresh(context).await }))
    }

    fn stream(&self, model: Model, context: Context, options: StreamOptions) -> AssistantMessageEventStream {
        self.streams.stream(model, context, options)
    }

    fn stream_simple(
        &self,
        model: Model,
        context: Context,
        options: SimpleStreamOptions,
    ) -> AssistantMessageEventStream {
        self.streams.stream_simple(model, context, options)
    }
}
