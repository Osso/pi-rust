# Tool detach registry

Frozen `packages/coding-agent/src/core/tool-detach-registry.ts`, all 63 lines, at `4f9e6f8a4`. Native `crates/pi-coding-agent-data/src/core/tool_detach_registry.rs`. Whole-module revalidation is in progress; prior old-pin proof is not current closure evidence.

| Frozen export / branch | Native and current proof |
|---|---|
| `ToolDetachHandle.detach` | Existing CallbackError Result prerequisite; `fallible_detach` compares TypeError/code/raw message and primitive/null/object throws against actual source |
| options / `DEFAULT_AUTO_DETACH_AFTER_MS` / constructor | Default 600000 matches; existing u64 option must become f64 through shared timer normalization. Actual overflow-delay RED recorded |
| `register` / Set identity / returned unsubscribe | Existing per-registration IDs differ for the same handle. Actual five-history source/native RED shows duplicate callbacks and surviving registration after first unsubscribe |
| `detachRunning` | Reverse snapshot, first true, immediate original throw propagation; retained normal tests and new fallible source histories |
| `detachAll` | Insertion-order snapshot, boolean result ignored, original throw stops later callbacks; common-pin retained test plus fallible source histories |
| `hasRunning` | Membership remains after detach; explicit unregister removes membership. Duplicate identity case remains RED |
| timer callback / membership check / unref | Actual source unref timer still fires while another ref'ed activity keeps process alive; retained-subscription/dropped-registry native Weak lifetime is RED. Actual source callback throw exits process 1; native error route uses existing uncaught authority; actual source/native error/raw-string process comparisons pass with original throw markers and exit1 |
| `unregister` / clear latest timer | Retained cancellation tests; duplicate-handle/latest-timer Map semantics still require full common-pin proof |

## Evidence before production fixes

- `tool-detach-sync-red-2.log`: ordinary distinct-handle histories match; duplicate same-handle registration differs. First attempt only had a fixture `as_number` compile typo, not behavioral RED.
- `tool-detach-lifetime-red.log`: actual frozen timer calls the handle after the caller drops the registry but retains unsubscribe; native loses it.
- `tool-detach-overflow-red.log`: actual frozen numeric overflow is normalized to 1ms; native waits the unnormalized duration.
- `tool-detach-callback-api-red.log`: exactly three incompatible infallible API diagnostics before the Result prerequisite. `tool-detach-callback-api-green.log`: four actual-source original-throw cases pass for both synchronous methods.
- `tool-detach-timer-throw-source.log`: real source child exits 1; monitor captures original TypeError/code/raw-D800. No fixture catch manufactures that uncaught outcome. `tool-detach-process-green.log` then proves native/source uncaught TypeError/code/raw-string metadata and exit1 through real private child processes (2 tests plus an explicitly invoked ignored helper).

Artifacts: `/tmp/pi-rust-repin-core-tools/`. New tests execute the actual pinned module; the historical claim that live handles/timers cannot be differential-tested is retired.

## Required consumer catch/escape audit

Frozen AgentSession:5875–5884 simply forwards; session runtime:439 forwards before restart branching. Bash:737–746 only aborts/reports activation. Pyrun detached-evaluation:465–478 performs lifecycle registration outside its activation-write catch; register failure must escape, activation failure returns false. Interactive:3767,3795–3804 invokes the background shortcut directly with no local catch. Required native updates follow those boundaries; producer error-schema corrections and unrelated state behavior are out of scope.

## Historical bounded detachAll proof

Common-pin `detachAll` was previously added with AgentSession and restart consumers: actual source/native RED `restart-detach-all-red.log`, GREEN `restart-detach-all-{runtime,registry}-green.log` at `d6f01524`, retained `4d-final-runtime.log` at `98953083`. That proof remains scoped to detachAll/restart, not the other branches now under audit.

## Discrepancies / gaps

Duplicate registration overwrites only the stored latest timer reference in frozen source; the older unref timer is not explicitly cleared. This source branch requires a behavioral classification/proof before native identity changes are closed. Shared normalization explicitly does not model Node warning emission/OS timer precision; no silent parity claim is made for those host effects. General Rust-native subscription/owned-value boundaries remain explicit. [Contract](../../specs/tool-detach-registry.md).
