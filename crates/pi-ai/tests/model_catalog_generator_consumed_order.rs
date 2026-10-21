use pi_ai::model_catalog_generator::{Generator, GeneratorOptions, HttpResponse, process_baseten_models};
use pi_js_compat::js_json::{JsValue, parse_lossless};

#[test]
fn baseten_rejects_reasoning_before_later_option_receivers() {
    let input = parse_lossless(r#"{"models":{"one":{"reasoning":"yes","reasoning_options":3}}}"#).unwrap();
    let error = process_baseten_models(&input).unwrap_err();
    assert_eq!(error.name, "TypeError");
    assert_eq!(
        error.message.to_utf8().unwrap(),
        "models.dev reasoning must be a boolean"
    );
}

#[test]
fn a_late_consumed_field_failure_discards_models_inside_the_fetch_catch() {
    for strict in [false, true] {
        let mut requests = Vec::new();
        let mut fetch = |url: &str| {
            requests.push(url.to_owned());
            Ok(HttpResponse {
                status: 200,
                body: r#"{"openai":{"models":{"first":{"tool_call":true,"reasoning_options":[{"type":"effort","values":["high"]}]},"second":{"tool_call":true,"reasoning":"yes"}}}}"#.to_owned(),
            })
        };
        let mut generator = Generator::new(
            GeneratorOptions {
                strict,
                ..Default::default()
            },
            &mut fetch,
        );
        let result = generator.load_models_dev_data();
        if strict {
            let error = result.unwrap_err();
            assert_eq!(error.name, "TypeError");
            assert_eq!(
                error.message.to_utf8().unwrap(),
                "models.dev reasoning must be a boolean"
            );
        } else {
            assert_eq!(result.unwrap(), JsValue::Array(vec![]));
        }
        assert_eq!(generator.logs.len(), 2);
        assert_eq!(requests, ["https://models.dev/api.json"]);
    }
}
