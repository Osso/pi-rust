#[path = "../support/llama_fetch.rs"]
mod llama_fetch;
#[path = "../support/oracle.rs"]
mod oracle;

use std::sync::Arc;

use pi_ai::types::ProviderEnv;
use pi_coding_agent::bundled_extensions::llama::huggingface::{HuggingFaceClient, find_hugging_face_token};
use serde_json::{Value, json};

use llama_fetch::{HandlerFetch, json_response};

const SOURCE_MODULE: &str = "packages/coding-agent/src/extensions/llama/huggingface.ts";
const RUST_BASE: &str = "http://127.0.0.1:8080";

pub(crate) fn run() {
    let directory = tempfile::tempdir().unwrap();
    let home = directory.path().to_string_lossy().into_owned();
    let mut oracle = oracle::TsOracle::spawn_with_environment(
        "llama-huggingface-oracle.ts",
        SOURCE_MODULE,
        &[("HOME", std::ffi::OsString::from(&home))],
    );
    compare_tokens(&mut oracle, &home);
    compare_search(&mut oracle);
    compare_details(&mut oracle);
}

fn compare_tokens(oracle: &mut oracle::TsOracle, home: &str) {
    let directory = tempfile::tempdir().unwrap();
    let token_path = directory.path().join("token");
    std::fs::write(&token_path, "  file-secret \n").unwrap();
    let token_path = token_path.to_string_lossy().into_owned();
    let hf_home = directory.path().to_string_lossy().into_owned();
    let cases = vec![
        json!({"name": "env", "env": {"HF_TOKEN": "  hf-secret  "}}),
        json!({"name": "env_blank", "env": {"HF_TOKEN": "   ", "HF_TOKEN_PATH": token_path}}),
        json!({"name": "hf_home", "env": {"HF_TOKEN_PATH": "/nonexistent/token", "HF_HOME": hf_home}}),
        json!({"name": "missing", "env": {"HF_TOKEN_PATH": "/nonexistent/token"}}),
    ];
    for case in cases {
        let response = oracle.request(&json!({"id": "token", "op": "token", "env": case["env"]}));
        assert_eq!(response["ok"], true, "oracle failed: {response}");
        let expected = response["output"]["token"].clone();
        let env: ProviderEnv = case["env"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(key, value)| (key.clone(), value.as_str().unwrap().to_owned()))
            .collect();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let actual = runtime.block_on(find_hugging_face_token(&env, home));
        assert_eq!(
            serde_json::to_string(&actual).unwrap(),
            serde_json::to_string(&expected).unwrap(),
            "token case {}",
            case["name"]
        );
    }
}

fn scripted_response(response: &Value) -> pi_ai::request::HttpResponse {
    let mut scripted = json_response(response["status"].as_u64().unwrap() as u16, &response["body"]);
    if let Some(extra) = response["headers"].as_object() {
        for (name, value) in extra {
            scripted
                .headers
                .push((name.clone(), value.as_str().unwrap().to_owned()));
        }
    }
    scripted
}

fn rust_search(response: &Value) -> (Value, Value) {
    let scripted = response.clone();
    let fetch = HandlerFetch::new(Arc::new(move |_| scripted_response(&scripted)));
    let client = HuggingFaceClient::with_fetch(Some("hf-secret"), Some(RUST_BASE), fetch.clone());
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let result = runtime.block_on(client.search("qwen coder", None));
    let requests = fetch
        .requests()
        .iter()
        .map(|request| {
            json!({
                "url": request.url.strip_prefix(RUST_BASE).unwrap_or(&request.url),
                "authorization": llama_fetch::header(request, "authorization"),
            })
        })
        .collect::<Vec<_>>();
    let outcome = match result {
        Ok(models) => json!({"result": serde_json::to_value(models).unwrap()}),
        Err(error) => json!({"error": error.message}),
    };
    (json!(requests), outcome)
}

