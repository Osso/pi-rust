# remote-catalog-provider — removed old-only architecture

`packages/coding-agent/src/core/remote-catalog-provider.ts` is absent from frozen `4f9e6f8a4`.
Q15 deletes `crates/pi-coding-agent-core/src/core/remote_catalog_provider.rs` and its old-only tests.
No remote-provider wrapper is retained. Frozen OpenRouter startup/refresh behavior belongs
to [model-catalog-cache](model-catalog-cache.md), wired into [model-registry](model-registry.md).
Production wiring and acceptance remain open.
