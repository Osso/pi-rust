use crate::shared_oauth as oauth_support;
use crate::shared_oracle as oracle;

use std::collections::BTreeMap;
use std::time::Duration;

use oauth_support::recording_fetch::{RecordingFetch, ScriptedResponse};
use oauth_support::{PromptAnswer, TestInteraction, http_get, lock_callback_port, recorded_requests, runtime};
use pi_ai::auth::oauth::anthropic::AnthropicOAuth;
use pi_ai::auth::oauth::openai_codex::OpenAICodexOAuth;
use pi_ai::auth::oauth::support::RandomSource;
use pi_ai::auth::{AuthEvent, AuthPromptKind, Credential, OAuthAuth, OAuthCredential};

use serde_json::{Value, json};

const NOW: f64 = 1_750_000_000_000.0;

#[derive(Clone)]
enum Flow {
    Anthropic,
    OpenAICodex,
}

impl Flow {
    fn module(&self) -> &'static str {
        match self {
            Self::Anthropic => "anthropic",
            Self::OpenAICodex => "openai-codex",
        }
    }
}

#[derive(Clone)]
enum Prompt {
    Text(String),
    Pending,
}

#[derive(Clone)]
struct Scenario {
    responses: Vec<(u16, String)>,
    prompts: Vec<Prompt>,
    callback_code: Option<String>,
}

impl Scenario {
    fn new(responses: Vec<(u16, String)>, prompts: Vec<Prompt>) -> Self {
        Self {
            responses,
            prompts,
            callback_code: None,
        }
    }

    fn with_callback(mut self, code: &str) -> Self {
        self.callback_code = Some(code.to_owned());
        self
    }

    fn oracle_value(&self) -> Value {
        json!({
            "responses": self.responses.iter().map(|(status, body)| json!({"status": status, "body": body})).collect::<Vec<_>>(),
            "prompts": self.prompts.iter().map(|prompt| match prompt {
                Prompt::Text(text) => json!({"text": text}),
                Prompt::Pending => json!({"pending": true}),
            }).collect::<Vec<_>>(),
            "callback": self.callback_code.as_ref().map(|code| json!({"code": code})),
        })
    }
}

fn prompt_answers(prompts: &[Prompt]) -> Vec<PromptAnswer> {
    prompts
        .iter()
        .map(|prompt| match prompt {
            Prompt::Text(text) => PromptAnswer::Text(text.clone()),
            Prompt::Pending => PromptAnswer::Pending,
        })
        .collect()
}

fn prompt_json(kind: &AuthPromptKind) -> Value {
    match kind {
        AuthPromptKind::Text { message, placeholder } => json!({
            "type": "text", "message": message, "placeholder": placeholder,
        }),
        AuthPromptKind::Secret { message, placeholder } => json!({
            "type": "secret", "message": message, "placeholder": placeholder,
        }),
        AuthPromptKind::ManualCode { message, placeholder } => json!({
            "type": "manual_code", "message": message, "placeholder": placeholder,
        }),
        AuthPromptKind::Select { message, options } => json!({
            "type": "select",
            "message": message,
            "options": options.iter().map(|option| json!({"id": option.id, "label": option.label})).collect::<Vec<_>>(),
        }),
    }
}

fn requests_json(fetch: &RecordingFetch) -> Value {
    Value::Array(
        recorded_requests(fetch)
            .into_iter()
            .map(|request| {
                let headers: BTreeMap<String, String> = request
                    .headers
                    .into_iter()
                    .map(|(name, value)| (name.to_ascii_lowercase(), value))
                    .collect();
                json!({"method": request.method, "url": request.url, "headers": headers, "body": request.body})
            })
            .collect(),
    )
}

fn credential_json(credential: &OAuthCredential) -> Value {
    serde_json::to_value(Credential::OAuth(credential.clone())).expect("credential JSON")
}