fn compare_search(oracle: &mut oracle::TsOracle) {
    let cases = vec![
        json!({"status": 200, "body": [
            {"id": "owner/model-GGUF", "downloads": 1200},
            {"id": "owner/no-downloads"},
            {"downloads": 5},
            {"id": "owner/second", "downloads": 7}
        ]}),
        json!({"status": 200, "body": {"not": "an array"}}),
        json!({"status": 401, "body": {"error": "bad token"}}),
        json!({"status": 500, "body": {}}),
        json!({"status": 429, "headers": {"retry-after": "7"}, "body": {}}),
        json!({"status": 429, "headers": {"ratelimit": "limit=10;t=42"}, "body": {}}),
        json!({"status": 429, "body": {}}),
    ];
    for response in cases {
        let oracle_response = oracle.request(&json!({
            "id": "search",
            "op": "search",
            "token": "hf-secret",
            "response": response,
        }));
        assert_eq!(oracle_response["ok"], true, "oracle failed: {oracle_response}");
        let output = &oracle_response["output"];
        let oracle_requests = output["requests"]
            .as_array()
            .unwrap()
            .iter()
            .map(|request| {
                json!({
                    "url": request["url"],
                    "authorization": request["authorization"],
                })
            })
            .collect::<Vec<_>>();
        let (rust_requests, rust_outcome) = rust_search(&response);
        assert_eq!(
            serde_json::to_string(&oracle_requests).unwrap(),
            serde_json::to_string(&rust_requests).unwrap()
        );
        let oracle_outcome = if let Some(error) = output.get("error") {
            json!({"error": error})
        } else {
            json!({"result": output["result"]})
        };
        assert_eq!(
            serde_json::to_string(&oracle_outcome).unwrap(),
            serde_json::to_string(&rust_outcome).unwrap(),
            "search case {response}"
        );
    }
}

fn rust_details(response: &Value, id: &str) -> (Value, Value) {
    let scripted = response.clone();
    let fetch = HandlerFetch::new(Arc::new(move |_| scripted_response(&scripted)));
    let client = HuggingFaceClient::with_fetch(Some("hf-secret"), Some(RUST_BASE), fetch.clone());
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let result = runtime.block_on(client.details(id, None));
    let requests = fetch
        .requests()
        .iter()
        .map(|request| {
            json!({
                "url": request.url.strip_prefix(RUST_BASE).unwrap_or(&request.url),
                "authorization": llama_fetch::header(request, "authorization"),
            })
        })
        .collect::<Vec<_>>();
    let outcome = match result {
        Ok(details) => json!({"result": serde_json::to_value(details).unwrap()}),
        Err(error) => json!({"error": error.message}),
    };
    (json!(requests), outcome)
}

fn compare_details(oracle: &mut oracle::TsOracle) {
    let cases = vec![
        (
            "owner/model-GGUF",
            json!({"status": 200, "body": {
                "id": "owner/model-GGUF",
                "gated": "manual",
                "siblings": [
                    {"rfilename": "model-Q5_K_M.gguf", "size": 6000},
                    {"rfilename": "model-Q4_K_M-00001-of-00002.gguf", "size": 2000},
                    {"rfilename": "model-Q4_K_M-00002-of-00002.gguf", "size": 3000},
                    {"rfilename": "mmproj-F16.gguf", "size": 1000}
                ]
            }}),
        ),
        (
            "owner/model x?",
            json!({"status": 200, "body": {
                "siblings": [
                    {"rfilename": "dir/model-MXFP4-00001-of-00002.gguf", "size": 10},
                    {"rfilename": "dir/model-MXFP4-00002-of-00002.gguf"},
                    {"rfilename": "dir/model-UD-Q4_K_XL.gguf", "size": 20},
                    {"rfilename": "dir/model-BF16.gguf", "size": 30},
                    {"rfilename": "dir/notes.txt", "size": 1}
                ]
            }}),
        ),
        ("owner/model", json!({"status": 200, "body": {"siblings": []}})),
        ("owner/model", json!({"status": 200, "body": [1, 2]})),
        ("owner/model", json!({"status": 200, "body": null})),
        ("owner/model", json!({"status": 404, "body": {"error": "missing"}})),
    ];
    for (id, response) in cases {
        let oracle_response = oracle.request(&json!({
            "id": "details",
            "op": "details",
            "token": "hf-secret",
            "id": id,
            "response": response,
        }));
        assert_eq!(oracle_response["ok"], true, "oracle failed: {oracle_response}");
        let output = &oracle_response["output"];
        let oracle_requests = output["requests"]
            .as_array()
            .unwrap()
            .iter()
            .map(|request| {
                json!({
                    "url": request["url"],
                    "authorization": request["authorization"],
                })
            })
            .collect::<Vec<_>>();
        let (rust_requests, rust_outcome) = rust_details(&response, id);
        assert_eq!(
            serde_json::to_string(&oracle_requests).unwrap(),
            serde_json::to_string(&rust_requests).unwrap()
        );
        let oracle_outcome = if let Some(error) = output.get("error") {
            json!({"error": error})
        } else {
            json!({"result": output["result"]})
        };
        assert_eq!(
            serde_json::to_string(&oracle_outcome).unwrap(),
            serde_json::to_string(&rust_outcome).unwrap(),
            "details case {id}"
        );
    }
}
