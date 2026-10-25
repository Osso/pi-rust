# Tool detach registry

Frozen `packages/coding-agent/src/core/tool-detach-registry.ts`, all 63 lines, at `4f9e6f8a4`. Native `crates/pi-coding-agent-data/src/core/tool_detach_registry.rs`. Whole-module common-pin revalidation is complete under the existing typed-core and Rust-native subscription boundaries. Prior old-pin proof is not used as new-pin evidence.

| Frozen export / branch | Native and current proof |
|---|---|
| `ToolDetachHandle.detach` | Existing CallbackError Result prerequisite; `fallible_detach` compares TypeError/code/raw message and primitive/null/object throws against actual source |
| options / `DEFAULT_AUTO_DETACH_AFTER_MS` / constructor | Default 600000 matches; f64 delay now uses existing normalize_timeout_delay. Actual overflow-delay RED→GREEN recorded |
| `register` / Set identity / returned unsubscribe | IndexMap keys use Arc data-pointer identity, preserving Set insertion order/deduplication; strong returned subscriptions own the registry. Actual five-history duplicate callback/unsubscribe RED→GREEN |
| `detachRunning` | Reverse snapshot, first true, immediate original throw propagation; actual source callback-mutation snapshot proof and four original-throw stop-order histories |
| `detachAll` | Insertion-order snapshot, boolean result ignored, original throw stops later callbacks; actual source callback-mutation snapshot proof and four original-throw stop-order histories |
| `hasRunning` | Membership remains after detach; explicit unregister removes membership. Duplicate identity case now passes |
| timer callback / membership check / unref | Actual source/native retained-subscription, seven numeric delays, old duplicate timer with both present/removed membership, and active-default-timer natural process exit pass. Actual source/native TypeError and raw-string timer throws retain original markers and exit1 through the existing uncaught authority |
| `unregister` / clear latest timer | Retained cancellation tests and real source/native overwritten-old-timer proof: after duplicate register, first unsubscribe clears latest timer, then re-register lets old+new callbacks fire (2 calls) |

## Evidence before production fixes

- `tool-detach-sync-red-2.log`: ordinary distinct-handle histories match; duplicate same-handle registration differs. First attempt only had a fixture `as_number` compile typo, not behavioral RED.
- `tool-detach-lifetime-red.log`: actual frozen timer calls the handle after the caller drops the registry but retains unsubscribe; native loses it.
- `tool-detach-overflow-red.log`: actual frozen numeric overflow is normalized to 1ms; native waits the unnormalized duration.
- `tool-detach-callback-api-red.log`: exactly three incompatible infallible API diagnostics before the Result prerequisite. `tool-detach-callback-api-green.log`: four actual-source original-throw cases pass for both synchronous methods.
- `tool-detach-timer-throw-source.log`: real source child exits 1; monitor captures original TypeError/code/raw-D800. No fixture catch manufactures that uncaught outcome. `tool-detach-process-green.log` then proves native/source uncaught TypeError/code/raw-string metadata and exit1 through real private child processes (2 tests plus an explicitly invoked ignored helper).

Artifacts: `/tmp/pi-rust-repin-core-tools/`. New tests execute the actual pinned module; the historical claim that live handles/timers cannot be differential-tested is retired.

Current owned corrections pass all three new source differential tests, nine retained registry tests and the two error-boundary tests (`tool-detach-owned-first-green.log`, 14 passed plus the explicitly invoked process helper). Seven binary64 delay cases, duplicate timer-reference behavior and active-default-timer natural process exit additionally pass (3 tests plus invoked process helper, `tool-detach-timer-semantics.log`). Snapshot mutation additionally passes actual source/native comparison (`tool-detach-snapshot-green.log`, 1 test): removed entries still run, new entries wait for the next snapshot, and reentrant unregister/register does not deadlock. Four original-throw cases now prove later callbacks stop in either order (`tool-detach-error-stop-green.log`, 1 test). The final missing expired-timer/removed-membership branch also passes alongside re-registration (`tool-detach-old-timer-membership.log`, 1 test/two histories, a6ed0ac5). All 18 scoped tests are covered by retained current-scope proofs; two ignored private process helpers are explicitly invoked by their outer tests.

## Required consumer catch/escape audit

Frozen AgentSession:5875–5884 simply forwards; session runtime:439 forwards before restart branching. Bash:737–746 only aborts/reports activation. Pyrun detached-evaluation:465–478 performs lifecycle registration outside its activation-write catch; register failure must escape, activation failure returns false. Interactive:3767,3795–3804 invokes the background shortcut directly with no local catch. Required native updates follow those boundaries; producer error-schema corrections and unrelated state behavior are out of scope.

## Final gates

- Four required consumers pass at 1ae67df6: AgentSession aliases, restart-before-teardown, active Pyrun detach and restored Bash lifecycle (`tool-detach-{session,restart,pyrun,bash}-consumer.log`). Production bytes are unchanged at a6ed0ac5.
- Workspace all-target keep-going check at a6ed0ac5: zero errors/warnings and successful build-finished (`tool-detach-last-workspace-summary.json`).
- Six changed packages' all-target/all-feature warning Clippy at 1ae67df6: zero changed-primary-span findings, 1518 unrelated existing diagnostics (`tool-detach-clippy-summary.json`). The only later Rust change is the timer test; its a6ed0ac5 targeted all-feature Clippy has zero findings (`tool-detach-last-target-clippy-summary.json`). This is not a global lint-clean claim.
- Protected common-reference TypeScript and oracle Biome pass at a6ed0ac5 (`tool-detach-last-{types,biome}.log`). Format is clean; manual changed-line Rust/TS readability audit found no new production or fixture clarity violations. The metric binaries are unavailable; no suppressions, dependencies or fallback paths were added.
- Final source inventory passes with unchanged 686 rows/186679 lines, 456 done/19 gap/46 partial/165 todo (`tool-detach-inventory.log`).

## Historical bounded detachAll proof

Common-pin `detachAll` was previously added with AgentSession and restart consumers: actual source/native RED `restart-detach-all-red.log`, GREEN `restart-detach-all-{runtime,registry}-green.log` at `d6f01524`, retained `4d-final-runtime.log` at `98953083`. That proof remains scoped to detachAll/restart, not the other branches now under audit.

## Discrepancies / gaps

Duplicate registration overwrites only the stored latest timer reference in frozen source; the older unref timer is not explicitly cleared. This is a preserved frozen source discrepancy: real source/native unregister-and-re-register histories both produce two callbacks, from the old and new timers. The timer-owned sender prevents map replacement from incorrectly cancelling the older timer. Node TimeoutOverflowWarning/TimeoutNegativeWarning/TimeoutNaNWarning emissions are excluded under the [existing runtime-oddity ruling](../../../AGENTS.md) (explicit supervisor disposition 2026-10-24, the same ruling as [Codex](openai-codex-responses.md) and [RPC dialog timers](rpc-mode.md#dialog-timer-normalization)). The existing normalize_timeout_delay authority retains the same approved OS scheduling precision gap. Native callers retain an owned RAII subscription (Drop calls unregister), matching the retained/called unsubscribe boundaries of the audited built-in consumers; invalid JavaScript runtime input types remain excluded by the existing typed-core policy. [Contract](../../specs/tool-detach-registry.md).
