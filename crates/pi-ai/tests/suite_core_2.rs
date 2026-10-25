// Process-local integration fixtures; process-sensitive fixtures keep standalone targets.

#[path = "faux_provider.rs"]
mod faux_provider;
#[path = "generator_repin_artifacts.rs"]
mod generator_repin_artifacts;
#[path = "generator_repin_catalog.rs"]
mod generator_repin_catalog;
#[path = "generator_repin_emission.rs"]
mod generator_repin_emission;
#[path = "generator_repin_fetch.rs"]
mod generator_repin_fetch;
#[path = "generator_repin_ingress.rs"]
mod generator_repin_ingress;
#[path = "generator_repin_metadata.rs"]
mod generator_repin_metadata;
#[path = "github_copilot_headers.rs"]
mod github_copilot_headers;
#[path = "github_copilot_oauth.rs"]
mod github_copilot_oauth;
#[path = "google_api.rs"]
mod google_api;
#[path = "google_shared.rs"]
mod google_shared;
#[path = "hash.rs"]
mod hash;
#[path = "headers.rs"]
mod headers;
#[path = "http.rs"]
mod http;
#[path = "image_catalog_generator.rs"]
mod image_catalog_generator;
#[path = "image_catalog_http.rs"]
mod image_catalog_http;
#[path = "image_catalog_prototype.rs"]
mod image_catalog_prototype;
#[path = "image_content_properties.rs"]
mod image_content_properties;
#[path = "images.rs"]
mod images;
#[path = "images_api_registry.rs"]
mod images_api_registry;
#[path = "json_parse.rs"]
mod json_parse;
#[path = "known_provider_contract.rs"]
mod known_provider_contract;
#[path = "lossless_auth_interaction.rs"]
mod lossless_auth_interaction;
#[path = "lossless_content.rs"]
mod lossless_content;
#[path = "lossless_ingress.rs"]
mod lossless_ingress;
#[path = "lossless_metadata.rs"]
mod lossless_metadata;
#[path = "lossless_oauth_callbacks.rs"]
mod lossless_oauth_callbacks;
#[path = "lossless_provider_errors.rs"]
mod lossless_provider_errors;
#[path = "lossless_sdk_boundaries.rs"]
mod lossless_sdk_boundaries;
#[path = "mistral_conversations.rs"]
mod mistral_conversations;
#[path = "native_number_decode.rs"]
mod native_number_decode;
#[path = "node_http_proxy.rs"]
mod node_http_proxy;
#[path = "oauth_abort_wrapper.rs"]
mod oauth_abort_wrapper;
#[path = "oauth_entropy_failures.rs"]
mod oauth_entropy_failures;
#[path = "oauth_load_contract.rs"]
mod oauth_load_contract;
#[path = "oauth_page.rs"]
mod oauth_page;
#[path = "oauth_registry.rs"]
mod oauth_registry;
#[path = "openai_codex_oauth.rs"]
mod openai_codex_oauth;
#[path = "openai_codex_responses.rs"]
mod openai_codex_responses;
#[path = "openai_completions.rs"]
mod openai_completions;
#[path = "openai_prompt_cache.rs"]
mod openai_prompt_cache;
#[path = "openai_responses_shared.rs"]
mod openai_responses_shared;
#[path = "openrouter_images.rs"]
mod openrouter_images;
#[path = "openrouter_images_provider.rs"]
mod openrouter_images_provider;
#[path = "overflow.rs"]
mod overflow;
#[path = "pkce.rs"]
mod pkce;
#[path = "pkce_repin.rs"]
mod pkce_repin;
#[path = "provider_auth.rs"]
mod provider_auth;
#[path = "provider_env.rs"]
mod provider_env;
#[path = "provider_retry.rs"]
mod provider_retry;
#[path = "providers.rs"]
mod providers;
#[path = "providers_openai_codex.rs"]
mod providers_openai_codex;
#[path = "random_uuid.rs"]
mod random_uuid;
#[path = "retry.rs"]
mod retry;
#[path = "sanitize_unicode.rs"]
mod sanitize_unicode;
#[path = "text.rs"]
mod text;
#[path = "transform_messages.rs"]
mod transform_messages;
#[path = "types.rs"]
mod types;
#[path = "validation.rs"]
mod validation;
