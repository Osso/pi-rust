//! Port of `packages/ai/src/providers/*.models.ts`.
//!
//! JSON artifacts represent evaluated provider exports, preserving model order.
use crate::types::Model;
use indexmap::IndexMap;
use pi_js_compat::js_json::{self, JsValue};

include!("model_catalogs_generated.rs");

/// `Object.keys(MODELS)`.
pub fn builtin_provider_ids() -> Vec<String> {
    PROVIDER_CATALOGS
        .iter()
        .map(|(provider, _)| (*provider).to_owned())
        .collect()
}

/// `Object.values(MODELS[provider])`, preserving generated insertion order.
pub fn provider_models(provider: &str) -> Option<Vec<Model>> {
    let (_, json) = PROVIDER_CATALOGS.iter().find(|(id, _)| *id == provider)?;
    Some(
        read_provider_catalog(json)
            .expect("embedded provider catalog must match the model contract")
            .into_values()
            .collect(),
    )
}

/// `MODELS[provider][modelId]`.
pub fn builtin_model(provider: &str, model_id: &str) -> Option<Model> {
    let (_, json) = PROVIDER_CATALOGS.iter().find(|(id, _)| *id == provider)?;
    read_provider_catalog(json)
        .expect("embedded provider catalog must match the model contract")
        .shift_remove(model_id)
}

/// Read a generated flat provider export without projecting human UTF-16 text.
/// Maintenance callers receive explicit errors rather than poisoned artifacts.
pub fn read_provider_catalog(json: &str) -> Result<IndexMap<String, Model>, String> {
    let catalog = js_json::parse_lossless(json).map_err(|error| error.to_string())?;
    let JsValue::Object(models) = catalog else {
        return Err("provider catalog must be an object".into());
    };
    js_json::object_entries_lossless(&models)
        .into_iter()
        .map(|(id, value)| {
            let key = id.to_utf8().map_err(|_| {
                format!(
                    "model key {} contains an unpaired UTF-16 unit",
                    js_json::stringify_lossless(&JsValue::String(id.clone()))
                )
            })?;
            for field in ["id", "provider", "api"] {
                if value
                    .get(field)
                    .and_then(JsValue::as_string)
                    .is_some_and(|value| value.to_utf8().is_err())
                {
                    return Err(format!("model {key} field {field} contains an unpaired UTF-16 unit"));
                }
            }
            let context = match value.get("provider").and_then(JsValue::as_string) {
                Some(provider) => format!(
                    "provider {}, model {key}",
                    js_json::stringify_lossless(&JsValue::String(provider.clone()))
                ),
                None => format!("model {key}"),
            };
            crate::utils::headers::validate_opaque_header_names(value.get("headers"), &context)?;
            let model = js_json::from_value(value).map_err(|error| format!("model {key}: {error}"))?;
            Ok((key, model))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    const FIXTURE: &str = r#"{"fixture":{"id":"fixture","name":"human\ud800name","api":"openai-completions","provider":"fixture","baseUrl":"https://example.test","reasoning":false,"input":["text"],"cost":{"input":1,"output":2,"cacheRead":0,"cacheWrite":0},"contextWindow":4096,"maxTokens":128}}"#;
    #[test]
    fn catalog_reader_preserves_escaped_human_utf16() {
        let model = read_provider_catalog(FIXTURE).unwrap().shift_remove("fixture").unwrap();
        assert_eq!(
            model.name,
            js_json::JsString::from_utf16(vec![104, 117, 109, 97, 110, 0xd800, 110, 97, 109, 101])
        );
    }
    #[test]
    fn catalog_reader_rejects_unpaired_header_names_with_provider_and_model_context() {
        for unit in [0xd800, 0xdfff] {
            let mut value = js_json::parse_lossless(FIXTURE).unwrap();
            value["fixture"]["provider"] = JsValue::String("catalog-header-provider".into());
            let name = js_json::JsString::from("X-") + js_json::JsString::from_utf16(vec![unit]);
            value["fixture"].as_object_mut().unwrap().insert(
                "headers".into(),
                JsValue::Object([(name, JsValue::String("value".into()))].into()),
            );
            let error = read_provider_catalog(&js_json::stringify_lossless(&value)).unwrap_err();
            assert!(error.contains("catalog-header-provider"), "{error}");
            assert!(error.contains("fixture"), "{error}");
            assert!(
                error.contains("header name") && error.contains("unpaired UTF-16"),
                "{error}"
            );
            assert!(!error.contains('\u{fffd}'), "{error}");
        }
    }

    #[test]
    fn catalog_reader_keeps_scalar_header_names_and_exact_header_values() {
        let mut value = js_json::parse_lossless(FIXTURE).unwrap();
        let header = js_json::JsString::from("value") + js_json::JsString::from_utf16(vec![0xd800]);
        value["fixture"].as_object_mut().unwrap().insert(
            "headers".into(),
            JsValue::Object([("X-😀".into(), JsValue::String(header.clone()))].into()),
        );
        let models = read_provider_catalog(&js_json::stringify_lossless(&value)).unwrap();
        assert_eq!(models["fixture"].headers.as_ref().unwrap().get("X-😀"), Some(&header));
    }

    #[test]
    fn catalog_reader_rejects_opaque_units_and_invalid_typed_fields_explicitly() {
        for field in ["id", "provider", "api", "contextWindow"] {
            let mut value = js_json::parse_lossless(FIXTURE).unwrap();
            value["fixture"][field] = JsValue::String(js_json::JsString::from_utf16(vec![0xd800]));
            let error = read_provider_catalog(&js_json::stringify_lossless(&value)).unwrap_err();
            assert!(error.contains("fixture"), "{error}");
        }
    }
}
