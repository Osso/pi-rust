//! Port of `packages/ai/src/providers/github-copilot.ts`.

use std::collections::BTreeSet;
use std::sync::Arc;

use indexmap::IndexMap;
use serde_json::Value;

use crate::api::anthropic_messages_lazy::anthropic_messages_api;
use crate::api::openai_completions::open_ai_completions_api;
use crate::api::openai_responses::open_ai_responses_api;
use crate::auth::oauth::load::load_github_copilot_oauth;
use crate::auth::{Credential, LazyOAuthInput, ProviderAuth, env_api_key_auth, lazy_oauth};
use crate::models::{FilterModelsFn, Provider, ProviderApi};
use crate::providers::{CustomProviderSpec, custom_provider, oauth_loader};
use crate::request::ProviderStreams;
use crate::types::Model;

/// `filterModels`: restricts the catalog to the account's picker models.
fn filter_models(models: Vec<Model>, credential: Option<&Credential>) -> Vec<Model> {
    let Some(Credential::OAuth(credential)) = credential else {
        return models;
    };
    let Some(Value::Array(available_model_ids)) = credential.extra.get("availableModelIds") else {
        return models;
    };
    let mut available: BTreeSet<&str> = BTreeSet::new();
    for id in available_model_ids {
        let Some(id) = id.as_str() else {
            return models;
        };
        available.insert(id);
    }
    models
        .into_iter()
        .filter(|model| available.contains(model.id.as_str()))
        .collect()
}

/// `githubCopilotProvider()`.
pub fn github_copilot_provider() -> Arc<dyn Provider> {
    let mut api: IndexMap<String, Arc<dyn ProviderStreams>> = IndexMap::new();
    api.insert("anthropic-messages".to_owned(), Arc::new(anthropic_messages_api()));
    api.insert("openai-completions".to_owned(), Arc::new(open_ai_completions_api()));
    api.insert("openai-responses".to_owned(), Arc::new(open_ai_responses_api()));
    custom_provider(
        CustomProviderSpec {
            id: "github-copilot",
            name: "GitHub Copilot",
            base_url: Some("https://api.individual.githubcopilot.com"),
            auth: ProviderAuth {
                api_key: Some(Arc::new(env_api_key_auth(
                    "GitHub Copilot token",
                    &["COPILOT_GITHUB_TOKEN"],
                ))),
                oauth: Some(Arc::new(lazy_oauth(LazyOAuthInput {
                    name: "GitHub Copilot".to_owned(),
                    is_subscription: Some(true),
                    login_label: None,
                    load: oauth_loader(load_github_copilot_oauth),
                }))),
            },
            filter_models: Some(Arc::new(filter_models) as FilterModelsFn),
        },
        ProviderApi::Map(api),
    )
}
