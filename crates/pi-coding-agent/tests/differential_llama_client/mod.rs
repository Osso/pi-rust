#[path = "../support/llama_fetch.rs"]
mod llama_fetch;
#[path = "../support/oracle.rs"]
mod oracle;

use std::sync::{Arc, Mutex};

use pi_coding_agent::bundled_extensions::llama::client::{
    LlamaClient, LlamaListOptions, format_bytes, llama_inference_url, normalize_llama_server_url,
};
use serde_json::{Value, json};

use llama_fetch::{HandlerFetch, header, json_response};

const SOURCE_MODULE: &str = "packages/coding-agent/src/extensions/llama/client.ts";
const RUST_BASE: &str = "http://127.0.0.1:8080";

pub(crate) fn run() {
    let mut oracle = oracle::TsOracle::spawn("llama-client-oracle.ts", SOURCE_MODULE);
    compare_leaves(&mut oracle);
    compare_requests(&mut oracle);
    compare_catalog_errors(&mut oracle);
}

fn leaf_value(name: &str, kind: &str, input: &Value) -> Value {
    match kind {
        "format_bytes" => json!({ "name": name, "value": format_bytes(input.as_f64().unwrap()) }),
        "normalize" => match normalize_llama_server_url(input.as_str().unwrap()) {
            Ok(value) => json!({ "name": name, "value": value }),
            Err(error) => json!({ "name": name, "error": error.message }),
        },
        "inference" => match llama_inference_url(input.as_str().unwrap()) {
            Ok(value) => json!({ "name": name, "value": value }),
            Err(error) => json!({ "name": name, "error": error.message }),
        },
        other => panic!("unknown leaf kind {other}"),
    }
}

fn compare_leaves(oracle: &mut oracle::TsOracle) {
    let cases = vec![
        json!({"name": "zero", "kind": "format_bytes", "input": 0}),
        json!({"name": "bytes", "kind": "format_bytes", "input": 512}),
        json!({"name": "just_below_kib", "kind": "format_bytes", "input": 1023}),
        json!({"name": "kib", "kind": "format_bytes", "input": 1024}),
        json!({"name": "kib_fraction", "kind": "format_bytes", "input": 1536}),
        json!({"name": "kib_five", "kind": "format_bytes", "input": 5120}),
        json!({"name": "kib_ten", "kind": "format_bytes", "input": 10240}),
        json!({"name": "mib", "kind": "format_bytes", "input": 1048576}),
        json!({"name": "mib_ten", "kind": "format_bytes", "input": 10485760}),
        json!({"name": "tib", "kind": "format_bytes", "input": 1099511627776.0}),
        json!({"name": "root_v1", "kind": "normalize", "input": "http://127.0.0.1:8080/v1/"}),
        json!({"name": "prefix_v1", "kind": "normalize", "input": "https://example.com/prefix/v1"}),
        json!({"name": "host_case", "kind": "normalize", "input": "http://LOCALHOST:8080"}),
        json!({"name": "default_port", "kind": "normalize", "input": "HTTP://Example.COM:80/"}),
        json!({"name": "no_path", "kind": "normalize", "input": "https://example.com"}),
        json!({"name": "double_slash", "kind": "normalize", "input": "http://x/a//v1//"}),
        json!({"name": "escaped", "kind": "normalize", "input": "http://x/%7Euser/v1"}),
        json!({"name": "space", "kind": "normalize", "input": "http://x/a b"}),
        json!({"name": "query_fragment", "kind": "normalize", "input": "http://x:8080/v1?x=1#h"}),
        json!({"name": "ipv6", "kind": "normalize", "input": "http://[::1]:8080/v1/"}),
        json!({"name": "trim", "kind": "normalize", "input": "  http://example.com/v1  "}),
        json!({"name": "file_scheme", "kind": "normalize", "input": "file:///tmp/llama"}),
        json!({"name": "invalid", "kind": "normalize", "input": "not a url"}),
        json!({"name": "empty", "kind": "normalize", "input": ""}),
        json!({"name": "inference_root", "kind": "inference", "input": "http://127.0.0.1:8080/v1/"}),
        json!({"name": "inference_prefix", "kind": "inference", "input": "https://example.com/prefix/v1"}),
    ];
    let response = oracle.request(&json!({"id": 1, "op": "leaves", "cases": cases}));
    assert_eq!(response["ok"], true, "oracle failed: {response}");
    let expected = response["output"]["results"].as_array().unwrap().clone();
    assert_eq!(expected.len(), cases.len());
    for (case, expected) in cases.iter().zip(expected) {
        let actual = leaf_value(
            case["name"].as_str().unwrap(),
            case["kind"].as_str().unwrap(),
            &case["input"],
        );
        assert_eq!(
            serde_json::to_string(&actual).unwrap(),
            serde_json::to_string(&expected).unwrap(),
            "leaf case {}",
            case["name"]
        );
    }
}

fn normalize_request(request: &Value, base: &str) -> Value {
    let url = request["url"].as_str().unwrap();
    let path = url.strip_prefix(base).unwrap_or(url);
    json!({
        "method": request["method"],
        "url": path,
        "contentType": request["contentType"],
        "authorization": request["authorization"],
        "body": request["body"],
    })
}

