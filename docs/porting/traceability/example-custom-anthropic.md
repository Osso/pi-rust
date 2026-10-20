# Custom Anthropic example traceability

Source: `packages/coding-agent/examples/extensions/custom-provider-anthropic/index.ts`, original runtime pin in [source lock](../source-lock.json). Native extension policy approved; factory is opt-in, never bundled/autoloaded.

| Source symbols/branches | Native representation | Focused proof |
|---|---|---|
| default registration, two models/custom API/OAuth name | `custom_anthropic_extension`, `_with_runtime`, `CustomAnthropic` | `default_factory_registers_exact_catalog_without_network` actual native factory/runtime |
| `generatePKCE`, `loginAnthropic`: URL, prompt, code/state split, extra/absent/empty segments | existing PKCE/OAuthRuntime; `CustomOAuth::login` | `native_factory_oauth_matches_pinned_login_refresh_and_failures` exact source differential |
| `refreshAnthropicToken`, getApiKey, five-minute expiry | `CustomOAuth::refresh`, `to_auth` | same differential: fake token exchange, refresh, status/prompt/fetch errors |
| API-key env resolution | existing `env_api_key_auth` | registration binding present; resolution fixture pending |
| all conversions, reasoning, headers, custom SDK event loop | pending | not claimed |
| valid content scratch fields | existing typed output boundary drops fields | RED `pinned_example_valid_text_start_survives_native_event_boundary` preserves exact oracle snapshot and proves loss of `index:3`; parent repair pending |

Oracle evaluates original source bytes with transpilation only; controlled `require` rejects every import except the fake Anthropic SDK and `calculateCost`/event-stream fixtures. Cost function is AST-selected from original `models.ts` with its body unchanged. Every pushed event is cloned immediately; neither scratch fields nor errors are normalized. Random bytes and `Date.now()` are deterministic. No source clone, SDK install, live fetch, default registry or session hooks.

Source package metadata pins SDK 0.52.0; source runtime uses controlled SDK events rather than an installed SDK assumption. Native transport/request reuse and its observable HTTP proof remain pending.

## Development ledger

Full logs: `/tmp/pi-rust-example-anthropic-proof/`.

- Base `0396a35`: `cargo test -p pi-coding-agent --test example_custom_anthropic -- --nocapture`: RED valid `text_start` event loses `index:3` across current typed serde boundary.
- Inert factory: focused `native_factory_oauth` RED, expected 1 registration, actual 0.
- OAuth/catalog implementation: focused `factory_` GREEN 2/2, fake-only controlled source differential; no full-suite/check/clippy/readability/final claim.

Coverage is partial until all source branches are proven and parent verification completes.
