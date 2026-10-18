# providers/radius traceability

Pinned source: `packages/ai/src/providers/radius.ts` at
`532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-ai/src/providers/radius.rs`.
Tests: `crates/pi-ai/tests/provider_auth.rs` (`radius_*`).

| TypeScript | Rust | Proof |
|---|---|---|
| `radiusProvider(options = {})` defaults (`radius`, `Radius`, default gateway) | `radius_provider` | `radius_provider_defaults_and_overrides` |
| `normalizeRadiusGatewayUrl(options.gateway ?? DEFAULT)` | `normalize_radius_gateway_url` | same |
| `getModels: () => models` starts empty | `RadiusState::current_models` | same |
| `auth.apiKey` `Radius API key` / `auth.oauth` `lazyOAuth({ name, load: loadRadiusOAuth })` | `ProviderAuth` | same, `oauth_subscription_flags_match_the_ts_factories` |
| `refreshModels`: stored models restore via `publish({ update })` | `RadiusState::refresh` | `radius_refresh_restores_stored_models_without_network` |
| legacy OAuth `gatewayConfig` import via `publish({ persist, update })` | same | `radius_refresh_imports_legacy_oauth_cached_catalog` |
| `!allowNetwork || signal.aborted` → return | same | `radius_refresh_imports_legacy_oauth_cached_catalog` (no request) |
| network: `loadRadiusGatewayConfig(gateway, apiKey, signal)` then publish | same | `radius_refresh_loads_the_gateway_config_with_the_oauth_access_token` |
| gateway error propagates | same | `radius_refresh_propagates_gateway_errors` |
| `stream`/`streamSimple` delegate to `piMessagesApi()` | `RadiusProvider::{stream, stream_simple}` | provider dispatch is skipped for Radius (no static models) |

## Branch notes

- `publish` returning `false` aborts the refresh at each publication point.
- `checkedAt` uses `Date.now()`; Rust uses `request::now_millis()`.
- API key selection: OAuth → `credential.access`, API key → `credential.key`,
  otherwise none.

## Gaps

- None.

## Discrepancies

- Rust exposes `radius_provider_with_fetch` so tests can inject the HTTP
  implementation that TypeScript reads from `globalThis.fetch`; the default
  factory uses the crate's process-wide client.
