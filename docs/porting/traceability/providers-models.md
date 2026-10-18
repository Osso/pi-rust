# providers/*.models traceability

Pinned source: `packages/ai/src/providers/*.models.ts` (40 generated modules) and
`packages/ai/src/models.generated.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-ai/src/providers/models_data.rs`.
Tests: `crates/pi-ai/tests/providers.rs` (`catalog_*`),
`crates/pi-ai/tests/differential_providers.rs` (`catalog_matches_the_generated_ts_modules`).

## Symbols → proof

| TypeScript | Rust | Test |
|---|---|---|
| `<PROVIDER>_MODELS` (40 modules, `flattenModelCatalog(provider, data/<provider>.json)`) | `provider_models(provider)` over the embedded `data/providers/*.json` | `catalog_matches_the_generated_ts_modules` compares every provider's full model array and order against the TS modules |
| `MODELS` (`models.generated.ts`) | `builtin_provider_ids()` (catalog order) | same differential compares `getBuiltinProviders()` |
| `modelDataManifest.generatedAt` / `getBuiltinModelDataGeneratedAt` | `model_data_generated_at()` via `utils::iso8601` | `catalog_helpers_report_manifest_metadata`, differential `generatedAt` |
| `getBuiltinModel(provider, id)` | `builtin_model(provider, id)` | `catalog_keeps_ts_generated_order_and_metadata`, differential probes the first model of each provider |

## Branch notes

- `flattenModelCatalog` is `Object.assign({}, ...Object.values(groups))`: group
  values in file order, later duplicate ids replace the value without moving the
  key position. `flatten_catalog` mirrors both rules; `serde_json` runs with
  `preserve_order`.
- Unknown provider ids return `None`/empty rather than throwing, matching the
  optional chaining in `getBuiltinModel`/`getBuiltinModels`.

## Gaps

- `getBuiltinModelDataUrl` has no per-provider generated module in Rust; it
  returns the `file://` URL of the embedded catalog JSON (dev-only helper, no
  TypeScript test). See `providers-all.md`.

## Discrepancies

- `providers.test.ts` expects `cacheWrite: 0` for `moonshotai`/`moonshotai-cn`
  `kimi-k3`, but the pinned generated catalog ships `cacheWrite: 3`. Rust follows
  the catalog; the stale test assertion is recorded, not reproduced.
- `together-models.test.ts` still names `moonshotai/Kimi-K2.6`; the pinned catalog
  ships `moonshotai/Kimi-K3`. Rust asserts the catalog value.
