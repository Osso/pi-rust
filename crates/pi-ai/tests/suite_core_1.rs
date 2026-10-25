// Process-local integration fixtures; process-sensitive fixtures keep standalone targets.

#[path = "abort.rs"]
mod abort;
#[path = "anthropic_messages.rs"]
mod anthropic_messages;
#[path = "anthropic_oauth.rs"]
mod anthropic_oauth;
#[path = "api_lazy.rs"]
mod api_lazy;
#[path = "approved_image_delta.rs"]
mod approved_image_delta;
#[path = "auth.rs"]
mod auth;
#[path = "auth_resolve_contract.rs"]
mod auth_resolve_contract;
#[path = "bedrock_converse_stream.rs"]
mod bedrock_converse_stream;
#[path = "bedrock_provider.rs"]
mod bedrock_provider;
#[path = "cloudflare.rs"]
mod cloudflare;
#[path = "cloudflare_gateway_binding.rs"]
mod cloudflare_gateway_binding;
#[path = "constrained_sampling.rs"]
mod constrained_sampling;
#[path = "device_code.rs"]
mod device_code;
#[path = "diagnostics.rs"]
mod diagnostics;
#[path = "differential_abort_combine.rs"]
mod differential_abort_combine;
#[path = "differential_ai_runtime.rs"]
mod differential_ai_runtime;
#[path = "differential_anthropic_messages.rs"]
mod differential_anthropic_messages;
#[path = "differential_anthropic_timeout.rs"]
mod differential_anthropic_timeout;
#[path = "differential_api_leaf.rs"]
mod differential_api_leaf;
#[path = "differential_azure_openai_responses.rs"]
mod differential_azure_openai_responses;
#[path = "differential_bedrock_converse_stream.rs"]
mod differential_bedrock_converse_stream;
#[path = "differential_chat_catalog.rs"]
mod differential_chat_catalog;
#[path = "differential_cloudflare_gateway_binding.rs"]
mod differential_cloudflare_gateway_binding;
#[path = "differential_constrained_sampling.rs"]
mod differential_constrained_sampling;
#[path = "differential_env_api_keys.rs"]
mod differential_env_api_keys;
#[path = "differential_error_body.rs"]
mod differential_error_body;
#[path = "differential_estimate.rs"]
mod differential_estimate;
#[path = "differential_event_stream.rs"]
mod differential_event_stream;
#[path = "differential_fetch_abort.rs"]
mod differential_fetch_abort;
#[path = "differential_github_copilot_headers.rs"]
mod differential_github_copilot_headers;
#[path = "differential_github_copilot_oauth.rs"]
mod differential_github_copilot_oauth;
#[path = "differential_google_api.rs"]
mod differential_google_api;
#[path = "differential_hash_sanitize.rs"]
mod differential_hash_sanitize;
#[path = "differential_headers.rs"]
mod differential_headers;
#[path = "differential_http_failures.rs"]
mod differential_http_failures;
#[path = "differential_json_parse.rs"]
mod differential_json_parse;
#[path = "differential_legacy_api_aliases.rs"]
mod differential_legacy_api_aliases;
#[path = "differential_mistral_conversations.rs"]
mod differential_mistral_conversations;
#[path = "differential_node_http_proxy.rs"]
mod differential_node_http_proxy;
#[path = "differential_oauth.rs"]
mod differential_oauth;
#[path = "differential_oauth_flows.rs"]
mod differential_oauth_flows;
#[path = "differential_openai_codex_responses.rs"]
mod differential_openai_codex_responses;
#[path = "differential_openai_completions.rs"]
mod differential_openai_completions;
#[path = "differential_openai_responses.rs"]
mod differential_openai_responses;
#[path = "differential_openai_responses_shared.rs"]
mod differential_openai_responses_shared;
#[path = "differential_openrouter_images.rs"]
mod differential_openrouter_images;
#[path = "differential_overflow.rs"]
mod differential_overflow;
#[path = "differential_provider_env.rs"]
mod differential_provider_env;
#[path = "differential_provider_inventory.rs"]
mod differential_provider_inventory;
#[path = "differential_provider_reasons.rs"]
mod differential_provider_reasons;
#[path = "differential_providers.rs"]
mod differential_providers;
#[path = "differential_retry.rs"]
mod differential_retry;
#[path = "differential_stainless_abort.rs"]
mod differential_stainless_abort;
#[path = "differential_text.rs"]
mod differential_text;
#[path = "differential_transform_messages.rs"]
mod differential_transform_messages;
#[path = "differential_validation.rs"]
mod differential_validation;
#[path = "env_api_keys.rs"]
mod env_api_keys;
#[path = "error_body.rs"]
mod error_body;
#[path = "estimate.rs"]
mod estimate;
#[path = "event_stream.rs"]
mod event_stream;