async fn wait_for_auth_url(interaction: &TestInteraction) -> String {
    for _ in 0..500 {
        if let Some(AuthEvent::AuthUrl { url, .. }) = interaction
            .events()
            .into_iter()
            .find(|event| matches!(event, AuthEvent::AuthUrl { .. }))
        {
            return pi_js_compat::js_json::encode_node_utf8(&url);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("auth_url event was not emitted");
}

fn callback_url(auth_url: &str, code: &str) -> String {
    let url = url::Url::parse(auth_url).expect("auth URL");
    if let Some(callback) = url
        .query_pairs()
        .find(|(name, _)| name == "callback_url")
        .map(|(_, value)| value.into_owned())
    {
        let mut callback = url::Url::parse(&callback).expect("callback URL");
        callback.query_pairs_mut().append_pair("code", code);
        return callback.to_string();
    }
    let redirect_uri = url
        .query_pairs()
        .find(|(name, _)| name == "redirect_uri")
        .map(|(_, value)| value.into_owned())
        .expect("redirect_uri");
    let state = url
        .query_pairs()
        .find(|(name, _)| name == "state")
        .map(|(_, value)| value.into_owned())
        .expect("state");
    let mut callback = url::Url::parse(&redirect_uri).expect("redirect URI");
    callback
        .query_pairs_mut()
        .append_pair("code", code)
        .append_pair("state", &state);
    callback.to_string()
}

async fn run_flow(flow: &Flow, scenario: &Scenario) -> Value {
    let fetch = RecordingFetch::new(
        scenario
            .responses
            .iter()
            .map(|(status, body)| ScriptedResponse::json(*status, body))
            .collect(),
    );
    let runtime = runtime(fetch.clone(), NOW);
    let interaction = TestInteraction::new(prompt_answers(&scenario.prompts));
    let oauth: std::sync::Arc<dyn OAuthAuth> = match flow {
        Flow::Anthropic => std::sync::Arc::new(AnthropicOAuth::with_runtime(runtime)),
        Flow::OpenAICodex => std::sync::Arc::new(OpenAICodexOAuth::with_runtime(runtime)),
    };

    let mut callback = Value::Null;
    let result = if let Some(code) = &scenario.callback_code {
        let login = oauth.login(interaction.clone());
        let task = tokio::spawn(login);
        let auth_url = wait_for_auth_url(&interaction).await;
        let (status, body) = http_get(&callback_url(&auth_url, code)).await;
        callback = json!({"status": status, "body": body});
        task.await.expect("login task")
    } else {
        oauth.login(interaction.clone()).await
    };

    let events = serde_json::to_value(interaction.events()).expect("events JSON");
    let prompts = Value::Array(
        interaction
            .prompts()
            .into_iter()
            .map(|prompt| prompt_json(&prompt.kind))
            .collect(),
    );
    let requests = requests_json(&fetch);
    match result {
        Ok(credential) => json!({
            "ok": true,
            "credential": credential_json(&credential),
            "events": events,
            "prompts": prompts,
            "requests": requests,
            "callback": callback,
        }),
        Err(error) => json!({
            "ok": false,
            "error": {"name": "Error", "message": error.detail().unwrap_or(&pi_js_compat::js_json::JsString::from("The operation was aborted"))},
            "events": events,
            "prompts": prompts,
            "requests": requests,
            "callback": callback,
        }),
    }
}

async fn run_refresh(flow: &Flow, credential: OAuthCredential, responses: Vec<(u16, String)>) -> Value {
    let fetch = RecordingFetch::new(
        responses
            .iter()
            .map(|(status, body)| ScriptedResponse::json(*status, body))
            .collect(),
    );
    let runtime = runtime(fetch.clone(), NOW);
    let oauth: std::sync::Arc<dyn OAuthAuth> = match flow {
        Flow::Anthropic => std::sync::Arc::new(AnthropicOAuth::with_runtime(runtime)),
        Flow::OpenAICodex => std::sync::Arc::new(OpenAICodexOAuth::with_runtime(runtime)),
    };
    let result = oauth.refresh(credential.clone()).await;
    let requests = requests_json(&fetch);
    match result {
        Ok(refreshed) => json!({"ok": true, "credential": credential_json(&refreshed), "requests": requests}),
        Err(error) => json!({
            "ok": false,
            "error": {"name": "Error", "message": error.detail().unwrap_or(&pi_js_compat::js_json::JsString::from("The operation was aborted"))},
            "requests": requests,
        }),
    }
}

fn oracle_request(oracle: &mut oracle::TsOracle, request: Value) -> Value {
    let response = oracle.request(&request);
    assert_eq!(response["ok"], json!(true), "oracle failed: {response}");
    response["output"].clone()
}

async fn compare(flow: &Flow, scenario: &Scenario, expected: &Value) {
    let actual = run_flow(flow, scenario).await;
    assert_eq!(
        normalize_ephemeral_ports(actual),
        normalize_ephemeral_ports(expected.clone()),
        "flow mismatch for {}",
        flow.module()
    );
}

/// OpenRouter binds an ephemeral loopback port, so the port is not comparable.
fn normalize_ephemeral_ports(value: Value) -> Value {
    match value {
        Value::String(text) => {
            let plain = regex::Regex::new(r"127\.0\.0\.1:\d+").expect("port regex");
            let encoded = regex::Regex::new(r"(%3A)\d+").expect("encoded port regex");
            let text = plain.replace_all(&text, "127.0.0.1:0").into_owned();
            Value::String(encoded.replace_all(&text, "${1}0").into_owned())
        }
        Value::Array(values) => Value::Array(values.into_iter().map(normalize_ephemeral_ports).collect()),
        Value::Object(object) => Value::Object(
            object
                .into_iter()
                .map(|(key, value)| (key, normalize_ephemeral_ports(value)))
                .collect(),
        ),
        other => other,
    }
}

pub(crate) fn run() {
    tokio::runtime::Runtime::new().expect("runtime").block_on(run_async());
}

type RefreshCase = (Flow, OAuthCredential, Vec<(u16, String)>);

async fn run_async() {
    let mut oracle = oracle::TsOracle::spawn("oauth-flows-oracle.mts", "packages/ai/src");

    let manual_code = format!("http://localhost:53692/callback?code=manual-code&state={}", {
        let random = oauth_support::TestRandom::new();
        pi_ai::auth::oauth::pkce::generate_pkce_with(|bytes| RandomSource::fill(random.as_ref(), bytes))
            .unwrap()
            .verifier
    });

    let scenarios: Vec<(Flow, Scenario)> = vec![
        (
            Flow::Anthropic,
            Scenario::new(
                vec![(
                    200,
                    r#"{"access_token":"access-token","refresh_token":"refresh-token","expires_in":3600}"#.to_owned(),
                )],
                vec![Prompt::Text(manual_code.clone())],
            ),
        ),
        (
            Flow::Anthropic,
            Scenario::new(
                vec![(
                    200,
                    r#"{"access_token":"access-token","refresh_token":"refresh-token","expires_in":3600}"#.to_owned(),
                )],
                vec![Prompt::Pending],
            )
            .with_callback("server-code"),
        ),
        (
            Flow::OpenAICodex,
            Scenario::new(
                vec![
                    (
                        200,
                        r#"{"device_auth_id":"device-auth-id","user_code":"ABCD-1234","interval":"0"}"#.to_owned(),
                    ),
                    (
                        200,
                        r#"{"authorization_code":"oauth-code","code_verifier":"device-code-verifier"}"#.to_owned(),
                    ),
                    (
                        200,
                        json!({
                            "access_token": access_token("account-123"),
                            "refresh_token": "refresh-token",
                            "expires_in": 3600
                        })
                        .to_string(),
                    ),
                ],
                vec![Prompt::Text("device_code".to_owned())],
            ),
        ),
    ];

    for (index, (flow, scenario)) in scenarios.iter().enumerate() {
        if scenario.callback_code.is_some() {
            let _port = lock_callback_port().await;
            let expected = oracle_request(
                &mut oracle,
                json!({"id": index, "operation": "flow", "flow": flow.module(), "scenario": scenario.oracle_value()}),
            );
            compare(flow, scenario, &expected).await;
        } else {
            let expected = oracle_request(
                &mut oracle,
                json!({"id": index, "operation": "flow", "flow": flow.module(), "scenario": scenario.oracle_value()}),
            );
            compare(flow, scenario, &expected).await;
        }
    }

    let refreshes: Vec<RefreshCase> = vec![
        (
            Flow::Anthropic,
            OAuthCredential::new("old-refresh", "old-access", 0.0),
            vec![(
                200,
                r#"{"access_token":"new-access","refresh_token":"new-refresh","expires_in":3600}"#.to_owned(),
            )],
        ),
        (
            Flow::OpenAICodex,
            OAuthCredential::new("old-refresh", "old-access", 0.0),
            vec![(
                200,
                json!({
                    "access_token": access_token("account-refresh"),
                    "refresh_token": "new-refresh",
                    "expires_in": 3600
                })
                .to_string(),
            )],
        ),
    ];
    for (index, (flow, credential, responses)) in refreshes.iter().enumerate() {
        let credential_value = credential_json(credential);
        let expected = oracle_request(
            &mut oracle,
            json!({
                "id": 100 + index,
                "operation": "refresh",
                "flow": flow.module(),
                "credential": credential_value,
                "responses": responses.iter().map(|(status, body)| json!({"status": status, "body": body})).collect::<Vec<_>>(),
            }),
        );
        let actual = run_refresh(flow, credential.clone(), responses.clone()).await;
        assert_eq!(actual, expected, "refresh mismatch for {}", flow.module());
    }
}

fn access_token(account_id: &str) -> String {
    let header = base64(br#"{"alg":"none"}"#);
    let payload = base64(
        json!({"https://api.openai.com/auth": {"chatgpt_account_id": account_id}})
            .to_string()
            .as_bytes(),
    );
    format!("{header}.{payload}.signature")
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let a = chunk[0] as u32;
        let b = chunk.get(1).copied().unwrap_or(0) as u32;
        let c = chunk.get(2).copied().unwrap_or(0) as u32;
        let value = (a << 16) | (b << 8) | c;
        out.push(ALPHABET[((value >> 18) & 63) as usize] as char);
        out.push(ALPHABET[((value >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[((value >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(value & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}
