use indexmap::IndexMap;
use std::sync::{Arc, Mutex};

use pi_ai::images::generate_images;
use pi_ai::request::{
    BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse, ImagesOptions, ImagesRequestOptions,
};
use pi_ai::types::{ImagesContext, ImagesModel, ImagesStopReason, Modality, TextOrImageContent};
use pi_js_compat::abort::AbortSignal;
use serde_json::json;

fn model(api: &str) -> ImagesModel {
    ImagesModel {
        properties: Default::default(),
        id: "google/gemini-3.1-flash-image-preview".to_owned(),
        name: "Gemini".into(),
        api: api.to_owned(),
        provider: "openrouter".to_owned(),
        base_url: ("https://openrouter.ai/api/v1".to_owned()).into(),
        thinking_level_map: None,
        input: vec![Modality::Text],
        output: vec![Modality::Image],
        cost: Default::default(),
        auto_compaction_threshold: None,
        headers: None,
    }
}

fn context() -> ImagesContext {
    ImagesContext {
        input: vec![TextOrImageContent::Text(pi_ai::types::TextContent::new(
            "Generate a dog",
        ))],
    }
}

struct FixtureFetch {
    requests: Mutex<Vec<HttpRequest>>,
}

impl Fetch for FixtureFetch {
    fn fetch(
        &self,
        request: HttpRequest,
        _signal: Option<AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        self.requests.lock().expect("requests lock").push(request);
        let body = json!({
            "id": "img-1",
            "choices": [{"message": {"images": [{"image_url": "data:image/png;base64,aGk="}]}}]
        })
        .to_string();
        Box::pin(async move {
            Ok(HttpResponse {
                status: 200,
                status_text: String::new(),
                headers: vec![("content-type".to_owned(), "application/json".to_owned())],
                body: Box::new(BytesBody {
                    chunk: Some(body.into_bytes()),
                }),
            })
        })
    }
}

struct BytesBody {
    chunk: Option<Vec<u8>>,
}

impl HttpBody for BytesBody {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        let chunk = self.chunk.take();
        Box::pin(async move { chunk.map(Ok) })
    }
}

#[tokio::test]
async fn dispatches_through_the_registered_builtin_api() {
    let fetch = Arc::new(FixtureFetch {
        requests: Mutex::new(Vec::new()),
    });
    let options = ImagesOptions {
        request: ImagesRequestOptions {
            api_key: Some(("test".to_owned()).into()),
            fetch: Some(fetch.clone()),
            headers: Some(IndexMap::new()),
            ..Default::default()
        },
        ..Default::default()
    };
    let output = generate_images(model("openrouter-images"), context(), options)
        .await
        .expect("registered API dispatches");
    assert_eq!(output.stop_reason, ImagesStopReason::Stop);
    assert_eq!(output.response_id.as_deref(), Some("img-1"));
    assert_eq!(fetch.requests.lock().expect("requests lock").len(), 1);
}

#[tokio::test]
async fn unknown_apis_are_reported() {
    let error = generate_images(model("not-a-real-images-api"), context(), ImagesOptions::default())
        .await
        .expect_err("unknown API is rejected");
    assert_eq!(
        error.message,
        "No API provider registered for api: not-a-real-images-api"
    );
}
