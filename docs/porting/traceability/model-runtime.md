# model-runtime — removed old-only architecture

`packages/coding-agent/src/core/model-runtime.ts` does not exist at frozen `4f9e6f8a4`.
Q15 deletes `crates/pi-coding-agent/src/core/model_runtime.rs` and its old-only tests;
this is deletion, not a parity gap or relaxed assertion.

Frozen replacement: [model-registry](model-registry.md), backed directly by
[auth-storage](auth-storage.md). Consumer migration and verification remain open.
