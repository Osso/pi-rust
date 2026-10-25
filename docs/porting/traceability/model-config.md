# model-config — removed old-only architecture

`packages/coding-agent/src/core/model-config.ts` is absent from frozen `4f9e6f8a4`.
Q15 deletes `crates/pi-coding-agent-core/src/core/model_config.rs` and old-only tests.
The frozen [model-registry](model-registry.md) directly reads/validates models.json;
its schema is internal, not an alternative public configuration architecture.
Consumer migration and differential proof remain open.
