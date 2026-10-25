# coding-agent models-store — removed old-only architecture

`packages/coding-agent/src/core/models-store.ts` is absent from frozen `4f9e6f8a4`.
Q15 deletes `crates/pi-coding-agent-core/src/core/models_store.rs` and its tests.
No fallback/publication store is retained. This is not a frozen parity gap.
Frozen catalog persistence belongs to [model-catalog-cache](model-catalog-cache.md).
The separate old pi-ai models-store surface awaits its models.ts contract slice.
