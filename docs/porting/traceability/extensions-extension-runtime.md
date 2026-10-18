# extensions/extension-runtime.ts → `core::extensions::extension_runtime`

Source: `packages/coding-agent/src/core/extensions/extension-runtime.ts`
(103 lines). Rust: `crates/pi-coding-agent/src/core/extensions/extension_runtime.rs`.

| Source | Rust | Tests |
| --- | --- | --- |
| `createUninitializedActions` | `uninitialized_actions` (exact message `Extension runtime not initialized. Action methods cannot be called during extension loading.`) | differential `providers` (pre-bind queueing) |
| `DEFAULT_STALE_MESSAGE` | same constant; runner `invalidate` uses it | `stale_contexts_reject_actions` |
| `createRuntimeState` | `RuntimeInner` + `ExtensionRuntime` | differential |
| `flagValues` | `flag_value`, `set_flag_value`, `set_default_flag_value`, `flag_values` | differential `flags` |
| `assertActive` | `assert_active` | `stale_contexts_reject_actions` |
| `invalidate` | same (first message wins, unsubscribes event bus) | `stale_contexts_reject_actions` |
| `trackEventBusSubscription` | same (idempotent tracked unsubscribe) | compile-time |
| `pendingProviderRegistrations` | `pending_provider_registrations` / `clear_pending…` | differential `providers` |
| `pendingNativeProviderRegistrations` | same | compile-time |
| `registerProvider` | `register_provider` (queue before bind, delegate after) | differential `providers` |
| `registerNativeProvider` | `register_native_provider` | compile-time |
| `unregisterProvider` | `unregister_provider` (removes queued entries by name/provider id) | differential `providers` |
| — | `ProviderHandlers`, `set_provider_handlers` | differential `providers` |

No gaps. Registration errors after bind propagate to the caller; queued
registration errors are reported by the runner as `register_provider`.
