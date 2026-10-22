use std::collections::BTreeMap;

use pi_ai::env_api_keys::{
    ANTHROPIC_API_KEY_ENV, ANTHROPIC_AUTH_TOKEN_ENV, ANTHROPIC_OAUTH_TOKEN_ENV, VertexAdc, find_env_keys_with,
    get_env_api_key_with,
};
use pi_ai::types::ProviderEnv;

fn env(pairs: &[(&str, &str)]) -> ProviderEnv {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect()
}

fn process(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect()
}

fn adc_missing() -> VertexAdc {
    VertexAdc::with_sources(|_| false, Some("/home/test"))
}

#[test]
fn anthropic_env_names_are_pinned() {
    assert_eq!(ANTHROPIC_AUTH_TOKEN_ENV, "ANTHROPIC_AUTH_TOKEN");
    assert_eq!(ANTHROPIC_OAUTH_TOKEN_ENV, "ANTHROPIC_OAUTH_TOKEN");
    assert_eq!(ANTHROPIC_API_KEY_ENV, "ANTHROPIC_API_KEY");
}

#[test]
fn generic_github_tokens_are_not_copilot_credentials() {
    let process_env = process(&[("GH_TOKEN", "gh-token"), ("GITHUB_TOKEN", "github-token")]);
    let scoped = None;
    let lookup = |name: &str| process_env.get(name).cloned();
    assert_eq!(find_env_keys_with("github-copilot", scoped, lookup), None);
    assert_eq!(
        get_env_api_key_with("github-copilot", scoped, lookup, &adc_missing()),
        None
    );
}

#[test]
fn copilot_credentials_come_from_copilot_github_token() {
    let process_env = process(&[
        ("COPILOT_GITHUB_TOKEN", "copilot-token"),
        ("GH_TOKEN", "gh-token"),
        ("GITHUB_TOKEN", "github-token"),
    ]);
    let lookup = |name: &str| process_env.get(name).cloned();
    assert_eq!(
        find_env_keys_with("github-copilot", None, lookup),
        Some(vec!["COPILOT_GITHUB_TOKEN".to_owned()])
    );
    assert_eq!(
        get_env_api_key_with("github-copilot", None, lookup, &adc_missing()),
        (Some("copilot-token".to_owned())).map(pi_js_compat::js_json::JsString::from)
    );
}

#[test]
fn zai_china_coding_plan_uses_its_own_env_var() {
    let scoped = env(&[("ZAI_CODING_CN_API_KEY", "zai-coding-cn-token")]);
    let lookup = |_: &str| None;
    assert_eq!(
        find_env_keys_with("zai-coding-cn", Some(&scoped), lookup),
        Some(vec!["ZAI_CODING_CN_API_KEY".to_owned()])
    );
    assert_eq!(
        get_env_api_key_with("zai-coding-cn", Some(&scoped), lookup, &adc_missing()),
        (Some("zai-coding-cn-token".to_owned())).map(pi_js_compat::js_json::JsString::from)
    );
}

#[test]
fn anthropic_reports_auth_token_but_never_returns_it_as_api_key() {
    let all = env(&[
        ("ANTHROPIC_AUTH_TOKEN", "auth-token"),
        ("ANTHROPIC_OAUTH_TOKEN", "oauth-token"),
        ("ANTHROPIC_API_KEY", "api-key"),
    ]);
    let lookup = |_: &str| None;
    assert_eq!(
        find_env_keys_with("anthropic", Some(&all), lookup),
        Some(vec![
            "ANTHROPIC_AUTH_TOKEN".to_owned(),
            "ANTHROPIC_OAUTH_TOKEN".to_owned(),
            "ANTHROPIC_API_KEY".to_owned(),
        ])
    );
    assert_eq!(
        get_env_api_key_with("anthropic", Some(&all), lookup, &adc_missing()),
        (Some("oauth-token".to_owned())).map(pi_js_compat::js_json::JsString::from)
    );

    let auth_only = env(&[("ANTHROPIC_AUTH_TOKEN", "auth-token")]);
    assert_eq!(
        find_env_keys_with("anthropic", Some(&auth_only), lookup),
        Some(vec!["ANTHROPIC_AUTH_TOKEN".to_owned()])
    );
    assert_eq!(
        get_env_api_key_with("anthropic", Some(&auth_only), lookup, &adc_missing()),
        None
    );
}

#[test]
fn anthropic_falls_back_to_oauth_then_api_key() {
    let lookup = |_: &str| None;
    let oauth_only = env(&[("ANTHROPIC_OAUTH_TOKEN", "oauth-token")]);
    assert_eq!(
        get_env_api_key_with("anthropic", Some(&oauth_only), lookup, &adc_missing()),
        (Some("oauth-token".to_owned())).map(pi_js_compat::js_json::JsString::from)
    );
    let api_only = env(&[("ANTHROPIC_API_KEY", "api-key")]);
    assert_eq!(
        get_env_api_key_with("anthropic", Some(&api_only), lookup, &adc_missing()),
        (Some("api-key".to_owned())).map(pi_js_compat::js_json::JsString::from)
    );
}

#[test]
fn empty_values_do_not_count_as_configured() {
    let process_env = process(&[("OPENAI_API_KEY", "")]);
    let lookup = |name: &str| process_env.get(name).cloned();
    assert_eq!(find_env_keys_with("openai", None, lookup), None);
    assert_eq!(get_env_api_key_with("openai", None, lookup, &adc_missing()), None);
}

