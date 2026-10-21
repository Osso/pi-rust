use pi_ai::model_catalog_generator::{Generator, GeneratorOptions, HttpResponse, process_baseten_models};
use pi_js_compat::js_json::JsValue;
use serde_json::json;

#[test]
fn consumed_reasoning_controls_reject_missing_required_fields() {
    for (option, message) in [
        (json!({}), "models.dev reasoning option.type must be a string"),
        (
            json!({"type":null}),
            "models.dev reasoning option.type must be a string",
        ),
        (json!({"type":"effort"}), "models.dev effort values must be an array"),
    ] {
        let input = JsValue::from(&json!({"models":{"one":{"reasoning_options":[option]}}}));
        let error = process_baseten_models(&input).unwrap_err();
        assert_eq!(error.name, "TypeError");
        assert_eq!(error.message.to_utf8().unwrap(), message);
    }
}

#[test]
fn missing_deferred_controls_reject_after_all_fetches() {
    for (option, message) in [
        (json!({}), "models.dev reasoning option.type must be a string"),
        (json!({"type":"effort"}), "models.dev effort values must be an array"),
    ] {
        let body = json!({"openai":{"models":{"one":{"tool_call":true,"reasoning_options":[option]}}}}).to_string();
        let mut requests = Vec::new();
        let mut fetch = |url: &str| {
            requests.push(url.to_owned());
            Ok(HttpResponse {
                status: 200,
                body: if url.contains("models.dev") {
                    body.clone()
                } else {
                    "{\"data\":[]}".to_owned()
                },
            })
        };
        let mut generator = Generator::new(GeneratorOptions::default(), &mut fetch);
        let error = generator.generate_catalog().unwrap_err();
        assert_eq!(error.name, "TypeError");
        assert_eq!(error.message.to_utf8().unwrap(), message);
        assert_eq!(generator.logs.len(), 6);
        assert_eq!(requests.len(), 3);
    }
}

#[test]
fn short_circuited_reasoning_controls_remain_unvalidated() {
    for input in [
        json!({"models":{"one":{"status":"deprecated","reasoning_options":[{}]}}}),
        json!({"models":{"one":{"reasoning_options":[{"type":"toggle"},{"type":"effort"},{}]}}}),
        json!({"models":{"one":{"reasoning_options":[{"type":"unknown"},{"type":"budget_tokens","min":{},"max":false}]}}}),
    ] {
        process_baseten_models(&JsValue::from(&input)).unwrap();
    }
}
