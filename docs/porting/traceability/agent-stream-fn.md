# Retired public stream-fn module

Frozen `4f9e6f8a4` has no `packages/agent-core/src/stream-fn.ts`. Agent/loop import `pi-ai/compat.streamSimple` directly.

Rust `agent/stream_fn.rs` now contains only a private compat adapter. The public module and get/set global default registry were removed. `defaults_to_compat_stream_without_registration` exercises constructor default streaming without global setup. See [Agent](agent.md) and [index](agent-index.md).
