# Tool detach registry

## What it must do

- [x] Detach callbacks and synchronous registry detach methods return `Result<_, CallbackError>`, retaining original Error branding/properties and arbitrary rejection values.
- [x] `detachRunning` snapshots newest-first, stops at first true, and propagates a callback failure immediately. `detachAll` snapshots insertion order, attempts every result until a callback failure, then propagates it.
- [x] AgentSession forwards those failures; restart forwards before either branch. Bash retains its existing abort/activation result. Pyrun lifecycle registration failure escapes outside its activation-write catch, matching frozen.
- [x] Match Set identity for repeated registration of the same handle and remove it once.
- [x] Keep registry state alive while its returned subscription remains alive, matching frozen's captured unsubscribe closure and timer lifetime.
- [x] Accept binary64 delay values through the existing `normalize_timeout_delay` authority; preserve the 600000 default.
- [x] Compare native timer callback failure with the actual frozen uncaught process boundary, retaining original throws and exit 1.
- [x] Retain unref/process-exit behavior independently from registry lifetime: active default timers do not keep either real child process alive.

## How it works

Use the existing `CallbackError` and process-uncaught authority only at a proven uncaught boundary. Do not introduce a carrier, panic propagation between callers, replacement runtime or fallback path. [Trace](../porting/traceability/tool-detach-registry.md).

## Implementation inventory

- `crates/pi-coding-agent-data/src/core/tool_detach_registry.rs`: callback/registry contract and timers.
- `crates/pi-coding-agent-core/src/core/{agent_session,agent_session_runtime}.rs`: direct forwarding.
- `crates/pi-coding-agent-tools/src/core/tools/bash.rs`, bundled Pyrun detached evaluation and interactive background shortcut: required callback consumers only.

## Tests asserting this spec

- `fallible_detach`: actual frozen direct-method TypeError/code/raw-D800, raw string, null and lookalike-object throws versus native CallbackError; interface RED before production changes (`tool-detach-callback-api-red.log`), focused GREEN (`tool-detach-callback-api-green.log`).
- Actual whole-registry differentials were RED for duplicate handle identity, retained-subscription lifetime and overflowing delay; these now pass alongside the nine retained registry tests (`tool-detach-owned-first-green.log`, 14 passed plus an explicitly invoked ignored process helper). Artifacts: `tool-detach-{sync-red-2,lifetime-red,overflow-red}.log`.
- Actual frozen timer throw process probe exits 1 and records the original TypeError/code/raw message through `uncaughtExceptionMonitor` (`tool-detach-timer-throw-source.log`). Native/source TypeError/code/raw-string child-process comparisons now pass (`tool-detach-process-green.log`, 2 tests plus an ignored child helper explicitly invoked by the outer test).

- `tool_detach_timer_semantics`: seven finite/nonfinite binary64 delay histories, actual overwritten-old-timer behavior after unregister/re-register, and source/native active-default-timer natural process exit pass (3 tests plus an explicitly invoked ignored child helper; `tool-detach-timer-semantics.log`).

- Snapshot callback mutation matches actual source: insertion/reverse snapshots still call removed entries and omit new entries; callbacks can unregister/register without a lock being held across the callback (`tool-detach-snapshot-green.log`, 1 passed). The four original-rejection histories now include first/last continuing callbacks and prove iteration stops at the throwing callback (`tool-detach-error-stop-green.log`, 1 passed).

## Known gaps (current cycle)

Node Overflow/Negative/NaN timer warnings are covered by the existing runtime-oddity gap (explicit supervisor disposition 2026-10-24). The requested shared delay authority retains its documented OS scheduling precision limitation. Native subscriptions are owned RAII values; built-in consumers retain them until the frozen unsubscribe boundary. Invalid JavaScript runtime input types are excluded by the existing typed-core policy.

Whole-module branch proof and the final consumer/static gates are complete; see the [trace and exact gate revisions](../porting/traceability/tool-detach-registry.md). The final expired-old-timer history also proves removed membership skips the callback (`tool-detach-old-timer-membership.log`).

## Out of scope

Unrelated AgentSession/runtime/Pyrun behavior, producer error representations, new dependencies, shared validation changes and live state.
