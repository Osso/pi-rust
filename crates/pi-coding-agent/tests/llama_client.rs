//! Behavioral tests for `extensions/llama/client.ts` with an injected `Fetch`.

use std::sync::{Arc, Mutex};

use pi_coding_agent::bundled_extensions::llama::client::{
    LlamaClient, LlamaListOptions, format_bytes, llama_inference_url, normalize_llama_server_url,
};
use serde_json::json;
use tokio_util::sync::CancellationToken;

#[path = "support/llama_fetch.rs"]
mod llama_fetch;

use llama_fetch::{HandlerFetch, VecBody, header, json_response};

fn sse_response(frames: &[&str]) -> pi_ai::request::HttpResponse {
    pi_ai::request::HttpResponse {
        status: 200,
        status_text: String::new(),
        headers: vec![("content-type".to_owned(), "text/event-stream".to_owned())],
        body: Box::new(VecBody {
            chunks: frames.iter().map(|frame| frame.as_bytes().to_vec()).collect(),
        }),
    }
}

fn client(fetch: Arc<HandlerFetch>) -> LlamaClient {
    LlamaClient::with_fetch("http://127.0.0.1:8080", None, fetch).unwrap()
}

#[test]
fn normalizes_management_and_inference_urls() {
    assert_eq!(
        normalize_llama_server_url("http://127.0.0.1:8080/v1/").unwrap(),
        "http://127.0.0.1:8080"
    );
    assert_eq!(
        normalize_llama_server_url("https://example.com/prefix/v1").unwrap(),
        "https://example.com/prefix"
    );
    assert_eq!(
        normalize_llama_server_url("http://LOCALHOST:8080").unwrap(),
        "http://localhost:8080"
    );
    assert_eq!(
        normalize_llama_server_url("HTTP://Example.COM:80/").unwrap(),
        "http://example.com"
    );
    assert_eq!(normalize_llama_server_url("http://x/a//v1//").unwrap(), "http://x/a");
    let error = normalize_llama_server_url("file:///tmp/llama").unwrap_err();
    assert_eq!(error.message, "Server URL must use http or https");
    let error = normalize_llama_server_url("not a url").unwrap_err();
    assert_eq!(error.message, "Invalid URL");
    assert_eq!(
        llama_inference_url("http://127.0.0.1:8080/v1/").unwrap(),
        "http://127.0.0.1:8080/v1"
    );
}

#[test]
fn formats_byte_counts() {
    assert_eq!(format_bytes(0.0), "0 B");
    assert_eq!(format_bytes(512.0), "512 B");
    assert_eq!(format_bytes(1024.0), "1.00 KiB");
    assert_eq!(format_bytes(5120.0), "5.00 KiB");
    assert_eq!(format_bytes(1024.0 * 1024.0), "1.00 MiB");
    assert_eq!(format_bytes(10.0 * 1024.0), "10.0 KiB");
    assert_eq!(format_bytes(1024.0 * 1024.0 * 1024.0), "1.00 GiB");
}

#[tokio::test]
async fn lists_models_and_preserves_router_metadata() {
    let fetch = HandlerFetch::new(Arc::new(|request| {
        assert_eq!(request.url, "http://127.0.0.1:8080/models?reload=1");
        assert_eq!(request.method, "GET");
        json_response(
            200,
            &json!({"data": [
                {"id": "loaded", "status": {"value": "loaded", "args": ["llama-server", "-c", "4096"]},
                 "architecture": {"input_modalities": ["text", "image"]},
                 "meta": {"n_ctx": 65536, "n_ctx_train": 131072}},
                {"id": "unloaded", "status": {"value": "unloaded"}}
            ]}),
        )
    }));
    let models = client(fetch)
        .list(LlamaListOptions {
            reload: true,
            signal: None,
        })
        .await
        .unwrap();
    assert_eq!(models.len(), 2);
    assert_eq!(models[0].id, "loaded");
    assert_eq!(models[0].status.value, "loaded");
    assert_eq!(
        models[0].status.args.as_deref(),
        Some(["llama-server".to_owned(), "-c".to_owned(), "4096".to_owned()].as_slice())
    );
    assert_eq!(models[0].meta.as_ref().unwrap().n_ctx, Some(65536.0));
    assert_eq!(models[1].status.value, "unloaded");
}

