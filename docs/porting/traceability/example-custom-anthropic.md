# Custom Anthropic example traceability

Common-pin whole-module audit (2026-10-25): literal ProviderConfigInput owns models, streamSimple and OAuth callbacks, matching frozen 4f9e6f8a4. Audited PKCE/login/refresh, code/state splitting, token expiry; OAuth tool-name mapping and the source's pair-destroying sanitizer; text/image/tool-result conversions; system/cache/thinking options; all stream content/usage/stop/error/abort branches and registration. Current `example_custom_anthropic` passes 13 tests, including installed-SDK loopback wire, source snapshots and acquisition/lifecycle cases (`/tmp/triage2-example_custom_anthropic-audit.log`). No live provider runs. The old native composer registration and its tests have no frozen counterpart and remain deleted. Historical development reports below do not expand current proof.

Source: `packages/coding-agent/examples/extensions/custom-provider-anthropic/index.ts`, original runtime pin in [source lock](../source-lock.json). Native extension policy approved; factory is opt-in, never bundled/autoloaded.

| Source symbols/branches | Native representation | Focused proof |
|---|---|---|
| default registration, two models/custom API/OAuth name | `custom_anthropic_extension`, `_with_runtime`, `CustomAnthropic` | `default_factory_registers_exact_catalog_without_network` actual native factory/runtime |
| `generatePKCE`, `loginAnthropic`: URL, prompt, code/state split, extra/absent/empty segments | existing PKCE/OAuthRuntime; `CustomOAuth::login` | `native_factory_oauth_matches_pinned_login_refresh_and_failures` exact source differential |
| `refreshAnthropicToken`, getApiKey, five-minute expiry | `CustomOAuth::refresh`, `to_auth` | same differential: fake token exchange, refresh, status/prompt/fetch errors |
| factory apiKey literal, API tag, base URL, catalog, OAuth and stream callback | literal ProviderConfigInput registration; auth resolution belongs to ModelRegistry, not this example | `default_factory_registers_exact_catalog_without_network`; old composer-only fixture removed |
| `isOAuthToken`, tool names, `sanitizeSurrogates`, content/message/tool conversion, cache and thinking budgets | example-local request module | `pinned_request_conversion_matrix` GREEN: OAuth/key, images, grouped/empty/mixed results, signed/unsigned thinking, whitespace, supplementary scalar sanitization, all seven reasoning levels/custom budgets, model-disabled thinking, raw stream |
| `mapStopReason`, usage/cost replacement, abort, missing stop reason, thrown Error/JSON forwarding | example-local stream module | `pinned_terminal_usage_abort_and_error_matrix` GREEN; exact source snapshots and final result |
| content start/delta/stop, repeated/unknown indices, mismatched deltas, signature concatenation, partial JSON retention on parse failure | example-local stream module | `pinned_stream_content_usage_and_error_matrix`, `pinned_incomplete_tool_failure_preserves_partial_json`: GREEN after explicit core dependency merge (`green-lifecycle-and-existing.log`) |
| valid content scratch fields | shared typed scratch dependency, example literal producer | `pinned_example_valid_text_start_survives_native_event_boundary`; `literal_lifecycle_keeps_unclosed_scratch_and_first_duplicate_index` GREEN after dependency merge (success/failure, duplicate indices, unclosed blocks) |
| nonobject tool JSON | explicit producer error before typed event serialization | `nonobject_tool_arguments_fail_explicitly_without_losing_scratch`: RED producer panic/stranded stream on null; approved object-only rejection implemented for null/array/number/bool/string; GREEN 5 values |

Oracle evaluates original source bytes with transpilation only; controlled mode `require` rejects every import except the fake Anthropic SDK and `calculateCost`/event-stream fixtures; explicit wire mode substitutes the installed Anthropic SDK. Cost function is AST-selected from original `models.ts` with its body unchanged. Every pushed event is cloned immediately; neither scratch fields nor errors are normalized. Random bytes and `Date.now()` are deterministic. Auth-registration probes additionally evaluate unchanged original composer/resolver bodies with process env empty and child-process/shell/API-registry operations replaced by fail-fast traps. No source clone, SDK install, external fetch, default registry or session hooks. Real HTTP is restricted to loopback inside the committed test sandbox.

Source package metadata pins SDK 0.52.0; installed root SDK is 0.91.1 (example-local SDK absent); no dependency installation. Controlled-source tests prove producer logic independently. `wire::installed_sdk_wire_auth_body_and_events` additionally executes the original source with the actual installed SDK and localhost HTTP, then native reqwest against the same response. It compares method/path, parsed JSON body, every header except explicitly asserted native deltas, and every unnormalized event/result (text/thinking/signature/tool JSON/usage/cost). Both API-key and OAuth paths pass. `wire::installed_sdk_wire_status_error_and_terminated_body` proves HTTP 400 and mid-body disconnect event ordering/messages using shared native HTTP failure identity, not an example parsing shim.

Raw HTTP request bytes, parsed headers/body, SDK version and source/native event snapshots are written to `$TMPDIR/wire-*.json`. The mandatory Cargo guard supplies private TMPDIR even without an explicit `PI_TEST_SANDBOX_DIR` selector. Historical development captures remain under `/tmp/pi-rust-example-anthropic-proof/`; independent selected-state captures under `/tmp/pi-rust-batch-a-examples-verification/sandbox/`. Runtime `node` versus `rust` and runtime version are approved differences.

Default-runner regression: wire capture previously required the optional selector and panicked after actual requests/events. `/tmp/pi-rust-batch-a-default-wire-proof/ledger.json` records default Cargo RED2 and GREEN2 after using only private TMPDIR; no guard change or header/body/event assertion relaxation. Main owns final integrated gate.

## Approved native transport defaults

User decision, 2026-10-20. Exact question:

> Anthropic wire tests found four Node-only HTTP defaults: Connection: keep-alive, Accept-Encoding: gzip, deflate, Accept-Language: *, and Sec-Fetch-Mode: cors. Allow Rust's native defaults, while keeping authentication, compatibility headers, body and events exact?

Exact answer: **Allow native defaults (Recommended)**. Selected description: “Record exact differences; do not advertise unsupported compression or browser behavior.” Alternative: “Match Node defaults”, including support they require.

| Header | Source Node wire | Native Rust wire |
|---|---|---|
| `connection` | `keep-alive` | absent |
| `accept-encoding` | `gzip, deflate` | absent |
| `accept-language` | `*` | absent |
| `sec-fetch-mode` | `cors` | absent |

Wire tests assert these four exact deltas before comparing the remaining complete header maps; unknown headers are not discarded. No broader header-normalization waiver. No production header changes, fake compression support or browser behavior.

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

Development proof: `3ec5f65` targeted example suite GREEN 12/12, including previously scratch-blocked content matrix. Installed-SDK wire tests GREEN 2/2 at `7d9cdc9`, four fixture cases, exact raw captures retained. Four transport deltas explicitly asserted and approved above. Original exact-header comparison RED is retained as `red-installed-sdk-all-headers.log`; GREEN log `green-installed-sdk-wire-7d9cdc9.log`. Parent owns integration and independent verification; coverage remains partial until those gates close.
