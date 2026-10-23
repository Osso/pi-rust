//! Frozen interactive-mode login/logout option assembly.
use crate::core::model_registry::ModelRegistry;
use crate::modes::interactive::components::oauth_selector::AuthSelectorProvider;
use pi_ai::auth::types::AuthType;
use pi_js_compat::{js_format, js_json::JsString};
use std::collections::HashSet;

/// Frozen isApiKeyLoginProvider; built-in providers without a display-name entry are excluded.
pub fn is_api_key_login_provider(
    provider: &str,
    oauth_provider_ids: &HashSet<String>,
    builtins: &HashSet<String>,
) -> bool {
    if crate::core::provider_display_names::built_in_provider_display_name(provider).is_some()
        || pi_js_compat::object_prototype::inherited_property_string(provider).is_some()
    {
        return true;
    }
    if builtins.contains(provider) {
        return false;
    }
    !oauth_provider_ids.contains(provider)
}
pub fn get_login_provider_options(registry: &ModelRegistry, auth_type: Option<AuthType>) -> Vec<AuthSelectorProvider> {
    let oauth = registry.auth_storage().get_oauth_providers();
    let oauth_ids = oauth.iter().map(|provider| provider.id().to_owned()).collect();
    let builtins = pi_ai::compat::get_providers().into_iter().collect();
    let mut options: Vec<_> = oauth
        .iter()
        .map(|provider| AuthSelectorProvider::new(provider.id(), provider.name(), AuthType::Oauth))
        .collect();
    let mut seen = HashSet::new();
    for model in registry.get_all() {
        if seen.insert(model.provider.clone()) && is_api_key_login_provider(&model.provider, &oauth_ids, &builtins) {
            options.push(AuthSelectorProvider::new(
                &model.provider,
                registry.get_provider_display_name(&model.provider),
                AuthType::ApiKey,
            ));
        }
    }
    options.retain(|option| auth_type.is_none_or(|kind| option.auth_type == kind));
    options.sort_by(|a, b| pi_js_compat::collator::locale_compare_utf16(a.name.units(), b.name.units()));
    options
}
pub fn get_logout_provider_options(registry: &ModelRegistry) -> Result<Vec<AuthSelectorProvider>, JsString> {
    let mut options = Vec::new();
    for id in registry.auth_storage().list_providers() {
        let Some(credential) = registry.auth_storage().get(&id) else {
            continue;
        };
        if !pi_js_compat::js_json::is_truthy(&credential) {
            continue;
        }
        let id = id
            .to_utf8()
            .map_err(|_| JsString::from("provider ID contains an unpaired UTF-16 unit"))?;
        let auth_type = match credential
            .get("type")
            .and_then(pi_js_compat::js_json::JsValue::as_string)
        {
            Some(kind) if kind == "oauth" => AuthType::Oauth,
            Some(kind) if kind == "api_key" => AuthType::ApiKey,
            _ => return Err(js_format!("Invalid credential type for provider {}", id)),
        };
        options.push(AuthSelectorProvider::new(
            &id,
            registry.get_provider_display_name(&id),
            auth_type,
        ));
    }
    options.sort_by(|a, b| pi_js_compat::collator::locale_compare_utf16(a.name.units(), b.name.units()));
    Ok(options)
}
