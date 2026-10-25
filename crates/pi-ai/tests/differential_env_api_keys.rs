#[path = "support/oracle.rs"]
mod oracle;

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::PathBuf;

use pi_ai::env_api_keys::{VertexAdc, find_env_keys_with, get_env_api_key_with};
use pi_ai::types::ProviderEnv;
use serde_json::{Value, json};

fn process_environment(overrides: &[(&str, &str)]) -> BTreeMap<String, String> {
    let mut environment: BTreeMap<String, String> = std::env::vars().collect();
    for (name, value) in overrides {
        environment.insert((*name).to_owned(), (*value).to_owned());
    }
    environment
}

fn oracle_environment(overrides: &[(&str, &str)]) -> Vec<(String, OsString)> {
    overrides
        .iter()
        .map(|(name, value)| ((*name).to_owned(), OsString::from(value)))
        .collect()
}

fn as_os_pairs(pairs: &[(String, OsString)]) -> Vec<(&str, OsString)> {
    pairs
        .iter()
        .map(|(name, value)| (name.as_str(), value.clone()))
        .collect()
}

fn scoped(value: &Value) -> Option<ProviderEnv> {
    value.as_object().map(|object| {
        object
            .iter()
            .map(|(key, value)| {
                (
                    key.clone(),
                    pi_js_compat::js_json::JsString::from(value.as_str().expect("string")),
                )
            })
            .collect()
    })
}

fn compare(overrides: &[(&str, &str)], requests: &[Value], vertex_home: Option<&str>) {
    let environment = process_environment(overrides);
    let os_environment = oracle_environment(overrides);
    let mut ts = oracle::TsOracle::spawn_with_environment(
        "env-api-keys-oracle.ts",
        "packages/ai/src/env-api-keys.ts",
        &as_os_pairs(&os_environment),
    );
    let lookup = |name: &str| environment.get(name).cloned();
    let vertex_home = pi_js_compat::js_json::JsString::from(vertex_home.unwrap_or("/nonexistent-pi-env-api-keys-home"));
    let adc = VertexAdc::with_sources(|path| std::fs::metadata(path).is_ok(), move || Ok(vertex_home.clone()));

    for request in requests {
        let provider = request["provider"].as_str().expect("provider");
        let env = scoped(&request["env"]);
        let observed = if request["operation"] == "find" {
            json!({
                "id": request["id"],
                "ok": true,
                "output": {"keys": find_env_keys_with(provider, env.as_ref(), lookup)}
            })
        } else {
            json!({
                "id": request["id"],
                "ok": true,
                "output": {"key": get_env_api_key_with(provider, env.as_ref(), lookup, &adc).unwrap()}
            })
        };
        assert_eq!(observed, ts.request(request), "request {request}");
    }
}

fn provider_matrix() -> Vec<Value> {
    let mut requests = Vec::new();
    let mut id = 0;
    let cases = [
        ("github-copilot", "COPILOT_GITHUB_TOKEN"),
        ("ant-ling", "ANT_LING_API_KEY"),
        ("qwen-token-plan", "QWEN_TOKEN_PLAN_API_KEY"),
        ("qwen-token-plan-cn", "QWEN_TOKEN_PLAN_CN_API_KEY"),
        ("qwen-token-plan-individual", "QWEN_TOKEN_PLAN_API_KEY"),
        ("openai", "OPENAI_API_KEY"),
        ("nvidia", "NVIDIA_API_KEY"),
        ("deepseek", "DEEPSEEK_API_KEY"),
        ("google", "GEMINI_API_KEY"),
        ("google-vertex", "GOOGLE_CLOUD_API_KEY"),
        ("groq", "GROQ_API_KEY"),
        ("cerebras", "CEREBRAS_API_KEY"),
        ("xai", "XAI_API_KEY"),
        ("radius", "RADIUS_API_KEY"),
        ("openrouter", "OPENROUTER_API_KEY"),
        ("vercel-ai-gateway", "AI_GATEWAY_API_KEY"),
        ("zai", "ZAI_API_KEY"),
        ("zai-coding-cn", "ZAI_CODING_CN_API_KEY"),
        ("mistral", "MISTRAL_API_KEY"),
        ("minimax", "MINIMAX_API_KEY"),
        ("minimax-cn", "MINIMAX_CN_API_KEY"),
        ("moonshotai", "MOONSHOT_API_KEY"),
        ("moonshotai-cn", "MOONSHOT_API_KEY"),
        ("huggingface", "HF_TOKEN"),
        ("fireworks", "FIREWORKS_API_KEY"),
        ("together", "TOGETHER_API_KEY"),
        ("baseten", "BASETEN_API_KEY"),
        ("opencode", "OPENCODE_API_KEY"),
        ("opencode-go", "OPENCODE_API_KEY"),
        ("kimi-coding", "KIMI_API_KEY"),
        ("cloudflare-workers-ai", "CLOUDFLARE_API_KEY"),
        ("cloudflare-ai-gateway", "CLOUDFLARE_API_KEY"),
        ("xiaomi", "XIAOMI_API_KEY"),
        ("xiaomi-token-plan-cn", "XIAOMI_TOKEN_PLAN_CN_API_KEY"),
        ("xiaomi-token-plan-ams", "XIAOMI_TOKEN_PLAN_AMS_API_KEY"),
        ("xiaomi-token-plan-sgp", "XIAOMI_TOKEN_PLAN_SGP_API_KEY"),
    ];
    for (provider, env_name) in cases {
        requests.push(json!({"id": id, "operation": "find", "provider": provider, "env": {env_name: "scoped-key"}}));
        id += 1;
        requests.push(json!({"id": id, "operation": "key", "provider": provider, "env": {env_name: "scoped-key"}}));
        id += 1;
        requests.push(json!({"id": id, "operation": "find", "provider": provider, "env": null}));
        id += 1;
        requests.push(json!({"id": id, "operation": "key", "provider": provider, "env": {}}));
        id += 1;
    }
    requests
}

