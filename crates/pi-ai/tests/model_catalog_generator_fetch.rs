use crate::shared_oracle as oracle;

use pi_ai::model_catalog_generator::{Generator, HttpResponse};
use pi_js_compat::{
    js_json,
    js_json::{JsString, JsValue, stringify_lossless},
    node_error::NativeNodeError,
};
fn map_entries(values: indexmap::IndexMap<JsString, JsString>) -> JsValue {
    JsValue::Array(
        values
            .into_iter()
            .map(|(key, value)| JsValue::Array(vec![JsValue::String(key), JsValue::String(value)]))
            .collect(),
    )
}
#[test]
fn source_fetch_status_transport_json_errors_keep_default_identity_and_logs() {
    let mut source = oracle::TsOracle::spawn("generator-repin-oracle.mts", "packages/ai/scripts/generate-models.ts");
    for (operation, url) in [
        ("fetchNvidiaNimModelIds", "https://integrate.api.nvidia.com/v1/models"),
        ("loadModelsDevData", "https://models.dev/api.json"),
        ("fetchOpenRouterModels", "https://openrouter.ai/api/v1/models"),
        ("fetchAiGatewayModels", "https://ai-gateway.vercel.sh/v1/models"),
    ] {
        for (status, body, transport) in [
            (503, "not JSON", false),
            (503, "{}", false),
            (200, "{", false),
            (200, "", true),
        ] {
            let fixture = if transport {
                js_json!({"error":"offline fixture"})
            } else {
                js_json!({"status":status,"bodyText":body})
            };
            let expected = source.request(&js_json!({"operation":operation,"responses":{url:fixture}}));
            assert_eq!(expected["ok"], js_json!(true), "{expected:?}");
            let mut requested = vec![];
            let mut fetch = |actual: &str| {
                requested.push(actual.to_owned());
                if transport {
                    Err(NativeNodeError::new("Error", "offline fixture").into())
                } else {
                    Ok(HttpResponse {
                        status,
                        body: body.into(),
                    })
                }
            };
            let mut generator = Generator::new(&mut fetch);
            let actual = match operation {
                "fetchNvidiaNimModelIds" => generator.fetch_nvidia_nim_model_ids().map(map_entries),
                "loadModelsDevData" => generator.load_models_dev_data(),
                "fetchOpenRouterModels" => generator.fetch_open_router_models(),
                _ => generator.fetch_ai_gateway_models(),
            }
            .unwrap();
            assert_eq!(actual, expected["result"], "{operation} {body}");
            assert_eq!(JsValue::Array(generator.logs), expected["logs"], "{operation} {body}");
            assert_eq!(requested, vec![url]);
        }
    }
}
#[test]
fn source_nvidia_map_preserves_original_normalized_alias_keys_and_overwrite_order() {
    let mut source = oracle::TsOracle::spawn("generator-repin-oracle.mts", "packages/ai/scripts/generate-models.ts");
    for body in [
        "{}",
        r#"{"data":null}"#,
        r#"{"data":[]}"#,
        r#"{"data":[{"id":"A_B"},{"id":"a.b"},{"id":"A_B"},{"id":"10"},{"id":"2"},{"id":"\ud800_x"},{"id":"\ud801_x"}]}"#,
    ] {
        let url = "https://integrate.api.nvidia.com/v1/models";
        let expected =
            source.request(&js_json!({"operation":"fetchNvidiaNimModelIds","responses":{url:{"bodyText":body}}}));
        let mut fetch = |_: &str| {
            Ok(HttpResponse {
                status: 200,
                body: body.into(),
            })
        };
        let mut generator = Generator::new(&mut fetch);
        assert_eq!(
            stringify_lossless(&map_entries(generator.fetch_nvidia_nim_model_ids().unwrap())),
            stringify_lossless(&expected["result"])
        );
        assert_eq!(JsValue::Array(generator.logs), expected["logs"]);
    }
}
