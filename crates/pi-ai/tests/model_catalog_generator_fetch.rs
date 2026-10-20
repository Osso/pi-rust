#[path = "support/oracle.rs"]
mod oracle;

use pi_ai::model_catalog_generator::{Generator, GeneratorError, GeneratorOptions, HttpResponse};
use pi_js_compat::js_json::{JsString, JsValue, stringify_lossless};
use serde_json::json;

fn map_entries(values: indexmap::IndexMap<JsString, JsString>) -> JsValue {
    JsValue::Array(
        values
            .into_iter()
            .map(|(key, value)| JsValue::Array(vec![JsValue::String(key), JsValue::String(value)]))
            .collect(),
    )
}

#[test]
fn source_fetch_status_transport_json_errors_keep_strict_default_identity_and_logs() {
    let mut source = oracle::TsOracle::spawn(
        "model-catalog-generator-oracle.ts",
        "packages/ai/scripts/generate-models.ts",
    );
    for (operation, url) in [
        ("fetchNvidiaNimModelIds", "https://integrate.api.nvidia.com/v1/models"),
        ("loadModelsDevData", "https://models.dev/api.json"),
        ("fetchOpenRouterModels", "https://openrouter.ai/api/v1/models"),
        ("fetchAiGatewayModels", "https://ai-gateway.vercel.sh/v1/models"),
    ] {
        for strict in [false, true] {
            for failure in ["status", "transport", "json"] {
                let fixture = match failure {
                    "status" => json!({"status":503,"body":"not JSON"}),
                    "transport" => json!({"error":"offline fixture"}),
                    _ => json!({"body":"{"}),
                };
                let expected=source.request(&json!({"operation":operation,"mapEntries":operation=="fetchNvidiaNimModelIds","argv":if strict{vec!["--strict"]}else{vec![]},"http":{url:fixture}}));
                let mut requested = vec![];
                let mut fetch = |actual: &str| {
                    requested.push(actual.to_owned());
                    if failure == "transport" {
                        Err(GeneratorError::new("Error", JsString::from("offline fixture")))
                    } else {
                        Ok(HttpResponse {
                            status: if failure == "status" { 503 } else { 200 },
                            body: if failure == "status" {
                                "not JSON".into()
                            } else {
                                "{".into()
                            },
                        })
                    }
                };
                let mut generator = Generator::new(
                    GeneratorOptions {
                        strict,
                        ..Default::default()
                    },
                    &mut fetch,
                );
                let actual = match operation {
                    "fetchNvidiaNimModelIds" => generator.fetch_nvidia_nim_model_ids().map(map_entries),
                    "loadModelsDevData" => generator.load_models_dev_data(),
                    "fetchOpenRouterModels" => generator.fetch_open_router_models(),
                    _ => generator.fetch_ai_gateway_models(),
                };
                if strict {
                    let actual = actual.unwrap_err();
                    assert_eq!(actual.name, expected["error"]["name"].as_str().unwrap());
                    assert_eq!(
                        actual.message.to_utf8().unwrap(),
                        expected["error"]["message"].as_str().unwrap(),
                        "{operation} {failure}"
                    );
                } else {
                    assert_eq!(stringify_lossless(&actual.unwrap()), expected["json"].as_str().unwrap());
                }
                assert_eq!(
                    stringify_lossless(&JsValue::Array(generator.logs)),
                    expected["logs"].to_string()
                );
                assert_eq!(requested, vec![url]);
            }
        }
    }
}

#[test]
fn source_nvidia_map_preserves_original_normalized_alias_keys_and_overwrite_order() {
    let mut source = oracle::TsOracle::spawn(
        "model-catalog-generator-oracle.ts",
        "packages/ai/scripts/generate-models.ts",
    );
    for body in [
        "{}",
        r#"{"data":null}"#,
        r#"{"data":[]}"#,
        r#"{"data":[{"id":"A_B"},{"id":"a.b"},{"id":"A_B"},{"id":"10"},{"id":"2"},{"id":"\ud800_x"},{"id":"\ud801_x"}]}"#,
    ] {
        let url = "https://integrate.api.nvidia.com/v1/models";
        let expected =
            source.request(&json!({"operation":"fetchNvidiaNimModelIds","mapEntries":true,"http":{url:{"body":body}}}));
        let mut fetch = |_: &str| {
            Ok(HttpResponse {
                status: 200,
                body: body.to_owned(),
            })
        };
        let mut generator = Generator::new(GeneratorOptions::default(), &mut fetch);
        assert_eq!(
            stringify_lossless(&map_entries(generator.fetch_nvidia_nim_model_ids().unwrap())),
            expected["json"].as_str().unwrap()
        );
        assert_eq!(
            stringify_lossless(&JsValue::Array(generator.logs)),
            expected["logs"].to_string()
        );
    }
}