#[test]
fn unknown_provider_matches_typescript() {
    let mut ts = oracle::TsOracle::spawn("env-api-keys-oracle.ts", "packages/ai/src/env-api-keys.ts");
    let request = json!({"id": 0, "operation": "find", "provider": "not-a-provider", "env": null});
    assert_eq!(
        json!({"id": 0, "ok": true, "output": {"keys": null}}),
        ts.request(&request)
    );
}

#[test]
fn env_api_keys_match_the_pinned_typescript_module() {
    let requests = provider_matrix();
    compare(
        &[
            ("OPENAI_API_KEY", "process-openai"),
            ("ANTHROPIC_AUTH_TOKEN", "process-auth"),
            ("ANTHROPIC_API_KEY", "process-anthropic"),
            ("AWS_PROFILE", "process-profile"),
            ("HOME", "/nonexistent-pi-env-api-keys-home"),
        ],
        &requests,
        None,
    );
}

#[test]
fn anthropic_ordering_and_auth_token_exclusion_match_typescript() {
    let requests = vec![
        json!({"id": 0, "operation": "find", "provider": "anthropic", "env": {"ANTHROPIC_AUTH_TOKEN": "a", "ANTHROPIC_OAUTH_TOKEN": "o", "ANTHROPIC_API_KEY": "k"}}),
        json!({"id": 1, "operation": "key", "provider": "anthropic", "env": {"ANTHROPIC_AUTH_TOKEN": "a", "ANTHROPIC_OAUTH_TOKEN": "o", "ANTHROPIC_API_KEY": "k"}}),
        json!({"id": 2, "operation": "find", "provider": "anthropic", "env": {"ANTHROPIC_AUTH_TOKEN": "a"}}),
        json!({"id": 3, "operation": "key", "provider": "anthropic", "env": {"ANTHROPIC_AUTH_TOKEN": "a"}}),
        json!({"id": 4, "operation": "key", "provider": "anthropic", "env": {"ANTHROPIC_AUTH_TOKEN": ""}}),
        json!({"id": 5, "operation": "find", "provider": "anthropic", "env": {"ANTHROPIC_AUTH_TOKEN": ""}}),
        json!({"id": 6, "operation": "key", "provider": "anthropic", "env": null}),
    ];
    compare(
        &[
            ("ANTHROPIC_AUTH_TOKEN", "process-auth"),
            ("ANTHROPIC_API_KEY", "process-anthropic"),
            ("HOME", "/nonexistent-pi-env-api-keys-home"),
        ],
        &requests,
        None,
    );
}

#[test]
fn bedrock_ambient_sources_match_typescript() {
    let requests = vec![
        json!({"id": 0, "operation": "key", "provider": "amazon-bedrock", "env": {"AWS_PROFILE": "default"}}),
        json!({"id": 1, "operation": "key", "provider": "amazon-bedrock", "env": {"AWS_ACCESS_KEY_ID": "id", "AWS_SECRET_ACCESS_KEY": "secret"}}),
        json!({"id": 2, "operation": "key", "provider": "amazon-bedrock", "env": {"AWS_ACCESS_KEY_ID": "id"}}),
        json!({"id": 3, "operation": "key", "provider": "amazon-bedrock", "env": {"AWS_BEARER_TOKEN_BEDROCK": "token"}}),
        json!({"id": 4, "operation": "key", "provider": "amazon-bedrock", "env": {"AWS_CONTAINER_CREDENTIALS_RELATIVE_URI": "/v2/credentials"}}),
        json!({"id": 5, "operation": "key", "provider": "amazon-bedrock", "env": {"AWS_CONTAINER_CREDENTIALS_FULL_URI": "http://169.254.170.2/creds"}}),
        json!({"id": 6, "operation": "key", "provider": "amazon-bedrock", "env": {"AWS_WEB_IDENTITY_TOKEN_FILE": "/var/run/token"}}),
        json!({"id": 7, "operation": "find", "provider": "amazon-bedrock", "env": {"AWS_PROFILE": "default"}}),
    ];
    compare(&[("HOME", "/nonexistent-pi-env-api-keys-home")], &requests, None);
}

