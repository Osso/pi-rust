#[path = "support/oracle.rs"]
mod oracle;

use pi_ai::model_catalog_generator::{Generator, GeneratorOptions, HttpResponse};
use pi_js_compat::js_json::{JsValue, stringify_lossless};
use serde_json::json;

#[test]
fn source_http_pipelines_parse_pricing_filter_tools_and_retain_raw_utf16() {
    let mut source = oracle::TsOracle::spawn(
        "model-catalog-generator-oracle.ts",
        "packages/ai/scripts/generate-models.ts",
    );
    for (operation, url, body) in [
        (
            "fetchOpenRouterModels",
            "https://openrouter.ai/api/v1/models",
            r#"{"data":[{"id":"skip"},{"id":"\ud800","name":"\ud801","supported_parameters":["tools","reasoning"],"architecture":{"modality":"text+image"},"pricing":{"prompt":"0.0000001234567tail","completion":"bad","input_cache_read":" 0.00000001"},"context_length":8192,"top_provider":{"context_length":0,"max_completion_tokens":3}}]}"#,
        ),
        (
            "fetchAiGatewayModels",
            "https://ai-gateway.vercel.sh/v1/models",
            r#"{"data":[{"id":"skip","tags":"tool-use"},{"id":"a","name":"","tags":["tool-use","vision","reasoning"],"pricing":{"input":" 1e-7more","output":"bad","input_cache_read":0.1},"context_window":0,"max_tokens":2}]}"#,
        ),
        (
            "fetchAiGatewayModels",
            "https://ai-gateway.vercel.sh/v1/models",
            r#"{"data":{}}"#,
        ),
        (
            "fetchOpenRouterModels",
            "https://openrouter.ai/api/v1/models",
            r#"{"data":{}}"#,
        ),
    ] {
        let expected = source.request(&json!({"operation":operation,"http":{url:{"body":body}}}));
        let mut requests = Vec::new();
        let mut fetch = |request: &str| {
            requests.push(request.to_owned());
            Ok(HttpResponse {
                status: 200,
                body: body.to_owned(),
            })
        };
        let mut generator = Generator::new(GeneratorOptions::default(), &mut fetch);
        let actual: JsValue = if operation == "fetchOpenRouterModels" {
            generator.fetch_open_router_models().unwrap()
        } else {
            generator.fetch_ai_gateway_models().unwrap()
        };
        assert_eq!(
            stringify_lossless(&actual),
            expected["json"].as_str().unwrap(),
            "{operation}: {expected}"
        );
        assert_eq!(requests, vec![url]);
    }
}

#[test]
fn raw_http_payloads_preserve_failure_boundaries_and_error_names() {
    let mut source = oracle::TsOracle::spawn(
        "model-catalog-generator-oracle.ts",
        "packages/ai/scripts/generate-models.ts",
    );
    for gateway in [false, true] {
        for strict in [false, true] {
            let url = if gateway {
                "https://ai-gateway.vercel.sh/v1/models"
            } else {
                "https://openrouter.ai/api/v1/models"
            };
            let operation = if gateway {
                "fetchAiGatewayModels"
            } else {
                "fetchOpenRouterModels"
            };
            for body in [
                "null",
                "{",
                "{\"data\":[null]}",
                "{\"data\":[{\"id\":\"valid\",\"supported_parameters\":[\"tools\"],\"tags\":[\"tool-use\"]},null]}",
                "{\"data\":[{\"supported_parameters\":true,\"tags\":true}]}",
                "{\"data\":[{\"supported_parameters\":[\"tools\"],\"tags\":[\"tool-use\"],\"architecture\":{\"modality\":2}}]}",
                "{\"data\":[{\"supported_parameters\":[\"tools\"],\"tags\":[\"tool-use\"],\"pricing\":{\"prompt\":{\"toString\":0},\"input\":{\"toString\":0}}}]}",
                "{\"data\":[{\"supported_parameters\":[\"tools\"],\"tags\":[\"tool-use\"]}]}",
                "{\"data\":[{\"id\":\"x\",\"supported_parameters\":[\"tools\"],\"tags\":[\"tool-use\"],\"pricing\":{\"prompt\":1e400,\"input\":1e400}}]}",
            ] {
                let expected=source.request(&json!({"operation":operation,"argv":if strict{vec!["--strict"]}else{vec![]},"http":{url:{"body":body}}}));
                let mut fetch = |_: &str| {
                    Ok(HttpResponse {
                        status: 200,
                        body: body.to_owned(),
                    })
                };
                let mut generator = Generator::new(
                    GeneratorOptions {
                        strict,
                        ..Default::default()
                    },
                    &mut fetch,
                );
                let actual = if gateway {
                    generator.fetch_ai_gateway_models()
                } else {
                    generator.fetch_open_router_models()
                };
                if let Some(error) = expected.get("error") {
                    let actual = actual.unwrap_err();
                    assert_eq!(actual.name, error["name"].as_str().unwrap());
                    assert_eq!(
                        actual.message.to_utf8().unwrap(),
                        error["message"].as_str().unwrap(),
                        "{body}"
                    );
                } else {
                    assert_eq!(
                        stringify_lossless(&actual.unwrap()),
                        expected["json"].as_str().unwrap(),
                        "{body} gateway={gateway}"
                    );
                }
                assert_eq!(
                    stringify_lossless(&JsValue::Array(generator.logs)),
                    expected["logs"].to_string(),
                    "{body}"
                );
            }
        }
    }
}

#[test]
fn source_http_strict_errors_are_not_swallowed_and_default_returns_empty() {
    let mut source = oracle::TsOracle::spawn(
        "model-catalog-generator-oracle.ts",
        "packages/ai/scripts/generate-models.ts",
    );
    for strict in [false, true] {
        for (operation, url) in [
            ("fetchOpenRouterModels", "https://openrouter.ai/api/v1/models"),
            ("fetchAiGatewayModels", "https://ai-gateway.vercel.sh/v1/models"),
        ] {
            let expected = source.request(&json!({"operation":operation,"argv":if strict {vec!["--strict"]}else{vec![]},"http":{url:{"status":503,"body":"{}"}}}));
            let mut fetch = |_: &str| {
                Ok(HttpResponse {
                    status: 503,
                    body: "{}".to_owned(),
                })
            };
            let mut generator = Generator::new(
                GeneratorOptions {
                    strict,
                    ..Default::default()
                },
                &mut fetch,
            );
            let actual = if operation == "fetchOpenRouterModels" {
                generator.fetch_open_router_models()
            } else {
                generator.fetch_ai_gateway_models()
            };
            if strict {
                assert_eq!(
                    actual.unwrap_err().message.to_utf8().unwrap(),
                    expected["error"]["message"].as_str().unwrap()
                );
            } else {
                assert_eq!(stringify_lossless(&actual.unwrap()), expected["json"].as_str().unwrap());
            }
        }
    }
}
