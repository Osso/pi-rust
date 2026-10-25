use crate::shared_oracle as oracle;

use pi_ai::image_models::{get_image_model, get_image_models, get_image_providers};
use pi_ai::image_models_generated::image_model_groups;
use pi_ai::providers::openrouter_images::openrouter_images_provider;
use serde_json::{Map, Value, json};

// Old native-only entries: all must be absent at the common pin, including lookups.
const REMOVED_IDS: [&str; 7] = [
    "krea/krea-2-large",
    "krea/krea-2-medium",
    "krea/krea-2-medium-turbo",
    "microsoft/mai-image-2.5-pro",
    "openrouter/auto-beta",
    "qwen/qwen-image-3",
    "qwen/qwen-image-3-pro",
];

fn catalog_value() -> Value {
    let mut catalog = Map::new();
    for (provider, models) in image_model_groups() {
        let mut entries = Map::new();
        for model in models {
            entries.insert(model.id.clone(), serde_json::to_value(model).expect("model serializes"));
        }
        catalog.insert(provider.clone(), Value::Object(entries));
    }
    Value::Object(catalog)
}

fn source_output(ts: &mut oracle::TsOracle, request: Value) -> Value {
    let response = ts.request(&request);
    assert_eq!(response["id"], request["id"]);
    assert_eq!(response["ok"], true, "{response}");
    response["output"].clone()
}

#[test]
fn image_catalog_matches_the_pinned_typescript_module() {
    let mut ts = oracle::TsOracle::spawn("image-models-oracle.ts", "packages/ai/src/image-models.ts");
    // The oracle imports the literal IMAGE_MODELS export, not a reconstructed registry.
    let source = source_output(&mut ts, json!({"id": 0, "operation": "catalog"}))["catalog"].clone();
    let native = catalog_value();
    assert_eq!(native, source, "every field, optional-field absence and membership");
    let providers: Vec<_> = source.as_object().expect("source catalog").keys().collect();
    assert_eq!(get_image_providers(), providers, "provider order");
    assert_eq!(
        source_output(&mut ts, json!({"id": 1, "operation": "providers"})),
        json!({"providers": providers})
    );

    let mut id = 2;
    let mut lookups = Vec::new();
    for (provider, entries) in source.as_object().expect("source catalog") {
        let entries = entries.as_object().expect("provider entries");
        let source_ids: Vec<_> = entries.keys().collect();
        let native_ids: Vec<_> = get_image_models(provider).iter().map(|model| &model.id).collect();
        assert_eq!(native_ids, source_ids, "model order for {provider}");
        let models: Vec<_> = entries.values().collect();
        assert_eq!(serde_json::to_value(get_image_models(provider)).unwrap(), json!(models));
        assert_eq!(
            source_output(&mut ts, json!({"id": id, "operation": "models", "provider": provider})),
            json!({"models": models})
        );
        id += 1;

        for model_id in entries.keys().map(String::as_str).chain(REMOVED_IDS) {
            let expected = entries.get(model_id).cloned().unwrap_or(Value::Null);
            let native_model = serde_json::to_value(get_image_model(provider, model_id)).unwrap();
            let actual = source_output(
                &mut ts,
                json!({"id": id, "operation": "model", "provider": provider, "modelId": model_id}),
            );
            assert_eq!(actual, json!({"model": expected}), "source lookup {model_id}");
            assert_eq!(native_model, expected, "native lookup {model_id}");
            lookups.push(json!({"provider": provider, "id": model_id, "model": native_model}));
            id += 1;
        }
    }

    for (provider, model_id) in [("openrouter", "missing/model"), ("missing-provider", "openrouter/auto")] {
        assert_eq!(get_image_model(provider, model_id), None);
        assert_eq!(
            source_output(
                &mut ts,
                json!({"id": id, "operation": "model", "provider": provider, "modelId": model_id})
            ),
            json!({"model": null})
        );
        id += 1;
    }
    assert!(get_image_models("missing-provider").is_empty());
    assert_eq!(
        source_output(
            &mut ts,
            json!({"id": id, "operation": "models", "provider": "missing-provider"})
        ),
        json!({"models": []})
    );

    // Static provider consumption only: no auth, refresh or image request.
    let provider_models = openrouter_images_provider().get_models();
    let expected_provider_models: Vec<_> = source["openrouter"].as_object().unwrap().values().collect();
    assert_eq!(
        serde_json::to_value(&provider_models).unwrap(),
        json!(expected_provider_models)
    );
    println!(
        "STATIC_IMAGE_PROOF {}",
        json!({"catalog": native, "lookups": lookups, "providerModels": provider_models})
    );
}
