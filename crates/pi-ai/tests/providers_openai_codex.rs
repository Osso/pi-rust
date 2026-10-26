//! Behavioral tests for `packages/ai/src/providers/openai-codex.ts`.

use crate::shared_oracle as oracle;
use pi_ai::models::Models;
use pi_ai::providers::all::{builtin_models, builtin_providers, get_builtin_models};
use pi_ai::providers::models_data;
use pi_ai::providers::openai_codex::{openai_codex_provider, openai_codex_provider_with_id};

#[test]
fn terra_catalog_and_registry_match_locked_typescript() {
    let mut ts = oracle::TsOracle::spawn(
        "codex-terra-oracle.ts",
        "packages/ai/src/providers/openai-codex.models.ts",
    );
    let expected: serde_json::Value = ts.request(&serde_json::json!({}));
    let registry = builtin_models(Default::default());
    for (index, provider) in ["openai-codex", "openai-codex-gc", "openai-codex-team"]
        .iter()
        .enumerate()
    {
        let catalog = models_data::builtin_model(provider, "gpt-6.1-terra").expect("Terra in catalog");
        assert_eq!(serde_json::to_value(&catalog).unwrap(), expected[index]);
        assert_eq!(registry.get_model(provider, "gpt-6.1-terra"), Some(catalog));
    }
}

#[test]
fn terra_generated_catalog_matches_locked_typescript() {
    use pi_ai::model_catalog_generator::{Generator, HttpResponse};
    use pi_js_compat::js_json::{JsValue, stringify_lossless};

    let mut ts = oracle::TsOracle::spawn(
        "codex-terra-oracle.ts",
        "packages/ai/src/providers/openai-codex.models.ts",
    );
    let expected: serde_json::Value = ts.request(&serde_json::json!({}));
    let mut fetch = |url: &str| {
        let body = if url == "https://models.dev/api.json" {
            "{}"
        } else {
            "{\"data\":[]}"
        };
        Ok(HttpResponse {
            status: 200,
            body: body.to_owned(),
        })
    };
    let actual = Generator::new(&mut fetch).generate_catalog().unwrap();
    for (index, provider) in ["openai-codex", "openai-codex-gc", "openai-codex-team"]
        .iter()
        .enumerate()
    {
        assert_eq!(
            stringify_lossless(&actual[*provider]["gpt-6.1-terra"]),
            stringify_lossless(&JsValue::from(&expected[index]))
        );
    }
}

#[test]
fn default_factory_matches_the_typescript_provider_descriptor() {
    let provider = openai_codex_provider();
    assert_eq!(provider.id(), "openai-codex");
    assert_eq!(provider.name(), "OpenAI Codex");
    assert_eq!(
        provider.base_url(),
        Some(&pi_js_compat::js_json::JsString::from(
            "https://chatgpt.com/backend-api"
        ))
    );
    assert!(provider.headers().is_none());

    let auth = provider.auth();
    assert!(auth.api_key.is_none());
    let oauth = auth.oauth.as_ref().expect("oauth auth");
    assert_eq!(oauth.name(), "OpenAI (ChatGPT Plus/Pro)");

    let models = provider.get_models().unwrap();
    let catalog = get_builtin_models("openai-codex");
    assert_eq!(
        models.iter().map(|model| model.id.clone()).collect::<Vec<_>>(),
        catalog.iter().map(|model| model.id.clone()).collect::<Vec<_>>()
    );
    assert!(
        models
            .iter()
            .all(|model| model.provider == "openai-codex" && model.api == "openai-codex-responses")
    );
}

#[test]
fn custom_id_rewrites_the_codex_catalog_provider() {
    let provider = openai_codex_provider_with_id("openai-codex-gc", "OpenAI Codex (GlobalComix)");
    assert_eq!(provider.id(), "openai-codex-gc");
    assert_eq!(provider.name(), "OpenAI Codex (GlobalComix)");
    assert_eq!(
        provider.base_url(),
        Some(&pi_js_compat::js_json::JsString::from(
            "https://chatgpt.com/backend-api"
        ))
    );

    let models = provider.get_models().unwrap();
    let codex_catalog = models_data::provider_models("openai-codex").expect("codex catalog");
    // The TypeScript factory spreads OPENAI_CODEX_MODELS, so the alias keeps the
    // codex catalog values (including thinkingLevelMap/compat) with a new provider.
    assert_eq!(models.len(), codex_catalog.len());
    for (model, catalog_model) in models.iter().zip(codex_catalog.iter()) {
        assert_eq!(model.id, catalog_model.id);
        assert_eq!(model.provider, "openai-codex-gc");
        assert_eq!(model.thinking_level_map, catalog_model.thinking_level_map);
        assert_eq!(model.compat, catalog_model.compat);
    }

    let gc_catalog = models_data::provider_models("openai-codex-gc").expect("gc catalog");
    // At the common pin the GC shard matches the rewritten Codex records;
    // the old-pin assertion that its thinking maps differ is retired.
    assert_eq!(models, gc_catalog);
}

#[test]
fn builtin_providers_and_models_include_both_codex_factories() {
    let ids: Vec<String> = builtin_providers()
        .iter()
        .map(|provider| provider.id().to_owned())
        .collect();
    let openai = ids.iter().position(|id| id == "openai").expect("openai");
    assert_eq!(ids[openai + 1], "openai-codex");
    assert_eq!(ids[openai + 2], "openai-codex-gc");

    let models = builtin_models(Default::default());
    assert!(models.get_model("openai-codex", "gpt-5.5").is_some());
    let alias = models.get_model("openai-codex-gc", "gpt-5.5").expect("alias model");
    assert_eq!(alias.provider, "openai-codex-gc");
    assert_eq!(alias.api, "openai-codex-responses");
}
