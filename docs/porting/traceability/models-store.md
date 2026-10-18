# models store traceability

Source: `packages/ai/src/models-store.ts` → `crates/pi-ai/src/models_store.rs`.
Tests: `crates/pi-ai/tests/models_store.rs`.

## Symbols → proof
| TS | Rust | Test |
|---|---|---|
| `ModelsStoreEntry` | same (`lastModified`/`checkedAt` use JS-number formatting) | `models_store_entry_serializes_camel_case_timestamps` |
| `ModelsStoreOperationOptions` | same (`signal` → `CancellationToken`) | cancellation test |
| `ModelsStore` (`read/write/delete`) | trait | store tests + models refresh tests |
| `InMemoryModelsStore` | same | `in_memory_models_store_round_trips_clones_and_deletes`, `in_memory_models_store_honors_cancellation` |

## Semantics
- Reads and writes clone entries, matching `structuredClone`.
- All operations check the signal before touching the map and return `AuthError::Aborted`.
- Timestamps and ETag round-trip verbatim.

## Gaps
- None. Persistent stores are app-owned, as in TS.