#[test]
fn source_qwen_individual_strict_exact_ids_follow_filtering_not_upstream_extra_models() {
    let mut source = oracle::TsOracle::spawn(
        "model-catalog-generator-oracle.ts",
        "packages/ai/scripts/generate-models.ts",
    );
    let ids = [
        "deepseek-v4-flash-0731",
        "deepseek-v4-pro",
        "glm-5.2",
        "qwen3.6-flash",
        "qwen3.7-max",
        "qwen3.7-plus",
        "qwen3.8-max",
    ];
    for complete in [false, true] {
        for strict in [false, true] {
            let mut models = serde_json::Map::new();
            for (index, id) in ids.iter().enumerate() {
                if !complete && index == 0 {
                    continue;
                }
                models.insert(id.to_string(), json!({"id":id,"name":id,"tool_call":true}));
            }
            for id in ["qwen3.8-max-preview", "extra-upstream-model"] {
                models.insert(id.to_owned(), json!({"id":id,"name":id,"tool_call":true}));
            }
            let body = json!({"alibaba-token-plan":{"models":models}}).to_string();
            let expected=source.request(&json!({"operation":"loadModelsDevData","argv":if strict{vec!["--strict"]}else{vec![]},"http":{"https://models.dev/api.json":{"body":body}}}));
            let mut fetch = |url: &str| {
                assert_eq!(url, "https://models.dev/api.json");
                Ok(HttpResponse {
                    status: 200,
                    body: body.clone(),
                })
            };
            let mut generator = Generator::new(
                GeneratorOptions {
                    strict,
                    ..Default::default()
                },
                &mut fetch,
            );
            let actual = generator.load_models_dev_data();
            if complete || !strict {
                assert_eq!(stringify_lossless(&actual.unwrap()), expected["json"].as_str().unwrap());
            } else {
                let actual = actual.unwrap_err();
                assert_eq!(actual.name, expected["error"]["name"].as_str().unwrap());
                assert_eq!(
                    actual.message.to_utf8().unwrap(),
                    expected["error"]["message"].as_str().unwrap()
                );
            }
            assert_eq!(
                stringify_lossless(&JsValue::Array(generator.logs)),
                expected["logs"].to_string()
            );
        }
    }
}

#[test]
fn original_automatic_cli_catch_stops_strict_fetches_and_sets_failure_exit() {
    let mut source = oracle::TsOracle::spawn(
        "model-catalog-generator-oracle.ts",
        "packages/ai/scripts/generate-models.ts",
    );
    for body in [json!({"status":503,"body":"not JSON"}), json!({"body":"{"})] {
        let expected=source.request(&json!({"operation":"cli","argv":["--strict","--json-only","--json-output","@OUTPUT@"],"http":{"https://models.dev/api.json":body}}));
        assert_eq!(expected["exitCode"], json!(1));
        assert_eq!(expected["requests"], json!(["https://models.dev/api.json"]));
        let mut logs = vec![];
        let mut fetch = |url: &str| {
            assert_eq!(url, "https://models.dev/api.json");
            Ok(HttpResponse {
                status: body["status"].as_u64().unwrap_or(200) as u16,
                body: body["body"].as_str().unwrap().to_owned(),
            })
        };
        let root = std::env::temp_dir();
        let args = vec![
            "--strict".into(),
            "--json-only".into(),
            "--json-output".into(),
            root.join("unused-model-generator-output")
                .to_string_lossy()
                .into_owned(),
        ];
        let error = pi_ai::model_catalog_generator::run_generator_cli(
            &args,
            &root,
            &JsString::from("2026-10-20T00:00:00.000Z"),
            &mut fetch,
            &mut |log| logs.push(log.clone()),
        )
        .unwrap_err();
        logs.push(JsValue::Array(vec![
            JsValue::String(JsString::from("error")),
            JsValue::String(JsString::from(error.to_string().as_str())),
        ]));
        assert_eq!(stringify_lossless(&JsValue::Array(logs)), expected["logs"].to_string());
    }
}