#[test]
fn scoped_env_wins_over_process_env() {
    let process_env = process(&[("OPENAI_API_KEY", "process-key")]);
    let scoped = env(&[("OPENAI_API_KEY", "scoped-key")]);
    let lookup = |name: &str| process_env.get(name).cloned();
    assert_eq!(
        get_env_api_key_with("openai", Some(&scoped), lookup, &adc_missing()),
        (Some("scoped-key".to_owned())).map(pi_js_compat::js_json::JsString::from)
    );
}

#[test]
fn unknown_providers_have_no_env_keys() {
    let lookup = |name: &str| Some(format!("value-for-{name}"));
    assert_eq!(find_env_keys_with("not-a-provider", None, lookup), None);
    assert_eq!(
        get_env_api_key_with("not-a-provider", None, lookup, &adc_missing()),
        None
    );
}

#[test]
fn vertex_requires_adc_credentials_project_and_location() {
    let lookup = |name: &str| match name {
        "GOOGLE_CLOUD_PROJECT" => Some("project".to_owned()),
        "GOOGLE_CLOUD_LOCATION" => Some("us-central1".to_owned()),
        _ => None,
    };
    let with_credentials = VertexAdc::with_sources(
        |path| path.ends_with("application_default_credentials.json"),
        Some("/home/test"),
    );
    assert_eq!(
        get_env_api_key_with("google-vertex", None, lookup, &with_credentials),
        (Some("<authenticated>".to_owned())).map(pi_js_compat::js_json::JsString::from)
    );
    assert_eq!(
        get_env_api_key_with("google-vertex", None, lookup, &adc_missing()),
        None
    );

    let no_location = |name: &str| match name {
        "GOOGLE_CLOUD_PROJECT" => Some("project".to_owned()),
        _ => None,
    };
    assert_eq!(
        get_env_api_key_with("google-vertex", None, no_location, &with_credentials),
        None
    );
}

#[test]
fn vertex_explicit_credentials_path_is_checked_directly() {
    let scoped = env(&[("GOOGLE_APPLICATION_CREDENTIALS", "/creds.json")]);
    let lookup = |name: &str| match name {
        "GOOGLE_CLOUD_PROJECT" => Some("project".to_owned()),
        "GOOGLE_CLOUD_LOCATION" => Some("us-central1".to_owned()),
        _ => None,
    };
    let existing = VertexAdc::with_sources(|path| path == "/creds.json", None);
    assert_eq!(
        get_env_api_key_with("google-vertex", Some(&scoped), lookup, &existing),
        (Some("<authenticated>".to_owned())).map(pi_js_compat::js_json::JsString::from)
    );
    let missing = VertexAdc::with_sources(|_| false, None);
    assert_eq!(
        get_env_api_key_with("google-vertex", Some(&scoped), lookup, &missing),
        None
    );
}

#[test]
fn vertex_adc_result_is_cached_but_explicit_paths_bypass_the_cache() {
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = std::sync::Arc::clone(&calls);
    let adc = VertexAdc::with_sources(
        move |_| {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            false
        },
        Some("/home/test"),
    );
    let lookup = |_: &str| None;
    assert!(!adc.has_credentials(None, lookup));
    assert!(!adc.has_credentials(None, lookup));
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);

    let explicit = env(&[("GOOGLE_APPLICATION_CREDENTIALS", "/creds.json")]);
    assert!(!adc.has_credentials(Some(&explicit), lookup));
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
}

#[test]
fn bedrock_ambient_credential_sources_report_authenticated() {
    let lookup = |_: &str| None;
    let adc = adc_missing();
    for scoped in [
        env(&[("AWS_PROFILE", "default")]),
        env(&[("AWS_ACCESS_KEY_ID", "id"), ("AWS_SECRET_ACCESS_KEY", "secret")]),
        env(&[("AWS_BEARER_TOKEN_BEDROCK", "token")]),
        env(&[("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI", "/v2/credentials")]),
        env(&[("AWS_CONTAINER_CREDENTIALS_FULL_URI", "http://169.254.170.2/creds")]),
        env(&[("AWS_WEB_IDENTITY_TOKEN_FILE", "/var/run/token")]),
    ] {
        assert_eq!(
            get_env_api_key_with("amazon-bedrock", Some(&scoped), lookup, &adc),
            (Some("<authenticated>".to_owned())).map(pi_js_compat::js_json::JsString::from)
        );
    }

    let partial = env(&[("AWS_ACCESS_KEY_ID", "id")]);
    assert_eq!(
        get_env_api_key_with("amazon-bedrock", Some(&partial), lookup, &adc),
        None
    );
}

#[test]
fn api_key_env_map_covers_known_providers() {
    let cases = [
        ("ant-ling", "ANT_LING_API_KEY"),
        ("qwen-token-plan", "QWEN_TOKEN_PLAN_API_KEY"),
        ("qwen-token-plan-cn", "QWEN_TOKEN_PLAN_CN_API_KEY"),
        ("qwen-token-plan-individual", "QWEN_TOKEN_PLAN_API_KEY"),
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
        let scoped = env(&[(env_name, "key")]);
        assert_eq!(
            find_env_keys_with(provider, Some(&scoped), |_: &str| None),
            Some(vec![env_name.to_owned()]),
            "provider {provider}"
        );
    }
}
