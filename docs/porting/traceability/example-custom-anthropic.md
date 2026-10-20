# Custom Anthropic example traceability

Source: `packages/coding-agent/examples/extensions/custom-provider-anthropic/index.ts`, original runtime pin in [source lock](../source-lock.json). Native extension policy approved; factory is opt-in, never bundled/autoloaded.

| Source symbols/branches | Native representation | Focused proof |
|---|---|---|
| default registration, two models/custom API/OAuth name | `custom_anthropic_extension`, `_with_runtime`, `CustomAnthropic` | `default_factory_registers_exact_catalog_without_network` actual native factory/runtime |
| `generatePKCE`, `loginAnthropic`: URL, prompt, code/state split, extra/absent/empty segments | existing PKCE/OAuthRuntime; `CustomOAuth::login` | `native_factory_oauth_matches_pinned_login_refresh_and_failures` exact source differential |
| `refreshAnthropicToken`, getApiKey, five-minute expiry | `CustomOAuth::refresh`, `to_auth` | same differential: fake token exchange, refresh, status/prompt/fetch errors |
| API-key env resolution/check and provider display name through source registration adapter | existing pure `compose_model_provider` with empty in-memory ModelConfig and typed api-key configuration, native Provider supplies streams/OAuth | `native_registered_api_key_auth_matches_pinned_composed_registration` exact original composer/resolver differential: set/missing/empty env, stored credential/env, empty stored key, availability checks |
| `isOAuthToken`, tool names, `sanitizeSurrogates`, content/message/tool conversion, cache and thinking budgets | example-local request module | `pinned_request_conversion_matrix` GREEN: OAuth/key, images, grouped/empty/mixed results, signed/unsigned thinking, whitespace, supplementary scalar sanitization, all seven reasoning levels/custom budgets, model-disabled thinking, raw stream |
| `mapStopReason`, usage/cost replacement, abort, missing stop reason, thrown Error/JSON forwarding | example-local stream module | `pinned_terminal_usage_abort_and_error_matrix` GREEN; exact source snapshots and final result |
| content start/delta/stop, repeated/unknown indices, mismatched deltas, signature concatenation, partial JSON retention on parse failure | example-local stream module | `pinned_stream_content_usage_and_error_matrix`, `pinned_incomplete_tool_failure_preserves_partial_json`: core dependency `c9d6f68` merged; targeted rerun pending |
| valid content scratch fields | shared typed scratch dependency, example literal producer | `pinned_example_valid_text_start_survives_native_event_boundary`; `literal_lifecycle_keeps_unclosed_scratch_and_first_duplicate_index` GREEN after dependency merge (success/failure, duplicate indices, unclosed blocks) |
| nonobject tool JSON | explicit producer error before typed event serialization | `nonobject_tool_arguments_fail_explicitly_without_losing_scratch`: RED producer panic/stranded stream on null; approved object-only rejection implemented for null/array/number/bool/string; GREEN pending |

Oracle evaluates original source bytes with transpilation only; controlled `require` rejects every import except the fake Anthropic SDK and `calculateCost`/event-stream fixtures. Cost function is AST-selected from original `models.ts` with its body unchanged. Every pushed event is cloned immediately; neither scratch fields nor errors are normalized. Random bytes and `Date.now()` are deterministic. Auth-registration probes additionally evaluate unchanged original composer/resolver bodies with process env empty and child-process/shell/API-registry operations replaced by fail-fast traps. No source clone, SDK install, live fetch, default registry or session hooks.

Source package metadata pins SDK 0.52.0; source runtime uses controlled SDK events rather than an installed SDK assumption. Installed root SDK is 0.91.1 (example-local SDK absent); no dependency installation. Native transport reuses existing adapter. HTTP fixture compares exact source params/default headers and auth/helper headers. SDK-generated Rust runtime identity remains the existing adapter contract, not a claim that host-specific Node headers are byte-identical.

## Narrow public reuse API

Parent approved visibility-only reuse after controlled HTTP RED. `pi_ai::api::anthropic_sdk::{Auth,SdkRequest,build_request,execute}` are now public, and `anthropic_messages::AnthropicEventReader::{new,next_event}` public. No SDK behavior, constructor shape, retry logic, default provider or public SDK dependency changes. The example adds `X-Stainless-Helper-Method: stream`, required specifically by source `messages.stream()` (core backend uses a different call).

The source regex replaces both UTF-16 surrogate halves even in valid supplementary characters; local conversion deliberately preserves that observable source discrepancy rather than using the core backend's pair-preserving sanitizer.

## Development ledger

Full logs: `/tmp/pi-rust-example-anthropic-proof/`.

- Base `0396a35`: `cargo test -p pi-coding-agent --test example_custom_anthropic -- --nocapture`: RED valid `text_start` event loses `index:3` across current typed serde boundary.
- Inert factory: focused `native_factory_oauth` RED, expected 1 registration, actual 0.
- OAuth/catalog implementation: focused `factory_` GREEN 2/2, fake-only controlled source differential; no full-suite/check/clippy/readability/final claim.

- HTTP fixture RED against inert stream at `859be4d`: zero HTTP requests; new custom stream produces GREEN 1/1 after example-local request/stream implementation and narrow SDK visibility changes.

- Request matrix GREEN; terminal/usage/abort/Error/non-Error matrix GREEN; API-key environment resolution GREEN (3 focused tests). Runtime registration uses actual native handler, explicitly rejecting the JSON provider slot. Tests use only selected factory and injected transport, no default model registry/SDK.

- Stronger composed-auth/name fixture RED exposed native provider name (`custom-anthropic` vs OAuth display name), availability check and auth source differences. Reuse of existing typed pure composer repairs these without a JSON stream slot or new framework; GREEN 1/1 (10 source/native cases). No config files or default registries loaded.

- Native nonfinite f64 budget fixture RED exposed premature JSON serialization losing NaN/Infinity before `??` selection; selecting typed fields directly preserves source null serialization. `pinned_native_numeric_budgets_preserve_nonfinite_null_serialization` GREEN for NaN/±Infinity; no input-domain exclusion added.

Coverage is partial until all source branches are proven and parent verification completes.
