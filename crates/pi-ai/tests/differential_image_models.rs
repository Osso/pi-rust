#[path = "support/oracle.rs"]
mod oracle;

use pi_ai::image_models::{get_image_model, get_image_models, get_image_providers};
use pi_ai::image_models_generated::image_model_groups;
use serde_json::{Map, Value, json};

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

#[test]
fn image_catalog_matches_the_pinned_typescript_module() {
    let mut ts = oracle::TsOracle::spawn("image-models-oracle.ts", "packages/ai/src/image-models.ts");

    assert_eq!(
        json!({"id": 0, "ok": true, "output": {"catalog": catalog_value()}}),
        ts.request(&json!({"id": 0, "operation": "catalog"}))
    );
    assert_eq!(
        json!({"id": 1, "ok": true, "output": {"providers": get_image_providers()}}),
        ts.request(&json!({"id": 1, "operation": "providers"}))
    );

    let mut id = 2;
    for provider in get_image_providers() {
        assert_eq!(
            json!({
                "id": id,
                "ok": true,
                "output": {"models": serde_json::to_value(get_image_models(provider)).expect("models serialize")}
            }),
            ts.request(&json!({"id": id, "operation": "models", "provider": provider}))
        );
        id += 1;
    }

    let lookups = [
        ("openrouter", "black-forest-labs/flux.2-max"),
        ("openrouter", "google/gemini-3-pro-image"),
        ("openrouter", "openrouter/auto"),
        ("openrouter", "missing/model"),
        ("missing-provider", "openrouter/auto"),
    ];
    for (provider, model_id) in lookups {
        let model = get_image_model(provider, model_id)
            .map(|model| serde_json::to_value(model).expect("model serializes"))
            .unwrap_or(Value::Null);
        assert_eq!(
            json!({"id": id, "ok": true, "output": {"model": model}}),
            ts.request(&json!({
                "id": id,
                "operation": "model",
                "provider": provider,
                "modelId": model_id
            }))
        );
        id += 1;
    }
}
