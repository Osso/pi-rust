//! Differential test for `packages/ai/src/auth/oauth/github-copilot.ts`
//! (`refresh` and `toAuth`; the device-code login needs timers and is covered
//! behaviorally in `tests/github_copilot_oauth.rs`).

use std::sync::Arc;

use pi_ai::auth::oauth::github_copilot::GitHubCopilotOAuth;
use pi_ai::auth::oauth::support::OAuthRuntime;
use pi_ai::auth::{OAuthAuth, OAuthCredential};
use pi_ai::request::Fetch;
use serde_json::{Map, Value, json};

use crate::shared_oracle as oracle;

#[allow(dead_code)]
use crate::shared_recording_fetch as recording_fetch;

use recording_fetch::{RecordingFetch, ScriptedResponse};

const SOURCE_MODULE: &str = "packages/ai/src/utils/oauth/github-copilot.ts";

fn credential(value: &Value) -> OAuthCredential {
    let mut credential = OAuthCredential::new(
        value["refresh"].as_str().expect("refresh").to_owned(),
        value["access"].as_str().expect("access").to_owned(),
        value["expires"].as_f64().expect("expires"),
    );
    if let Some(extra) = value.as_object() {
        for (key, value) in extra {
            if matches!(key.as_str(), "refresh" | "access" | "expires" | "type") {
                continue;
            }
            credential.extra.insert(key.into(), value.into());
        }
    }
    credential
}

fn runtime(fetch: Arc<dyn Fetch>) -> OAuthRuntime {
    OAuthRuntime::new(fetch, Arc::new(|| 1_772_000_000_000.0), oauth_support_random())
}

fn oauth_support_random() -> Arc<dyn pi_ai::auth::oauth::support::RandomSource> {
    struct FixedRandom;
    impl pi_ai::auth::oauth::support::RandomSource for FixedRandom {
        fn fill(&self, bytes: &mut [u8]) -> Result<(), pi_ai::request::CallbackError> {
            for (index, byte) in bytes.iter_mut().enumerate() {
                *byte = (index % 255 + 1) as u8;
            }
            Ok(())
        }
        fn uuid(&self) -> Result<String, pi_ai::request::CallbackError> {
            Ok("00000000-0000-4000-8000-000000000000".to_owned())
        }
    }
    Arc::new(FixedRandom)
}

fn request_json(fetch: &RecordingFetch) -> Vec<Value> {
    fetch
        .requests()
        .into_iter()
        .map(|request| {
            let mut headers: Map<String, Value> = Map::new();
            for (name, value) in request.headers {
                headers.insert(name.to_lowercase(), Value::String(value));
            }
            json!({
                "url": request.url,
                "method": request.method,
                "headers": headers,
                "body": request.body.map(|body| String::from_utf8_lossy(&body).into_owned()),
            })
        })
        .collect()
}

fn refresh_cases() -> Vec<Value> {
    vec![
        json!({
            "credential": {"type": "oauth", "access": "old", "refresh": "ghu_refresh", "expires": 0},
            "token": {"token": "tid=test;exp=9999999999;proxy-ep=proxy.individual.githubcopilot.com;", "expires_at": 9999999999i64},
            "models": {"data": [
                {"id": "gpt-5.4", "model_picker_enabled": true, "capabilities": {"supports": {"tool_calls": true}}},
                {"id": "claude-opus-4.7", "model_picker_enabled": true, "policy": {"state": "disabled"}},
                {"id": "gpt-5.4-nano", "model_picker_enabled": false, "policy": {"state": "enabled"}}
            ]}
        }),
        json!({
            "credential": {"type": "oauth", "access": "old", "refresh": "ghu_refresh", "expires": 0},
            "token": {"token": "tid=test;exp=9999999999;proxy-ep=proxy.individual.githubcopilot.com;", "expires_at": 9999999999i64},
            "models": {"data": [
                {"id": "gpt-5.4", "model_picker_enabled": false, "policy": {"state": "enabled"}},
                {"id": "gpt-4o", "model_picker_enabled": true, "capabilities": {"supports": {"tool_calls": false}}}
            ]}
        }),
        json!({
            "credential": {"type": "oauth", "access": "old", "refresh": "ghu_refresh", "expires": 0},
            "token": {"token": "tid=test;exp=9999999999;proxy-ep=proxy.business.githubcopilot.com;", "expires_at": 9999999999i64},
            "models": {"data": [{"id": "gpt-5.4", "model_picker_enabled": false, "policy": {"state": "enabled"}}]}
        }),
        json!({
            "credential": {"type": "oauth", "access": "old", "refresh": "ghu_refresh", "expires": 0,
                "enterpriseUrl": "company.ghe.com"},
            "token": {"token": "plain-token", "expires_at": 1000},
            "models": {"data": []}
        }),
        json!({
            "credential": {"type": "oauth", "access": "old", "refresh": "ghu_refresh", "expires": 0},
            "token": {"token": "plain-token"},
            "models": {"data": []}
        }),
        json!({
            "credential": {"type": "oauth", "access": "old", "refresh": "ghu_refresh", "expires": 0},
            "token": {"token": "plain-token", "expires_at": 1000},
            "models": {"invalid": true}
        }),
    ]
}