#[tokio::test]
async fn rejects_invalid_catalogs_with_source_error_text() {
    let fetch = HandlerFetch::new(Arc::new(|_| json_response(200, &json!({"data": "nope"}))));
    let error = client(fetch).list(LlamaListOptions::default()).await.unwrap_err();
    assert_eq!(error.message, "llama.cpp returned an invalid model catalog");

    let fetch = HandlerFetch::new(Arc::new(|_| json_response(200, &json!({"data": [{"id": 1}]}))));
    let error = client(fetch).list(LlamaListOptions::default()).await.unwrap_err();
    assert_eq!(error.message, "Server is not running in llama.cpp router mode");

    let fetch = HandlerFetch::new(Arc::new(|_| json_response(500, &json!({"error": {"message": "boom"}}))));
    let error = client(fetch).list(LlamaListOptions::default()).await.unwrap_err();
    assert_eq!(error.message, "boom");

    let fetch = HandlerFetch::new(Arc::new(|_| json_response(503, &json!({}))));
    let error = client(fetch).list(LlamaListOptions::default()).await.unwrap_err();
    assert_eq!(error.message, "llama.cpp returned HTTP 503");
}

#[tokio::test]
async fn sends_management_requests_with_expected_headers_and_bodies() {
    let fetch = HandlerFetch::new(Arc::new(|request| match request.url.as_str() {
        "http://127.0.0.1:8080/models/load" | "http://127.0.0.1:8080/models/unload" => {
            json_response(200, &json!({"success": true}))
        }
        "http://127.0.0.1:8080/models" => json_response(200, &json!({"success": true})),
        _ => json_response(404, &json!({})),
    }));
    let client = LlamaClient::with_fetch("http://127.0.0.1:8080", Some("secret"), fetch.clone()).unwrap();
    client.load("model", None).await.unwrap();
    client.unload("model", None).await.unwrap();
    client.download("owner/repo:Q4_K_M", None).await.unwrap();

    let requests = fetch.requests();
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].url, "http://127.0.0.1:8080/models/load");
    assert_eq!(requests[0].body.as_deref(), Some(br#"{"model":"model"}"#.as_slice()));
    assert_eq!(
        header(&requests[0], "content-type").as_deref(),
        Some("application/json")
    );
    assert_eq!(header(&requests[0], "authorization").as_deref(), Some("Bearer secret"));
    assert_eq!(requests[1].url, "http://127.0.0.1:8080/models/unload");
    assert_eq!(requests[2].url, "http://127.0.0.1:8080/models");
    assert_eq!(
        requests[2].body.as_deref(),
        Some(br#"{"model":"owner/repo:Q4_K_M"}"#.as_slice())
    );
}

#[tokio::test]
async fn waits_for_unload_through_catalog_polling() {
    let polls = Arc::new(Mutex::new(0_u32));
    let fetch = HandlerFetch::new(Arc::new(move |request| {
        if request.url == "http://127.0.0.1:8080/models/unload" {
            return json_response(200, &json!({"success": true}));
        }
        let mut polls = polls.lock().unwrap();
        *polls += 1;
        let status = if *polls < 2 { "unloading" } else { "unloaded" };
        json_response(200, &json!({"data": [{"id": "m", "status": {"value": status}}]}))
    }));
    client(fetch).unload_and_wait("m", None).await.unwrap();
}

#[tokio::test]
async fn loads_with_sse_progress_and_waits_for_loaded_catalog_state() {
    let loaded = Arc::new(Mutex::new(false));
    let fetch = HandlerFetch::new(Arc::new(move |request| {
        if request.url == "http://127.0.0.1:8080/models/sse" {
            return sse_response(&[
                "data: {\"model\":\"test-model\",\"event\":\"status_change\",\"data\":{\"status\":\"loading\",\"progress\":{\"stages\":[\"text_model\",\"mmproj_model\"],\"current\":\"text_model\",\"value\":0.5}}}\n\n",
                "data: {\"model\":\"test-model\",\"event\":\"status_change\",\"data\":{\"status\":\"loaded\"}}\n\n",
            ]);
        }
        if request.url == "http://127.0.0.1:8080/models/load" {
            *loaded.lock().unwrap() = true;
            return json_response(200, &json!({"success": true}));
        }
        let status = if *loaded.lock().unwrap() { "loaded" } else { "loading" };
        json_response(
            200,
            &json!({"data": [{"id": "test-model", "status": {"value": status}}]}),
        )
    }));

    let progress = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&progress);
    let model = client(fetch)
        .load_and_wait(
            "test-model",
            Arc::new(move |entry| sink.lock().unwrap().push(entry.message)),
            None,
        )
        .await
        .unwrap();
    assert_eq!(model.id, "test-model");
    assert_eq!(model.status.value, "loaded");
    wait_for_progress(&progress, |messages| {
        messages.iter().any(|message| message == "Loading text model")
    })
    .await;
}

#[tokio::test]
async fn fails_load_when_the_router_reports_an_exit_code() {
    let fetch = HandlerFetch::new(Arc::new(|request| {
        if request.url.ends_with("/models/sse") {
            return pi_ai::request::HttpResponse {
                status: 500,
                status_text: String::new(),
                headers: Vec::new(),
                body: Box::new(VecBody { chunks: Vec::new() }),
            };
        }
        if request.url.ends_with("/models/load") {
            return json_response(200, &json!({"success": true}));
        }
        json_response(
            200,
            &json!({"data": [{"id": "m", "status": {"value": "unloaded", "failed": true, "exit_code": 3}}]}),
        )
    }));
    let error = client(fetch)
        .load_and_wait("m", Arc::new(|_| {}), None)
        .await
        .unwrap_err();
    assert_eq!(error.message, "Model exited with code 3");
}

#[tokio::test]
async fn downloads_with_byte_progress_and_returns_the_refreshed_catalog() {
    let polls = Arc::new(Mutex::new(0_u32));
    let fetch = HandlerFetch::new(Arc::new(move |request| {
        if request.url == "http://127.0.0.1:8080/models/sse" {
            return sse_response(&[
                "data: {\"model\":\"owner/repo:Q4_K_M\",\"event\":\"download_progress\",\"data\":{\"progress\":{\"https://example/model.gguf\":{\"done\":512,\"total\":1024}}}}\n\n",
                "data: {\"model\":\"owner/repo:Q4_K_M\",\"event\":\"download_finished\",\"data\":{}}\n\n",
            ]);
        }
        if request.url == "http://127.0.0.1:8080/models" && request.method == "POST" {
            return json_response(200, &json!({"success": true}));
        }
        if request.url == "http://127.0.0.1:8080/models?reload=1" {
            return json_response(
                200,
                &json!({"data": [{"id": "owner/repo:Q4_K_M", "status": {"value": "unloaded"}}]}),
            );
        }
        let mut polls = polls.lock().unwrap();
        *polls += 1;
        let status = if *polls == 1 { "downloading" } else { "unloaded" };
        json_response(
            200,
            &json!({"data": [{"id": "owner/repo:Q4_K_M", "status": {"value": status}}]}),
        )
    }));

    let progress = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&progress);
    let models = client(fetch)
        .download_and_wait(
            "owner/repo:Q4_K_M",
            Arc::new(move |entry| sink.lock().unwrap().push(entry)),
            None,
        )
        .await
        .unwrap();
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].status.value, "unloaded");
    wait_for_progress(&progress, |entries| {
        entries.iter().any(|entry| {
            entry.message == "Downloading model"
                && entry.ratio == Some(0.5)
                && entry.detail.as_deref() == Some("512 B / 1.00 KiB")
        })
    })
    .await;
}

