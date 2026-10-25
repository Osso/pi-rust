use std::sync::{Arc, Mutex};

use pi_ai::images_api_registry::{ImagesApiProvider, get_images_api_provider, register_images_api_provider};
use pi_ai::request::{BoxFuture, CallbackError, ImagesOptions, ProviderImages};
use pi_ai::types::{AssistantImages, ImagesContext, ImagesModel, ImagesStopReason, TextOrImageContent};

fn model(api: &str, id: &str) -> ImagesModel {
    ImagesModel {
        properties: Default::default(),
        id: id.to_owned(),
        name: id.into(),
        api: api.to_owned(),
        provider: "test".to_owned(),
        base_url: ("https://example.test/v1".to_owned()).into(),
        thinking_level_map: None,
        input: vec![pi_ai::types::Modality::Text],
        output: vec![pi_ai::types::Modality::Image],
        cost: Default::default(),
        auto_compaction_threshold: None,
        headers: None,
    }
}

fn context() -> ImagesContext {
    ImagesContext {
        input: vec![TextOrImageContent::Text(pi_ai::types::TextContent::new("a red circle"))],
    }
}

fn ok_result(model: &ImagesModel) -> AssistantImages {
    AssistantImages {
        api: model.api.clone(),
        provider: model.provider.clone(),
        model: model.id.clone(),
        output: vec![TextOrImageContent::Image(pi_ai::types::ImageContent::new(
            "aGk=",
            "image/png",
        ))],
        response_id: Some("response-1".to_owned()),
        usage: None,
        stop_reason: ImagesStopReason::Stop,
        error_message: None,
        timestamp: 1.0,
    }
}

struct FakeImages {
    calls: Mutex<Vec<String>>,
}

impl ProviderImages for FakeImages {
    fn generate_images(
        &self,
        model: ImagesModel,
        _context: ImagesContext,
        _options: ImagesOptions,
    ) -> BoxFuture<'_, Result<AssistantImages, CallbackError>> {
        self.calls.lock().expect("calls lock").push(model.id.clone());
        Box::pin(async move { Ok(ok_result(&model)) })
    }
}

struct FailingImages;

impl ProviderImages for FailingImages {
    fn generate_images(
        &self,
        _model: ImagesModel,
        _context: ImagesContext,
        _options: ImagesOptions,
    ) -> BoxFuture<'_, Result<AssistantImages, CallbackError>> {
        Box::pin(async { Err(CallbackError::new("boom")) })
    }
}

#[tokio::test]
async fn registers_and_resolves_by_api() {
    let fake = Arc::new(FakeImages {
        calls: Mutex::new(Vec::new()),
    });
    register_images_api_provider(ImagesApiProvider {
        api: "test-registry-basic".to_owned(),
        generate_images: fake.clone(),
    });

    let provider = get_images_api_provider("test-registry-basic").expect("registered provider");
    let response = provider
        .generate_images(model("test-registry-basic", "m1"), context(), ImagesOptions::default())
        .await
        .expect("generation succeeds");
    assert_eq!(response.stop_reason, ImagesStopReason::Stop);
    assert_eq!(response.response_id.as_deref(), Some("response-1"));
    assert_eq!(fake.calls.lock().expect("calls lock").as_slice(), ["m1"]);
    assert!(get_images_api_provider("missing-api").is_none());
}

#[tokio::test]
async fn rejects_models_with_a_mismatched_api() {
    register_images_api_provider(ImagesApiProvider {
        api: "test-registry-mismatch".to_owned(),
        generate_images: Arc::new(FakeImages {
            calls: Mutex::new(Vec::new()),
        }),
    });
    let provider = get_images_api_provider("test-registry-mismatch").expect("registered provider");
    let error = provider
        .generate_images(model("other-api", "m1"), context(), ImagesOptions::default())
        .await
        .expect_err("mismatched api is rejected");
    assert_eq!(
        error.message,
        "Mismatched api: other-api expected test-registry-mismatch"
    );
}

#[tokio::test]
async fn later_registrations_replace_earlier_ones() {
    register_images_api_provider(ImagesApiProvider {
        api: "test-registry-replace".to_owned(),
        generate_images: Arc::new(FakeImages {
            calls: Mutex::new(Vec::new()),
        }),
    });
    register_images_api_provider(ImagesApiProvider {
        api: "test-registry-replace".to_owned(),
        generate_images: Arc::new(FailingImages),
    });
    let provider = get_images_api_provider("test-registry-replace").expect("registered provider");
    let error = provider
        .generate_images(
            model("test-registry-replace", "m1"),
            context(),
            ImagesOptions::default(),
        )
        .await
        .expect_err("replacement provider is used");
    assert_eq!(error.message, "boom");
}
