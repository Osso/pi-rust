//! Differential proof for `core/model-config.ts`: provider maps and validation errors must match the
//! pinned TypeBox-compiled module byte-for-byte.

#[path = "support/oracle.rs"]
mod oracle;

use pi_js_compat::js_json::JsString;
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

use pi_coding_agent::core::model_config::ModelConfig;
use serde_json::json;

use oracle::TsOracle;

fn temp_dir() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("pi-model-config-{}-{unique}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn fixtures() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "valid-full",
            r#"{
  // comments are stripped
  "providers": {
    "anthropic": {
      "name": "Anthropic",
      "baseUrl": "https://api.anthropic.com",
      "apiKey": "$ANTHROPIC_KEY",
      "api": "anthropic-messages",
      "headers": {"x-extra": "1"},
      "authHeader": true,
      "models": [{
        "id": "claude",
        "name": "Claude",
        "reasoning": true,
        "thinkingLevelMap": {"off": null, "high": "high"},
        "input": ["text", "image"],
        "cost": {"input": 3, "output": 15, "cacheRead": 0.3, "cacheWrite": 3.75,
                 "tiers": [{"inputTokensAbove": 200000, "input": 6, "output": 22.5, "cacheRead": 0.6, "cacheWrite": 7.5}]},
        "contextWindow": 200000,
        "maxTokens": 8192,
        "samplingParams": {"temperature": 0.2, "nested": {"any": [1, true, null]}},
        "headers": {"x-model": "y"},
        "compat": {"supportsDeveloperRole": true, "sessionAffinityFormat": "openai"}
      }],
      "modelOverrides": {"claude": {"name": "Override", "cost": {"input": 1}, "compat": {"supportsTemperature": false}}}
    },
    "openrouter": {
      "oauth": "radius",
      "compat": {
        "thinkingFormat": "openrouter",
        "chatTemplateKwargs": {"a": "b", "c": 1, "d": true, "e": null, "f": {"$var": "thinking.effort", "omitWhenOff": true}},
        "openRouterRouting": {"data_collection": "deny", "sort": {"by": "price", "partition": null}, "max_price": {"prompt": 1, "completion": "2"}},
        "preferred_min_throughput": {"p50": 10},
        "preferred_max_latency": 5
      }
    }
  }
}"#,
        ),
        (
            "numeric-provider-order",
            r#"{"providers":{"2":{},"1":{},"z":{},"01":{},"4294967295":{}}}"#,
        ),
        ("missing-providers", r#"{"other": true}"#),
        ("providers-not-object", r#"{"providers": []}"#),
        ("invalid-json", r#"{"providers": }"#),
        ("empty-name", r#"{"providers": {"p": {"name": ""}}}"#),
        (
            "model-missing-id",
            r#"{"providers": {"p": {"models": [{"name": "m"}]}}}"#,
        ),
        ("model-empty-id", r#"{"providers": {"p": {"models": [{"id": ""}]}}}"#),
        (
            "cost-missing-field",
            r#"{"providers": {"p": {"models": [{"id": "m", "cost": {"input": 1, "output": 2, "cacheRead": 3}}]}}}"#,
        ),
        (
            "cost-wrong-type",
            r#"{"providers": {"p": {"models": [{"id": "m", "cost": {"input": "1", "output": 2, "cacheRead": 3, "cacheWrite": 4}}]}}}"#,
        ),
        (
            "input-invalid-enum",
            r#"{"providers": {"p": {"models": [{"id": "m", "input": ["text", "video"]}]}}}"#,
        ),
        (
            "thinking-map-wrong-type",
            r#"{"providers": {"p": {"models": [{"id": "m", "thinkingLevelMap": {"high": 5}}]}}}"#,
        ),
        (
            "compat-invalid",
            r#"{"providers": {"p": {"compat": {"thinkingFormat": "nope"}}}}"#,
        ),
        (
            "sampling-params-not-object",
            r#"{"providers": {"p": {"models": [{"id": "m", "samplingParams": []}]}}}"#,
        ),
        ("headers-not-string", r#"{"providers": {"p": {"headers": {"a": 1}}}}"#),
        (
            "override-bad-cost",
            r#"{"providers": {"p": {"modelOverrides": {"m": {"cost": {"input": "x"}}}}}}"#,
        ),
        ("oauth-invalid", r#"{"providers": {"p": {"oauth": "other"}}}"#),
        (
            "tier-missing-field",
            r#"{"providers": {"p": {"models": [{"id": "m", "cost": {"input": 1, "output": 2, "cacheRead": 3, "cacheWrite": 4, "tiers": [{"inputTokensAbove": 1, "input": 2}]}}]}}}"#,
        ),
        (
            "unknown-properties",
            r#"{"providers": {"p": {"unknown": 1, "models": [{"id": "m", "extra": true}]}}}"#,
        ),
        ("empty-object", "{}"),
        ("array-root", "[]"),
        (
            "multiple-errors",
            r#"{"providers": {"p": {"models": [{"id": ""}, {}]}}}"#,
        ),
    ]
}

#[test]
fn model_config_matches_the_pinned_typescript_module() {
    let dir = temp_dir();
    let mut oracle = TsOracle::spawn(
        "model-config-oracle.ts",
        "packages/coding-agent/src/core/model-config.ts",
    );

    let missing = dir.join("missing.json");
    let response = oracle.request(&json!({"id": "missing", "path": missing}));
    assert_eq!(response["ok"], true, "{response}");
    let config = ModelConfig::load(Some(&JsString::from(&missing.display().to_string())));
    assert_eq!(config.get_provider_ids(), Vec::<String>::new());
    assert_eq!(json!(config.get_error()), response["output"]["error"]);

    for (index, (name, content)) in fixtures().iter().enumerate() {
        let path = dir.join(format!("{name}.json"));
        fs::write(&path, content).unwrap();
        let response = oracle.request(&json!({"id": index, "path": path}));
        assert_eq!(response["ok"], true, "{name}: {response}");
        let config = ModelConfig::load(Some(&JsString::from(&path.display().to_string())));
        let expected = &response["output"];
        assert_eq!(
            json!(config.get_provider_ids()),
            expected["providerIds"],
            "{name}: provider ids"
        );
        let providers: std::collections::BTreeMap<String, pi_js_compat::js_json::JsValue> = config
            .get_provider_ids()
            .into_iter()
            .filter_map(|id| config.get_provider(&id).map(|provider| (id, provider.clone())))
            .collect();
        assert_eq!(json!(providers), expected["providers"], "{name}: providers");
        assert_eq!(json!(config.get_error()), expected["error"], "{name}: error");
    }
}

#[test]
fn focused_model_config_behavior() {
    let dir = temp_dir();
    let path = dir.join("models.json");
    fs::write(
        &path,
        r#"{"providers": {"zeta": {"name": "Z"}, "alpha": {"name": "A"}}}"#,
    )
    .unwrap();
    let config = ModelConfig::load(Some(&JsString::from(&path.display().to_string())));
    assert_eq!(config.get_error(), None);
    assert_eq!(config.get_provider_ids(), ["zeta", "alpha"]);
    assert_eq!(
        config.get_provider("zeta").unwrap()["name"],
        pi_js_compat::js_json::JsValue::String("Z".into())
    );
    assert_eq!(config.get_provider("missing"), None);

    let missing = dir.join("none.json");
    let config = ModelConfig::load(Some(&JsString::from(&missing.display().to_string())));
    assert!(config.get_error().is_none());
    assert!(config.get_provider_ids().is_empty());

    let config = ModelConfig::load(None);
    assert!(config.get_error().is_none());

    let invalid = dir.join("invalid.json");
    fs::write(&invalid, "{").unwrap();
    let config = ModelConfig::load(Some(&JsString::from(&invalid.display().to_string())));
    let error = config.get_error().expect("parse error");
    assert!(error.starts_with("Failed to parse models.json: "), "error: {error:?}");
    assert!(error.contains(&format!("\n\nFile: {}", invalid.display())));

    let schema = dir.join("schema.json");
    fs::write(&schema, r#"{"providers": []}"#).unwrap();
    let config = ModelConfig::load(Some(&JsString::from(&schema.display().to_string())));
    let error = config.get_error().expect("schema error");
    assert!(error.starts_with("Invalid models.json schema:\n"), "error: {error:?}");
}
