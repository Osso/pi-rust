//! Behavioral tests for `extensions/llama/huggingface.ts`.

use std::sync::Arc;

use pi_ai::types::ProviderEnv;
use pi_coding_agent::bundled_extensions::llama::huggingface::{
    HuggingFaceClient, HuggingFaceGated, find_hugging_face_token, parse_rate_limit_delay,
};
use serde_json::json;

#[path = "support/llama_fetch.rs"]
mod llama_fetch;

use llama_fetch::{HandlerFetch, header, json_response};

fn client(fetch: Arc<HandlerFetch>) -> HuggingFaceClient {
    HuggingFaceClient::with_fetch(Some("hf-secret"), Some("http://127.0.0.1:8080"), fetch)
}

#[tokio::test]
async fn searches_gguf_models_in_download_order() {
    let fetch = HandlerFetch::new(Arc::new(|request| {
        assert_eq!(
            request.url,
            "http://127.0.0.1:8080/api/models?search=qwen+coder&filter=gguf&sort=downloads&direction=-1&limit=20"
        );
        assert_eq!(header(request, "authorization").as_deref(), Some("Bearer hf-secret"));
        json_response(
            200,
            &json!([
                {"id": "owner/model-GGUF", "downloads": 1200},
                {"id": "owner/no-downloads"},
                {"downloads": 5},
                {"id": "owner/second", "downloads": 7}
            ]),
        )
    }));
    let recorded = Arc::clone(&fetch);
    let models = client(fetch).search("qwen coder", None).await.unwrap();
    assert_eq!(recorded.requests().len(), 1);
    assert_eq!(models.len(), 3);
    assert_eq!(models[0].id, "owner/model-GGUF");
    assert_eq!(models[0].downloads, 1200.0);
    assert_eq!(models[1].id, "owner/no-downloads");
    assert_eq!(models[1].downloads, 0.0);
    assert_eq!(models[2].id, "owner/second");
}

#[tokio::test]
async fn rejects_invalid_search_results() {
    let fetch = HandlerFetch::new(Arc::new(|_| json_response(200, &json!({"not": "an array"}))));
    let error = client(fetch).search("qwen", None).await.unwrap_err();
    assert_eq!(error.message, "Hugging Face returned invalid search results");
}

#[tokio::test]
async fn reads_quantizations_and_access_requirements() {
    let fetch = HandlerFetch::new(Arc::new(|request| {
        assert_eq!(
            request.url,
            "http://127.0.0.1:8080/api/models/owner/model-GGUF?blobs=true"
        );
        json_response(
            200,
            &json!({
                "id": "owner/model-GGUF",
                "gated": "manual",
                "siblings": [
                    {"rfilename": "model-Q5_K_M.gguf", "size": 6000},
                    {"rfilename": "model-Q4_K_M-00001-of-00002.gguf", "size": 2000},
                    {"rfilename": "model-Q4_K_M-00002-of-00002.gguf", "size": 3000},
                    {"rfilename": "mmproj-F16.gguf", "size": 1000}
                ]
            }),
        )
    }));
    let details = client(fetch).details("owner/model-GGUF", None).await.unwrap();
    assert_eq!(details.id, "owner/model-GGUF");
    assert_eq!(details.gated, HuggingFaceGated::Manual);
    assert_eq!(details.quantizations.len(), 2);
    assert_eq!(details.quantizations[0].name, "Q4_K_M");
    assert_eq!(details.quantizations[0].size, Some(5000.0));
    assert_eq!(details.quantizations[1].name, "Q5_K_M");
    assert_eq!(details.quantizations[1].size, Some(6000.0));
}

#[tokio::test]
async fn marks_incomplete_shards_and_encodes_ids() {
    let fetch = HandlerFetch::new(Arc::new(|request| {
        assert_eq!(
            request.url,
            "http://127.0.0.1:8080/api/models/owner/model%20x%3F?blobs=true"
        );
        json_response(
            200,
            &json!({
                "siblings": [
                    {"rfilename": "dir/model-MXFP4-00001-of-00002.gguf", "size": 10},
                    {"rfilename": "dir/model-MXFP4-00002-of-00002.gguf"},
                    {"rfilename": "dir/model-UD-Q4_K_XL.gguf", "size": 20},
                    {"rfilename": "dir/model-BF16.gguf", "size": 30},
                    {"rfilename": "dir/notes.txt", "size": 1}
                ]
            }),
        )
    }));
    let details = client(fetch).details("owner/model x?", None).await.unwrap();
    assert_eq!(details.id, "owner/model x?");
    assert_eq!(details.gated, HuggingFaceGated::NotGated);
    assert_eq!(details.quantizations.len(), 3);
    assert_eq!(details.quantizations[0].name, "UD-Q4_K_XL");
    assert_eq!(details.quantizations[0].size, Some(20.0));
    assert_eq!(details.quantizations[1].name, "BF16");
    assert_eq!(details.quantizations[1].size, Some(30.0));
    assert_eq!(details.quantizations[2].name, "MXFP4");
    assert_eq!(details.quantizations[2].size, None);
}