#[test]
fn refresh_matches_ts() {
    let cases = refresh_cases();
    let mut ts = oracle::TsOracle::spawn("github-copilot-oauth-oracle.ts", SOURCE_MODULE);
    let response = ts.request(&json!({"id": 1, "op": "refresh", "cases": cases}));
    assert_eq!(response["ok"], true, "{response}");
    let ts_results = response["output"]["results"].as_array().expect("results").clone();

    for (case, ts_result) in cases.iter().zip(ts_results) {
        let fetch = RecordingFetch::new(vec![
            ScriptedResponse::json(200, &case["token"].to_string()),
            ScriptedResponse::json(200, &case["models"].to_string()),
        ]);
        let oauth = GitHubCopilotOAuth::with_runtime(runtime(fetch.clone() as Arc<dyn Fetch>));
        let result = tokio::runtime::Runtime::new()
            .expect("runtime")
            .block_on(oauth.refresh(credential(&case["credential"])));

        assert_eq!(
            json!(request_json(&fetch)),
            ts_result["requests"],
            "requests for case {}",
            case["credential"]
        );
        match result {
            Ok(credential) => {
                assert_eq!(
                    serde_json::to_value(pi_ai::auth::Credential::OAuth(credential)).expect("credential"),
                    ts_result["credential"],
                    "credential for case {}",
                    case["credential"]
                );
            }
            Err(error) => {
                assert_eq!(
                    json!({"name": "Error", "message": error.detail()}),
                    ts_result["error"],
                    "error for case {}",
                    case["credential"]
                );
            }
        }
    }
}

#[test]
fn to_auth_matches_ts() {
    let cases = vec![
        json!({"credential": {"type": "oauth", "access": "plain-token", "refresh": "r", "expires": 0}}),
        json!({"credential": {"type": "oauth",
            "access": "tid=test;exp=1;proxy-ep=proxy.business.githubcopilot.com;", "refresh": "r", "expires": 0}}),
        json!({"credential": {"type": "oauth", "access": "plain-token", "refresh": "r", "expires": 0,
            "enterpriseUrl": "company.ghe.com"}}),
        json!({"credential": {"type": "oauth",
            "access": "tid=test;exp=1;proxy-ep=proxy.individual.githubcopilot.com;", "refresh": "r", "expires": 0,
            "enterpriseUrl": "company.ghe.com"}}),
    ];
    let mut ts = oracle::TsOracle::spawn("github-copilot-oauth-oracle.ts", SOURCE_MODULE);
    let response = ts.request(&json!({"id": 1, "op": "toAuth", "cases": cases}));
    assert_eq!(response["ok"], true, "{response}");
    let ts_results = response["output"]["results"].as_array().expect("results").clone();

    for (case, ts_result) in cases.iter().zip(ts_results) {
        let fetch = RecordingFetch::new(Vec::new());
        let oauth = GitHubCopilotOAuth::with_runtime(runtime(fetch as Arc<dyn Fetch>));
        let auth = tokio::runtime::Runtime::new()
            .expect("runtime")
            .block_on(oauth.to_auth(credential(&case["credential"])))
            .expect("to auth");
        assert_eq!(
            serde_json::to_value(auth).expect("auth"),
            ts_result["auth"],
            "toAuth {}",
            case["credential"]
        );
    }
}
