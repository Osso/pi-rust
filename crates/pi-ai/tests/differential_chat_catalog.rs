//! Only frozen models.generated.ts and its literal shards; not factory/all.ts parity.
use pi_ai::providers::all::{get_builtin_model, get_builtin_models, get_builtin_providers};
use pi_ai::providers::openai_codex::openai_codex_provider_with_id;
use pi_ai::types::Model;
use serde_json::{Value, json};

use crate::shared_oracle as oracle;

fn source_export() -> Value {
    let mut ts = oracle::TsOracle::spawn("chat-catalog-oracle.ts", "packages/ai/src/models.generated.ts");
    let response = ts.request(&json!({"id": 1}));
    assert_eq!(response["ok"], true);
    response["output"].clone()
}

fn source() -> Value {
    source_export()["models"].clone()
}

#[test]
fn artifact_provenance_matches_actual_import_graph_and_payload_bytes() {
    use sha2::{Digest, Sha256};

    let exported = source_export();
    let manifest: Value = serde_json::from_str(include_str!("../data/providers/.manifest.json")).unwrap();
    assert_eq!(manifest["sourceFiles"], exported["provenance"]);
    assert_eq!(manifest["sourceCommit"], "4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d");
    assert_eq!(manifest["sourceExport"], "packages/ai/src/models.generated.ts::MODELS");
    assert!(manifest.get("generatedAt").is_none());
    let payloads = manifest["files"].as_object().unwrap();
    let models = exported["models"].as_object().unwrap();
    assert_eq!(
        payloads.keys().cloned().collect::<Vec<_>>(),
        models.keys().map(|key| format!("{key}.json")).collect::<Vec<_>>()
    );
    for (file, hash) in payloads {
        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("data/providers")
                .join(file),
        )
        .unwrap();
        assert_eq!(format!("{:x}", Sha256::digest(bytes)), hash.as_str().unwrap(), "{file}");
    }
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
        // At this pin the literal alias equals default+rewrite; separate keys
        // and independent lookup are still required, not factory delegation.
        assert_ne!(catalogs[alias], catalogs["openai-codex"]);
        let factory = openai_codex_provider_with_id(alias, alias);
        assert_eq!(
            serde_json::to_value(factory.get_models().unwrap()).unwrap(),
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
