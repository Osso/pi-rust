# Tool detach registry

## What it must do

- [x] Detach callbacks and synchronous registry detach methods return `Result<_, CallbackError>`, retaining original Error branding/properties and arbitrary rejection values.
- [x] `detachRunning` snapshots newest-first, stops at first true, and propagates a callback failure immediately. `detachAll` snapshots insertion order, attempts every result until a callback failure, then propagates it.
- [x] AgentSession forwards those failures; restart forwards before either branch. Bash retains its existing abort/activation result. Pyrun lifecycle registration failure escapes outside its activation-write catch, matching frozen.
- [ ] Match Set identity for repeated registration of the same handle and remove it once.
- [ ] Keep registry state alive while its returned subscription remains alive, matching frozen's captured unsubscribe closure and timer lifetime.
- [ ] Accept binary64 delay values through the existing `normalize_timeout_delay` authority; preserve the 600000 default.
- [x] Compare native timer callback failure with the actual frozen uncaught process boundary, retaining original throws and exit 1.
- [ ] Retain unref/process-exit behavior independently from registry lifetime.

## How it works

Use the existing `CallbackError` and process-uncaught authority only at a proven uncaught boundary. Do not introduce a carrier, panic propagation between callers, replacement runtime or fallback path. [Trace](../porting/traceability/tool-detach-registry.md).

## Implementation inventory

- `crates/pi-coding-agent-data/src/core/tool_detach_registry.rs`: callback/registry contract and timers.
- `crates/pi-coding-agent-core/src/core/{agent_session,agent_session_runtime}.rs`: direct forwarding.
- `crates/pi-coding-agent-tools/src/core/tools/bash.rs`, bundled Pyrun detached evaluation and interactive background shortcut: required callback consumers only.

## Tests asserting this spec

- `fallible_detach`: actual frozen direct-method TypeError/code/raw-D800, raw string, null and lookalike-object throws versus native CallbackError; interface RED before production changes (`tool-detach-callback-api-red.log`), focused GREEN (`tool-detach-callback-api-green.log`).
- New whole-registry differential remains RED for duplicate handle identity, retained-subscription lifetime and overflowing delay. Artifacts: `tool-detach-{sync-red-2,lifetime-red,overflow-red}.log`; no whole-module closure claim.
- Actual frozen timer throw process probe exits 1 and records the original TypeError/code/raw message through `uncaughtExceptionMonitor` (`tool-detach-timer-throw-source.log`). Native/source TypeError/code/raw-string child-process comparisons now pass (`tool-detach-process-green.log`, 2 tests plus an ignored child helper explicitly invoked by the outer test).

## Known gaps (current cycle)

Whole module implementation/proof is in progress. Existing shared delay normalization documents omitted Node warning emission and OS scheduling precision; its disposition is recorded at final module closure, not silently presented as parity.

## Out of scope

Unrelated AgentSession/runtime/Pyrun behavior, producer error representations, new dependencies, shared validation changes and live state.