fn temporary_home(name: &str) -> PathBuf {
    let home = std::env::temp_dir().join(format!("pi-env-api-keys-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".config/gcloud")).expect("temporary home must be creatable");
    home
}

#[test]
fn vertex_empty_credentials_fall_back_to_ambient_and_explicit_paths_bypass_the_cache() {
    let home = temporary_home("scoped-empty-adc");
    let ambient_path = home.join("ambient-credentials.json");
    std::fs::write(&ambient_path, "{}").expect("ambient credentials fixture must be writable");
    let home_text = home.to_str().expect("temporary home is UTF-8");
    let ambient_text = ambient_path.to_str().expect("ambient path is UTF-8");
    let requests = vec![
        json!({"id": 0, "operation": "key", "provider": "google-vertex", "env": {
            "GOOGLE_APPLICATION_CREDENTIALS": "", "GOOGLE_CLOUD_PROJECT": "p", "GOOGLE_CLOUD_LOCATION": "us-central1"
        }}),
        json!({"id": 1, "operation": "key", "provider": "google-vertex", "env": {
            "GOOGLE_APPLICATION_CREDENTIALS": "/nonexistent/scoped-credentials.json", "GOOGLE_CLOUD_PROJECT": "p", "GOOGLE_CLOUD_LOCATION": "us-central1"
        }}),
        json!({"id": 2, "operation": "key", "provider": "google-vertex", "env": {
            "GOOGLE_CLOUD_PROJECT": "p", "GOOGLE_CLOUD_LOCATION": "us-central1"
        }}),
    ];
    compare(
        &[
            ("HOME", home_text),
            ("GOOGLE_APPLICATION_CREDENTIALS", ambient_text),
            ("GOOGLE_CLOUD_API_KEY", ""),
        ],
        &requests,
        Some(home_text),
    );
    std::fs::remove_dir_all(home).expect("temporary home must be removable");
}

#[test]
fn vertex_adc_and_explicit_credentials_match_typescript() {
    let home = temporary_home("adc");
    let credentials = home.join(".config/gcloud/application_default_credentials.json");
    std::fs::write(&credentials, "{}").expect("ADC fixture must be writable");
    let home_text = home.to_str().expect("temporary home is UTF-8");
    let requests = vec![
        json!({"id": 0, "operation": "key", "provider": "google-vertex", "env": {"GOOGLE_CLOUD_PROJECT": "p", "GOOGLE_CLOUD_LOCATION": "us-central1"}}),
        json!({"id": 1, "operation": "key", "provider": "google-vertex", "env": {"GOOGLE_CLOUD_PROJECT": "p"}}),
        json!({"id": 2, "operation": "key", "provider": "google-vertex", "env": {"GCLOUD_PROJECT": "p", "GOOGLE_CLOUD_LOCATION": "us-central1"}}),
        json!({"id": 3, "operation": "key", "provider": "google-vertex", "env": {"GOOGLE_CLOUD_API_KEY": "vertex-key"}}),
    ];
    compare(&[("HOME", home_text)], &requests, Some(home_text));

    let missing_home = temporary_home("missing-adc");
    let missing_home_text = missing_home.to_str().expect("temporary home is UTF-8");
    let explicit_path = home.join("explicit-credentials.json");
    std::fs::write(&explicit_path, "{}").expect("explicit fixture must be writable");
    let explicit_text = explicit_path.to_str().expect("explicit path is UTF-8");
    let requests = vec![
        json!({"id": 0, "operation": "key", "provider": "google-vertex", "env": {"GOOGLE_CLOUD_PROJECT": "p", "GOOGLE_CLOUD_LOCATION": "us-central1"}}),
        json!({"id": 1, "operation": "key", "provider": "google-vertex", "env": {"GOOGLE_CLOUD_PROJECT": "p", "GOOGLE_CLOUD_LOCATION": "us-central1", "GOOGLE_APPLICATION_CREDENTIALS": explicit_text}}),
        json!({"id": 2, "operation": "key", "provider": "google-vertex", "env": {"GOOGLE_CLOUD_PROJECT": "p", "GOOGLE_CLOUD_LOCATION": "us-central1", "GOOGLE_APPLICATION_CREDENTIALS": "/nonexistent/credentials.json"}}),
    ];
    compare(&[("HOME", missing_home_text)], &requests, Some(missing_home_text));

    let _ = std::fs::remove_dir_all(home);
    let _ = std::fs::remove_dir_all(missing_home);
}
