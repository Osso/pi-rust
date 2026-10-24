# OAuth loaders — common pin

Authority: `packages/ai/src/utils/oauth/load.ts:1–21` at4f9e6f8a4. Native `crates/pi-ai/src/auth/oauth/load.rs` statically links only the three frozen flow modules.

| Frozen export | Native / actual-source proof |
|---|---|
| loadAnthropicOAuth | load_anthropic_oauth; complete auth name/method set/toAuth token result compared to real imported flow. |
| loadOpenAICodexOAuth | load_openai_codex_oauth; complete auth name/method set/toAuth token result compared. |
| loadGitHubCopilotOAuth | load_github_copilot_oauth; complete auth name/method set/toAuth/baseUrl result compared. |

`oauth_load_contract::frozen_three_loaders_return_current_auth_records` compares the sorted actual frozen loader exports and their complete auth records; Rust types return owned values per existing approval. Static linking is the native representation of loading these built-in modules, not an alternate fallback/override path.

## Source-absent deletion audit

Before deleting, searched all crates and docs. No coding-agent settings or /login list referenced the four stale flow constructors; /login consumes the actual registry (Anthropic/Copilot/Codex). One outside executable hook remained: coding-agent/src/bun/cli.rs called register_bun_oauth_flows. Parent approved deleting that call; frozen bun/cli.ts:9–12 has only restoreSandboxEnv and Bedrock/cli imports. Stale docs now link this deletion ledger.

Deleted native OpenRouter/Radius/Kimi/xAI OAuth modules and exports, bun_oauth.rs/export, OAuthFlowLoaders/registered override registry, all four extra load functions, and OpenRouter-image OAuth advertisement/error stub. No compatibility fallback remains. Radius provider/config production code is queued for the separately approved next slice, not claimed current-pin behavior.

## Retired old-only tests

| Deleted test file | Exact test names |
|---|---|
| openrouter_oauth.rs | callback_login_exchanges_the_code_for_a_permanent_key; callback_exchange_failures_reach_the_page_and_the_login; manual_codes_are_trimmed_and_exchanged_without_a_callback; empty_manual_input_fails_without_a_token_request; refresh_is_identity_and_to_auth_uses_the_key. |
| radius_oauth.rs | device_login_uses_gateway_endpoints_and_keeps_the_scope; refresh_uses_the_gateway_without_discovery; browser_discovery_requires_the_authorization_endpoint; browser_login_runs_the_callback_and_token_exchange; device_denials_and_unknown_methods_have_source_messages; to_auth_uses_the_access_token. |
| kimi_coding_oauth.rs | device_login_waits_before_the_first_poll_and_uses_the_complete_uri; device_errors_have_source_messages; honors_the_oauth_host_override_and_strips_trailing_slashes; refresh_retries_429_then_succeeds_and_rejects_invalid_grant; to_auth_returns_a_bearer_header_and_labels_match. |
| xai_oauth.rs | device_flow_delays_polling_and_handles_pending_and_slow_down; prefers_verification_uri_complete_and_defaults_interval_zero; rejects_untrusted_verification_uris; denied_and_expired_device_codes_have_source_messages; refresh_preserves_an_unrotated_refresh_token_and_defaults_the_lifetime; to_auth_and_labels_match_the_source. |
| oauth_load.rs | loader_registry_defaults_overrides_and_bun_registration; replacement actual-source three-loader test preserves valid built-in results without old override registration assertions. |

`differential_oauth_flows` retains its Anthropic manual/callback, Codex device and both refresh scenarios with complete credential/prompt/event/request output comparisons. Removed only OpenRouter manual/callback, xAI/Kimi/Radius device and Radius browser scenarios, extra xAI/Kimi/Radius refresh cases, plus compare_radius_config (normalize/sanitize/models) and obsolete variants/imports. Frozen lacks radius-config.ts; parent directed deletion, not conversion to a retained fake oracle. Real imports now use utils/oauth, not the retired auth/oauth path.

## Proof

OpenRouter-image actual-factory RED showed extra native OAuth metadata; after removal actual-source loader/image2 + retained flow differential1 + image-provider4 GREEN (`q15c-oauth-retained-flows-1.log`). Gate revision/logs are recorded in the checkpoint ledger. No live providers/network used.
