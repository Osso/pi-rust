# Working AgentHarness — common-pin re-port (in progress)

Frozen: `4f9e6f8a4`, `packages/agent-core/src/harness/agent-harness.ts` (1031 lines).
The prior scaffold and its old traceability are replaced, not retained as fallback.

| Frozen lines | Native implementation | Current proof |
|---|---|---|
| 38–64, 344–380, 516–602 | `agent_harness/runtime.rs`: native user/failure content, awaiting request hooks, forwarding headers/options/metadata, failure catch | `agent_harness`: rejected request hook persists one empty-text assistant with exact unpaired error; unpaired header-name conversion rejects before provider dispatch |
| 67–124 | `stream_options.rs` native snapshots/patches | Prior `harness_stream_options` + unit stream patch proof; working request-chain oracle pending |
| 126–342, 385–515 | `agent_harness.rs`, `runtime.rs`: construction, hooks/listeners, turn state, queues, writes/events | `differential_agent_harness`: 14 frozen scenarios cover real faux turns, native stream patches, context/start hooks, deferred writes, rejected drains/reporting, sequential/all queues and live registration during hook dispatch |
| 604–708, 835–1031 | runtime/settings/resources/abort/idle | Implemented; focused matrix pending |
| 710–833 | `navigation.rs`: compaction/tree hooks, persistence, user/custom editor text | Implemented; focused/differential matrix pending |

## Pending deviations / audit

- **Approved deviation (2026-10-22, AGENTS.md general owned-values rule):** `getModel()` (835–837), nested resource objects (956–962), and turn snapshots return owned Rust values. Built-in-consumer audit `/tmp/pi-rust-repin-delta-agent-core/harness-getter-consumers.txt` found no identity/mutation-through-return reliance. Frozen tests asserting identity are covered by this approval.
- HTTP header names cross the core → AI boundary via fallible `JsString::to_utf8()` under the approved opaque-name policy; errors propagate through StreamFn, then frozen executeTurn's failure reporting. Header values and arbitrary metadata remain native.
- `JsObject` metadata is forwarded directly; any serialization must wrap `JsValue::Object`, never scalar `serde_json::to_value`.
- Whole-module coverage remains `todo` until branch/export proof is complete. Current proof: two native boundary cases plus 14 direct frozen-module differential scenarios; missing tree/compaction/tool/abort concurrency and settings/error-classification branches.
- Differential RED `working-harness-matrix-red5.log` found registration-time snapshots skipped hooks added while dispatching. Native registries now use live ordered iteration and Arc-identity deduplication, matching TS Set. GREEN `working-harness-matrix-green.log`; oracle intercepts `globalThis.fetch` and prohibits unexpected HTTP, uses actual frozen faux Models/session/harness modules.