async fn wait_for_progress<T>(progress: &Mutex<Vec<T>>, matches: impl Fn(&[T]) -> bool) {
    for _ in 0..100 {
        if matches(&progress.lock().unwrap()) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("progress callback was never invoked");
}

#[tokio::test]
async fn aborted_requests_surface_the_abort_message() {
    let fetch = HandlerFetch::new(Arc::new(|_| json_response(200, &json!({"data": []}))));
    let signal = CancellationToken::new();
    signal.cancel();
    let error = client(fetch)
        .load_and_wait("m", Arc::new(|_| {}), Some(signal))
        .await
        .unwrap_err();
    assert_eq!(error.message, "This operation was aborted");
}

#[tokio::test]
async fn cancelled_poll_loops_stop_with_cancelled_message() {
    let signal = CancellationToken::new();
    let fetch = HandlerFetch::new(Arc::new(|request| {
        if request.url.ends_with("/models/load") {
            return json_response(200, &json!({"success": true}));
        }
        if request.url.ends_with("/models/sse") {
            return sse_response(&[]);
        }
        json_response(200, &json!({"data": [{"id": "m", "status": {"value": "loading"}}]}))
    }));
    let cancel = signal.clone();
    let error = client(fetch)
        .load_and_wait("m", Arc::new(move |_| cancel.cancel()), Some(signal))
        .await
        .unwrap_err();
    assert_eq!(error.message, "Cancelled");
}
