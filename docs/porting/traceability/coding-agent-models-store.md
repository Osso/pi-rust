# coding-agent models store traceability

Pinned source: `packages/coding-agent/src/core/models-store.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-coding-agent-core/src/core/models_store.rs`.
The `pi-ai` `ModelsStore` contract itself is documented in [models store traceability](models-store.md).

Focused tests: `crates/pi-coding-agent/tests/models_store.rs` (4 cases, ports of
`test/models-store.test.ts`). Differential test skipped: the store is a thin file layer over
`pi-ai`'s `ModelsStore`; its byte format is pinned by the focused tests and the shared JSON writer is
differentially proven in [js-json traceability](js-json.md).

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `models-store.ts:26-46` | `InMemoryCodingAgentModelsStore` clones entries and honors cancellation | `InMemoryCodingAgentModelsStore`; `in_memory_store_round_trips_entries` |
| `models-store.ts:48-77` | Constructor: normalized path, file backend, shared read state | `FileModelsStore::new`, `with_backend`; reload tests |
| `models-store.ts:79-118` | Coalesced reload with reader counting and cancellation | `Reload`, `start_reload`, `wait_for_reload`; `coalesces_reloads_across_readers_and_instances` |
| `models-store.ts:120-140` | `read` returns a clone; `write` merges one provider and writes pretty JSON; `delete` removes one | `read`/`write`/`delete`; persistence test, abort test |
| `models-store.ts:142-146` | Shared state is keyed by path | per-store `read_state`; cross-instance reload test |

## Gaps

- Entries are re-serialized from `ModelsStoreEntry` in the order `models, checkedAt, lastModified,
  etag` (the catalog provider's fresh-entry literal). A 304 refresh in TypeScript spreads the stored
  entry first and appends `checkedAt`, which can order fields differently. Rust rewrites the entry in
  its canonical order; readers accept both.

## Discrepancies

- None observed for the ported operations.

## Proof inventory

- Focused cases: 4.
