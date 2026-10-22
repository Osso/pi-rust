# model-config traceability

Pinned source: `packages/coding-agent/src/core/model-config.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-coding-agent/src/core/model_config.rs`.

Differential proof: `oracle/model-config-oracle.ts` loads the same fixtures with the real
`ModelConfig` and returns provider ids, provider contents, and the exact error text;
`crates/pi-coding-agent/tests/differential_model_config.rs` compares 21 fixtures (valid full config,
comments, missing/invalid JSON, schema violations across providers/models/costs/compat/records/
overrides, unknown properties, empty and array roots, multiple errors). Focused tests cover missing
files and the three error prefixes.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `model-config.ts:1-230` | TypeBox schema definitions (models, overrides, providers, compat unions, records) | `models_config_schema` and helpers; every schema branch is exercised by the differential fixtures |
| `model-config.ts:232-246` | `formatValidationPath` (required paths, dotted instance paths, `root`) | shared `pi_ai::utils::validation::schema_error_lines` (TypeBox `Issue::path`); differential errors |
| `model-config.ts:248-253` | `deepFreeze(structuredClone(...))` immutability | Rust ownership: providers are cloned into the config and exposed by reference |
| `model-config.ts:256-292` | `load`: missing file → empty, read/parse/schema error messages, provider map | `ModelConfig::load`; focused error-prefix tests + differential |
| `model-config.ts:294-306` | `getProvider`, `getProviderIds`, `getError` | same-named methods; focused test |

## Representation decisions

- TypeBox 1.3.7 emits `patternProperties: {"^.*$": ...}` for `Type.Record`, `{type, const}` for
  literals, and `anyOf` for unions. The Rust schema mirrors that exact shape, which is why the
  shared validation engine produces identical messages.
- `ModelConfig` keeps providers as raw `serde_json::Value` entries in insertion order.

## Gaps

- Read failures report Rust's `io::Error` text instead of Node's `EISDIR:`-style message; only the
  `Failed to load models.json:` prefix is shared. Not reachable through the differential fixtures
  (permissions/directory cases would need an unreadable path).

## Discrepancies

- None observed: all 21 fixtures match provider maps and error text byte-for-byte.

## Proof inventory

- Differential: 21 fixtures plus the missing-file case.
- Focused cases: 1 test with five assertions.

## JS-value boundary regression (2026-10-20)

Shared JS stringify/object enumeration is used at this module's audited observable boundary; values and parser errors are unchanged. Concrete counterexamples and per-boundary tests: [JS-value proof](../js-value-semantics-proof.md).

## Q18 native auth configuration

Stored apiKey/baseUrl/header values and composed auth environment use `JsString`. `models.json` parsing stores one `JsObject`; provider IDs reject unmatched units explicitly under the scalar opaque-provider approval. Typed auth config decodes directly from that authority. `raw_error_consumer_components::model_config_keeps_raw_provider_credentials_and_url` written **unexecuted**; coding-agent compilation blocked. No request-boundary conversion occurs in composition.
