#[path = "support/oracle.rs"]
mod oracle;
use pi_ai::model_catalog_generator::{
    Generator, GeneratorOptions, HttpResponse, get_models_dev_cost, process_baseten_models, process_fireworks_models,
};
use pi_js_compat::js_json::{parse_lossless, stringify_lossless};
use serde_json::json;

#[test]
fn consumed_receiver_types_reject_with_native_errors_instead_of_v8_accidents() {
    let mut source = oracle::TsOracle::spawn(
        "model-catalog-generator-oracle.ts",
        "packages/ai/scripts/generate-models.ts",
    );
    let mut failures = vec![];
    for (operation, input, message) in [
        (
            "getModelsDevCost",
            r#"{"tiers":{}}"#,
            "models.dev cost.tiers must be an array",
        ),
        (
            "getModelsDevCost",
            r#"{"tiers":[null]}"#,
            "models.dev cost.tiers entries must be objects",
        ),
        (
            "processBasetenModels",
            r#"{"models":[{"name":"one"}]}"#,
            "models.dev provider.models must be an object",
        ),
        (
            "processBasetenModels",
            r#"{"models":{"one":null}}"#,
            "models.dev model must be an object",
        ),
        (
            "processBasetenModels",
            r#"{"models":{"one":{"reasoning_options":{}}}}"#,
            "models.dev reasoning_options must be an array",
        ),
        (
            "processBasetenModels",
            r#"{"models":{"one":{"reasoning_options":[null]}}}"#,
            "models.dev reasoning options must contain objects",
        ),
        (
            "processBasetenModels",
            r#"{"models":{"one":{"reasoning_options":[{"type":"effort","values":"low"}]}}}"#,
            "models.dev effort values must be an array",
        ),
        (
            "processBasetenModels",
            r#"{"models":{"one":{"reasoning_options":[{"type":"effort","values":[3,"high"]}]}}}"#,
            "models.dev effort values must contain strings or null",
        ),
        (
            "processBasetenModels",
            r#"{"models":{"one":{"modalities":{"input":"image"}}}}"#,
            "models.dev modalities.input must be an array of strings",
        ),
        (
            "processFireworksModels",
            r#"{"models":{"one":{"tool_call":true,"modalities":{"input":[3]}}}}"#,
            "models.dev modalities.input must be an array of strings",
        ),
    ] {
        let source_result = source.request(&json!({"operation":operation,"argsJson":format!("[{input}]")}));
        let value = parse_lossless(input).unwrap();
        let actual = match operation {
            "getModelsDevCost" => get_models_dev_cost(&value),
            "processBasetenModels" => process_baseten_models(&value),
            _ => process_fireworks_models(&value),
        };
        match actual {
            Ok(_) => failures.push(format!(
                "{operation} {input}: native accepted; required {message}; source {source_result}"
            )),
            Err(error) => {
                if error.name != "TypeError" || error.message.to_utf8().unwrap() != message {
                    failures.push(format!(
                        "{operation} {input}: native {error}; required {message}; source {source_result}"
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn filters_defaults_unknown_string_controls_and_unused_budget_values_stay_literal() {
    let mut source = oracle::TsOracle::spawn(
        "model-catalog-generator-oracle.ts",
        "packages/ai/scripts/generate-models.ts",
    );
    for (operation, input) in [
        (
            "processBasetenModels",
            r#"{"models":{"skip":{"status":"deprecated","reasoning_options":{},"modalities":{"input":3}},"one":{"tool_call":3,"reasoning_options":[{"type":"unknown","values":{}},{"type":"budget_tokens","min":{},"max":false},{"type":"effort","values":["high","unknown","\ud800",null]}]}}}"#,
        ),
        (
            "processBasetenModels",
            r#"{"models":{"one":{"reasoning_options":[{"type":"toggle"},{"type":"effort","values":[]},null]}}}"#,
        ),
        (
            "processFireworksModels",
            r#"{"models":{"skip":{"tool_call":false,"reasoning_options":{},"modalities":{"input":3}},"one":{"tool_call":true,"modalities":{"input":["audio"]}}}}"#,
        ),
        (
            "getModelsDevCost",
            r#"{"tiers":[{"tier":{"type":"unknown"},"input":{}},{"tier":{"type":"context"},"input":{}}]}"#,
        ),
    ] {
        let expected = source.request(&json!({"operation":operation,"argsJson":format!("[{input}]")}));
        let value = parse_lossless(input).unwrap();
        let actual = match operation {
            "getModelsDevCost" => get_models_dev_cost(&value),
            "processBasetenModels" => process_baseten_models(&value),
            _ => process_fireworks_models(&value),
        }
        .unwrap();
        assert_eq!(
            stringify_lossless(&actual),
            expected["json"].as_str().unwrap(),
            "{expected}"
        );
    }
}

#[test]
fn deferred_reasoning_type_failure_remains_outside_fetch_catches() {
    let mut source = oracle::TsOracle::spawn(
        "model-catalog-generator-oracle.ts",
        "packages/ai/scripts/generate-models.ts",
    );
    let mut failures = vec![];
    for options in [
        json!({}),
        json!(true),
        json!([null]),
        json!([{"type":"effort","values":"low"}]),
        json!([{"type":"effort","values":[3]}]),
    ] {
        let body =
            json!({"openai":{"models":{"copied":{"name":"Copied","tool_call":true,"reasoning_options":options}}}})
                .to_string();
        let expected=source.request(&json!({"operation":"generateModels","argv":["--json-only","--json-output","@OUTPUT@"],"http":{"https://models.dev/api.json":{"body":body},"https://openrouter.ai/api/v1/models":{"body":"{\"data\":[]}"},"https://ai-gateway.vercel.sh/v1/models":{"body":"{\"data\":[]}"}}}));
        let mut requests = vec![];
        let mut fetch = |url: &str| {
            requests.push(url.to_owned());
            Ok(HttpResponse {
                status: 200,
                body: if url == "https://models.dev/api.json" {
                    body.clone()
                } else {
                    "{\"data\":[]}".to_owned()
                },
            })
        };
        let mut generator = Generator::new(GeneratorOptions::default(), &mut fetch);
        match generator.generate_catalog() {
            Ok(_) => failures.push(format!("{options}: native accepted malformed reasoning controls")),
            Err(error) => {
                if error.name != "TypeError" || !error.message.to_utf8().unwrap().starts_with("models.dev ") {
                    failures.push(format!("{options}: wrong native error {error}"));
                }
            }
        }
        assert_eq!(generator.logs.len(), 6);
        assert_eq!(requests.len(), 3);
        assert_eq!(expected["requests"].as_array().unwrap().len(), 3);
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
