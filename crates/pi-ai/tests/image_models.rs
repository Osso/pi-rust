use pi_ai::image_models::{get_image_model, get_image_models, get_image_providers};
use pi_ai::types::Modality;

#[test]
fn providers_are_listed_in_catalog_order() {
    assert_eq!(get_image_providers(), vec!["openrouter"]);
}

#[test]
fn openrouter_catalog_is_present_with_generated_values() {
    let models = get_image_models("openrouter");
    assert_eq!(models.len(), 42);
    assert!(models.iter().all(|model| model.api == "openrouter-images"));
    assert!(models.iter().all(|model| model.provider == "openrouter"));
    assert!(
        models
            .iter()
            .all(|model| model.base_url == "https://openrouter.ai/api/v1")
    );
}

#[test]
fn model_lookup_returns_generated_model_and_none_for_missing() {
    let model = get_image_model("openrouter", "black-forest-labs/flux.2-max").expect("generated model");
    assert_eq!(model.name, "Black Forest Labs: FLUX.2 Max");
    assert_eq!(model.input, vec![Modality::Text, Modality::Image]);
    assert_eq!(model.output, vec![Modality::Image]);
    assert_eq!(model.cost.rates.input, 0.0);
    assert_eq!(model.cost.rates.output, 0.0);
    assert_eq!(model.cost.rates.cache_read, 0.0);
    assert_eq!(model.cost.rates.cache_write, 0.0);

    let priced = get_image_model("openrouter", "google/gemini-3-pro-image").expect("priced model");
    assert_eq!(priced.name, "Google: Nano Banana Pro (Gemini 3 Pro Image)");
    assert_eq!(priced.output, vec![Modality::Image, Modality::Text]);
    assert_eq!(priced.cost.rates.input, 2.0);
    assert_eq!(priced.cost.rates.output, 12.0);
    assert_eq!(
        priced.cost.rates.cache_read.to_bits(),
        0x3fc9_9999_9999_9999,
        "catalog keeps the generated f64 value below 0.2"
    );
    assert_eq!(priced.cost.rates.cache_write, 0.375);

    let auto = get_image_model("openrouter", "openrouter/auto").expect("auto router");
    assert_eq!(auto.cost.rates.input, -1_000_000.0);
    assert_eq!(auto.cost.rates.output, -1_000_000.0);

    assert_eq!(get_image_model("openrouter", "missing/model"), None);
    assert_eq!(get_image_model("missing-provider", "openrouter/auto"), None);
}

#[test]
fn unknown_provider_has_no_models() {
    assert!(get_image_models("missing-provider").is_empty());
}

#[test]
fn catalog_models_are_distinct_and_sorted_by_id() {
    let models = get_image_models("openrouter");
    let mut ids: Vec<&str> = models.iter().map(|model| model.id.as_str()).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(ids, sorted);
    ids.dedup();
    assert_eq!(ids.len(), models.len());
}
