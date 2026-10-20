//! Behavioral tests for the bundled `<first-party:openai-remote-compact>` extension.

use pi_ai::types::Model;
use pi_coding_agent::bundled_extensions::openai_remote_compact::{
    is_openai_responses_model, openai_remote_compact_extension_factory,
};
use pi_coding_agent::core::resource_loader::{
    DefaultResourceLoader, DefaultResourceLoaderOptions, InlineExtension, ResourceLoader,
};
use serde_json::json;

fn model(provider: &str, api: &str) -> Model {
    serde_json::from_value(json!({
        "id": "model",
        "name": "model",
        "api": api,
        "provider": provider,
        "baseUrl": "https://example.test",
        "reasoning": false,
        "input": ["text"],
        "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0},
        "contextWindow": 100000,
        "maxTokens": 4096
    }))
    .unwrap()
}

#[test]
fn recognizes_openai_responses_models() {
    assert!(is_openai_responses_model(Some(&model("openai", "openai-responses"))));
    assert!(is_openai_responses_model(Some(&model(
        "openai-codex",
        "openai-codex-responses"
    ))));
    assert!(is_openai_responses_model(Some(&model(
        "openai-codex-gc",
        "openai-codex-responses"
    ))));
    assert!(!is_openai_responses_model(Some(&model("openai", "openai-completions"))));
    assert!(!is_openai_responses_model(Some(&model(
        "anthropic",
        "openai-responses"
    ))));
    assert!(!is_openai_responses_model(None));
}

#[tokio::test]
async fn registers_a_compaction_handler() {
    let temp = tempfile::tempdir().unwrap();
    let loader = DefaultResourceLoader::new(DefaultResourceLoaderOptions {
        cwd: temp.path().display().to_string(),
        agent_dir: temp.path().display().to_string(),
        extension_factories: vec![InlineExtension::Factory {
            extension_path: Some("<first-party:openai-remote-compact>".to_owned()),
            factory: openai_remote_compact_extension_factory(),
        }],
        no_extensions: true,
        no_skills: true,
        no_prompt_templates: true,
        no_themes: true,
        no_context_files: true,
        ..Default::default()
    })
    .unwrap();
    loader.reload().await.unwrap();
    let extensions = loader.get_extensions();
    assert!(extensions.errors.is_empty());
    assert_eq!(extensions.extensions.len(), 1);
    assert_eq!(extensions.extensions[0].read().handlers.compaction.len(), 1);
}
