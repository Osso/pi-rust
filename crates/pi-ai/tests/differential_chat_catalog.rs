//! Only frozen models.generated.ts and its literal shards; not factory/all.ts parity.
use pi_ai::providers::all::{get_builtin_model, get_builtin_models, get_builtin_providers};
use pi_ai::providers::openai_codex::openai_codex_provider_with_id;
use pi_ai::types::Model;
use serde_json::{Value, json};

#[path = "support/oracle.rs"]
mod oracle;

fn source() -> Value {
    let mut ts = oracle::TsOracle::spawn("chat-catalog-oracle.ts", "packages/ai/src/models.generated.ts");
    let response = ts.request(&json!({"id": 1}));
    assert_eq!(response["ok"], true);
    response["output"]["models"].clone()
}

#[test]
fn every_source_record_roundtrips_native_model_without_field_loss() {
    let catalogs = source();
    for (provider, models) in catalogs.as_object().unwrap() {
        for (key, value) in models.as_object().unwrap() {
            let model: Model =
                serde_json::from_value(value.clone()).unwrap_or_else(|e| panic!("{provider}/{key}: {e}"));
            assert_eq!(
                serde_json::to_value(model).unwrap(),
                *value,
                "{provider}/{key}: full value including field omission"
            );
        }
    }
}

#[test]
fn every_literal_value_order_and_lookup_matches_source() {
    let catalogs = source();
    let providers = catalogs.as_object().unwrap();
    assert_eq!(get_builtin_providers(), providers.keys().cloned().collect::<Vec<_>>());
    let files = std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/data/providers")).unwrap();
    let mut names = files
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.'))
        .collect::<Vec<_>>();
    names.sort();
    let mut expected = providers.keys().map(|p| format!("{p}.json")).collect::<Vec<_>>();
    expected.sort();
    assert_eq!(names, expected, "no old-only or supplemental catalogs");
    for (provider, records) in providers {
        let records = records.as_object().unwrap();
        let models = get_builtin_models(provider);
        assert_eq!(
            models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            records.keys().map(String::as_str).collect::<Vec<_>>(),
            "{provider} full order/membership, including duplicate IDs"
        );
        for (model, (key, value)) in models.iter().zip(records) {
            assert_eq!(serde_json::to_value(model).unwrap(), *value, "{provider}/{key}");
            assert_eq!(
                serde_json::to_value(get_builtin_model(provider, key).unwrap()).unwrap(),
                *value,
                "lookup {provider}/{key}"
            );
        }
        assert!(get_builtin_model(provider, "__absent_model__").is_none());
    }
    for absent in [
        "__absent_provider__",
        "baseten",
        "azure-openai-responses",
        "qwen-token-plan",
        "qwen-token-plan-cn",
        "qwen-token-plan-individual",
    ] {
        assert!(get_builtin_models(absent).is_empty());
        assert!(get_builtin_model(absent, "gpt-5.4").is_none());
    }
}

#[test]
fn codex_factory_aliases_use_default_while_literal_aliases_remain_distinct_and_fresh() {
    let catalogs = source();
    for alias in ["openai-codex-gc", "openai-codex-team"] {
        let expected_literal = catalogs[alias]
            .as_object()
            .unwrap()
            .values()
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            serde_json::to_value(get_builtin_models(alias)).unwrap(),
            json!(expected_literal)
        );
        let expected_factory = catalogs["openai-codex"]
            .as_object()
            .unwrap()
            .values()
            .map(|value| {
                let mut model = value.clone();
                model["provider"] = json!(alias);
                model
            })
            .collect::<Vec<_>>();
        assert_ne!(
            expected_literal, expected_factory,
            "source distinguishes literal and operational {alias}"
        );
        let factory = openai_codex_provider_with_id(alias, alias);
        assert_eq!(
            serde_json::to_value(factory.get_models()).unwrap(),
            json!(expected_factory)
        );
        let mut first = get_builtin_models(alias);
        first[0].name = "mutated local copy".into();
        assert_eq!(
            serde_json::to_value(get_builtin_models(alias)).unwrap(),
            json!(expected_literal)
        );
    }
}