#[tokio::test]
async fn reports_rate_limits_with_retry_delays() {
    let fetch = HandlerFetch::new(Arc::new(|_| {
        let mut response = json_response(429, &json!({}));
        response.headers = vec![
            ("retry-after".to_owned(), "7".to_owned()),
            ("ratelimit".to_owned(), "limit=10;t=42".to_owned()),
        ];
        response
    }));
    let error = client(fetch).search("qwen", None).await.unwrap_err();
    assert_eq!(error.message, "Hugging Face rate limit reached; retry in 7s");

    let fetch = HandlerFetch::new(Arc::new(|_| {
        let mut response = json_response(429, &json!({}));
        response.headers = vec![("ratelimit".to_owned(), "limit=10;t=42".to_owned())];
        response
    }));
    let error = client(fetch).search("qwen", None).await.unwrap_err();
    assert_eq!(error.message, "Hugging Face rate limit reached; retry in 42s");

    let fetch = HandlerFetch::new(Arc::new(|_| json_response(429, &json!({}))));
    let error = client(fetch).search("qwen", None).await.unwrap_err();
    assert_eq!(error.message, "Hugging Face rate limit reached");
}

#[tokio::test]
async fn surfaces_payload_errors_and_status_fallbacks() {
    let fetch = HandlerFetch::new(Arc::new(|_| json_response(401, &json!({"error": "bad token"}))));
    let error = client(fetch).search("qwen", None).await.unwrap_err();
    assert_eq!(error.message, "bad token");

    let fetch = HandlerFetch::new(Arc::new(|_| json_response(500, &json!({}))));
    let error = client(fetch).search("qwen", None).await.unwrap_err();
    assert_eq!(error.message, "Hugging Face returned HTTP 500");

    let fetch = HandlerFetch::new(Arc::new(|_| json_response(200, &json!({"siblings": []}))));
    let details = client(fetch).details("owner/model", None).await.unwrap();
    assert_eq!(details.id, "owner/model");
    assert!(details.quantizations.is_empty());

    let fetch = HandlerFetch::new(Arc::new(|_| json_response(200, &json!([1, 2]))));
    let details = client(fetch).details("owner/model", None).await.unwrap();
    assert_eq!(details.id, "owner/model");
    assert!(details.quantizations.is_empty());

    let fetch = HandlerFetch::new(Arc::new(|_| json_response(200, &serde_json::Value::Null)));
    let error = client(fetch).details("owner/model", None).await.unwrap_err();
    assert_eq!(error.message, "Hugging Face returned invalid model details");
}

#[tokio::test]
async fn finds_tokens_from_environment_and_files() {
    let directory = tempfile::tempdir().unwrap();
    let home = directory.path().to_string_lossy().into_owned();
    let mut env = ProviderEnv::new();
    env.insert("HF_TOKEN".to_owned(), "  hf-secret  ".to_owned());
    assert_eq!(find_hugging_face_token(&env, &home).await, Some("hf-secret".to_owned()));

    let token_path = directory.path().join("token");
    std::fs::write(&token_path, "  file-secret \n").unwrap();
    let mut env = ProviderEnv::new();
    env.insert("HF_TOKEN".to_owned(), "   ".to_owned());
    env.insert("HF_TOKEN_PATH".to_owned(), token_path.to_string_lossy().into_owned());
    assert_eq!(
        find_hugging_face_token(&env, &home).await,
        Some("file-secret".to_owned())
    );

    let mut env = ProviderEnv::new();
    env.insert("HF_TOKEN_PATH".to_owned(), "/nonexistent/token".to_owned());
    env.insert("HF_HOME".to_owned(), directory.path().to_string_lossy().into_owned());
    assert_eq!(
        find_hugging_face_token(&env, &home).await,
        Some("file-secret".to_owned())
    );

    let mut env = ProviderEnv::new();
    env.insert("HF_TOKEN_PATH".to_owned(), "/nonexistent/token".to_owned());
    assert_eq!(find_hugging_face_token(&env, &home).await, None);

    let cache = directory.path().join(".cache/huggingface");
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::write(cache.join("token"), "home-secret").unwrap();
    let env = ProviderEnv::new();
    assert_eq!(
        find_hugging_face_token(&env, &home).await,
        Some("home-secret".to_owned())
    );
}

#[test]
fn parses_ratelimit_header_delays() {
    assert_eq!(parse_rate_limit_delay(Some("limit=10;t=42")), Some(42.0));
    assert_eq!(parse_rate_limit_delay(Some("t=5")), Some(5.0));
    assert_eq!(parse_rate_limit_delay(Some("limit=10; t=42")), None);
    assert_eq!(parse_rate_limit_delay(Some("limit=10")), None);
    assert_eq!(parse_rate_limit_delay(None), None);
}