async fn rust_requests(api_key: Option<&str>, actions: &[&str]) -> Value {
    let fetch = HandlerFetch::new(Arc::new(|request| {
        if request.method == "GET" && (request.url.ends_with("/models?reload=1") || request.url.ends_with("/models")) {
            return json_response(200, &json!({"data": [{"id": "loaded", "status": {"value": "loaded"}}]}));
        }
        if request.url.ends_with("/models/load") || request.url.ends_with("/models/unload") {
            return json_response(200, &json!({"success": true}));
        }
        if request.url.ends_with("/models") && request.method == "POST" {
            return json_response(200, &json!({"success": true}));
        }
        json_response(404, &json!({}))
    }));
    let client = LlamaClient::with_fetch(RUST_BASE, api_key, fetch.clone()).unwrap();
    let mut results = Vec::new();
    for action in actions {
        let list = match *action {
            "list" => Some(client.list(LlamaListOptions::default()).await),
            "list_reload" => Some(
                client
                    .list(LlamaListOptions {
                        reload: true,
                        signal: None,
                    })
                    .await,
            ),
            _ => None,
        };
        if let Some(list) = list {
            results.push(match list {
                Ok(models) => json!({"action": action, "value": serde_json::to_value(models).unwrap()}),
                Err(error) => json!({"action": action, "error": error.message}),
            });
            continue;
        }
        let result = match *action {
            "load" => client.load("model", None).await,
            "unload" => client.unload("model", None).await,
            "download" => client.download("owner/repo:Q4_K_M", None).await,
            other => panic!("unknown action {other}"),
        };
        results.push(match result {
            Ok(()) => json!({"action": action}),
            Err(error) => json!({"action": action, "error": error.message}),
        });
    }
    let requests = fetch
        .requests()
        .iter()
        .map(|request| {
            json!({
                "method": request.method,
                "url": request.url.strip_prefix(RUST_BASE).unwrap_or(&request.url),
                "contentType": header(request, "content-type"),
                "authorization": header(request, "authorization"),
                "body": request.body.as_ref().map(|body| String::from_utf8(body.clone()).unwrap()),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "requests": requests,
        "results": results,
        "serverUrl": client.server_url(),
    })
}

fn compare_requests(oracle: &mut oracle::TsOracle) {
    let actions = ["list", "list_reload", "load", "unload", "download"];
    let response = oracle.request(&json!({
        "id": 2,
        "op": "requests",
        "apiKey": "secret",
        "actions": actions,
    }));
    assert_eq!(response["ok"], true, "oracle failed: {response}");
    let output = &response["output"];
    let base = output["serverUrl"].as_str().unwrap();
    let oracle_requests = output["requests"]
        .as_array()
        .unwrap()
        .iter()
        .map(|request| normalize_request(request, base))
        .collect::<Vec<_>>();
    let oracle_results = output["results"].clone();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let rust = runtime.block_on(rust_requests(Some("secret"), &actions));
    assert_eq!(
        serde_json::to_string(&oracle_requests).unwrap(),
        serde_json::to_string(&rust["requests"]).unwrap()
    );
    assert_eq!(
        serde_json::to_string(&oracle_results).unwrap(),
        serde_json::to_string(&rust["results"]).unwrap()
    );
}

fn compare_catalog_errors(oracle: &mut oracle::TsOracle) {
    let responses = json!([
        {"status": 200, "body": {"data": "nope"}},
        {"status": 200, "body": {"data": [{"id": 1}]}},
        {"status": 500, "body": {"error": {"message": "boom"}}},
        {"status": 503, "body": {}},
        {"status": 200, "body": {"data": []}}
    ]);
    let response = oracle.request(&json!({
        "id": 3,
        "op": "catalog_errors",
        "responses": responses,
    }));
    assert_eq!(response["ok"], true, "oracle failed: {response}");
    let oracle_results = response["output"]["results"].clone();

    let index = Arc::new(Mutex::new(0_usize));
    let scripted = responses.as_array().unwrap().clone();
    let attempts = scripted.len();
    let fetch = HandlerFetch::new(Arc::new(move |_| {
        let mut index = index.lock().unwrap();
        let entry = &scripted[(*index).min(scripted.len() - 1)];
        *index += 1;
        json_response(entry["status"].as_u64().unwrap() as u16, &entry["body"])
    }));
    let client = LlamaClient::with_fetch(RUST_BASE, None, fetch).unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut rust_results = Vec::new();
    for _ in 0..attempts {
        let result = runtime.block_on(client.list(LlamaListOptions::default()));
        rust_results.push(match result {
            Ok(models) => json!({"ok": true, "value": serde_json::to_value(models).unwrap()}),
            Err(error) => json!({"ok": false, "error": error.message}),
        });
    }
    assert_eq!(
        serde_json::to_string(&oracle_results).unwrap(),
        serde_json::to_string(&rust_results).unwrap()
    );
}
