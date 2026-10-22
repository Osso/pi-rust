# Working AgentHarness — common-pin re-port (in progress)

Frozen: `4f9e6f8a4`, `packages/agent-core/src/harness/agent-harness.ts` (1031 lines).
The prior scaffold and its old traceability are replaced, not retained as fallback.

| Frozen lines | Native implementation | Current proof |
|---|---|---|
| 38–64, 344–380, 516–602 | `agent_harness/runtime.rs`: native user/failure content, awaiting request hooks, forwarding headers/options/metadata, failure catch | `agent_harness`: rejected request hook persists one empty-text assistant with exact unpaired error; unpaired header-name conversion rejects before provider dispatch |
| 67–124 | `stream_options.rs` native snapshots/patches | Prior `harness_stream_options` + unit stream patch proof; working request-chain oracle pending |
| 126–342, 385–515 | `agent_harness.rs`, `runtime.rs`: construction, hooks/listeners, turn state, queues, writes/events | Implemented; whole-module oracle and concurrency matrix pending |
| 604–708, 835–1031 | runtime/settings/resources/abort/idle | Implemented; focused matrix pending |
| 710–833 | `navigation.rs`: compaction/tree hooks, persistence, user/custom editor text | Implemented; focused/differential matrix pending |

## Pending deviations / audit

- `getModel()` (835–837), nested resource objects (956–962), and turn snapshots return owned Rust values. General identity/mutation-through-return rule remains **pending user approval**; not claimed parity. Prior built-in-consumer audit: `/tmp/pi-rust-repin-delta-agent-core/harness-getter-consumers.txt` found no built-in reliance; frozen tests explicitly assert identity.
- HTTP header names cross the core → AI boundary via fallible `JsString::to_utf8()` under the approved opaque-name policy; errors propagate through StreamFn, then frozen executeTurn's failure reporting. Header values and arbitrary metadata remain native.
- `JsObject` metadata is forwarded directly; any serialization must wrap `JsValue::Object`, never scalar `serde_json::to_value`.
- Whole-module coverage remains `todo` until branch/export proof is complete. Two initial behavioral RED/GREEN cases do not establish whole-module parity.
