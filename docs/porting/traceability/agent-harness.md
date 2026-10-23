# Working AgentHarness — common-pin re-port (in progress)

Frozen: `4f9e6f8a4`, `packages/agent-core/src/harness/agent-harness.ts` (1031 lines).
The prior scaffold and its old traceability are replaced, not retained as fallback.

| Frozen lines | Native implementation | Current proof |
|---|---|---|
| 38–64, 344–380, 516–602 | `agent_harness/runtime.rs`: native user/failure content, awaiting request hooks, forwarding headers/options/metadata, failure catch | `agent_harness`: rejected request hook persists one empty-text assistant with exact unpaired error; unpaired header-name conversion rejects before provider dispatch |
| 67–124 | `stream_options.rs` native snapshots/patches | Prior `harness_stream_options` + unit stream patch proof; working request-chain oracle pending |
| 126–342, 385–515 | `agent_harness.rs`, `runtime.rs`: construction, hooks/listeners, turn state, queues, writes/events | `differential_agent_harness`: 14 frozen scenarios cover real faux turns, native stream patches, context/start hooks, deferred writes, rejected drains/reporting, sequential/all queues and live registration during hook dispatch |
| 604–708, 835–1031 | runtime/settings/resources/abort/idle | Seven native tests include abort/idle/snapshot concurrency; partial focused proof: classify pre-loop hook failures without wrapping existing harness codes; `jsonl_harness_persists_failure_across_storage_reopen` proves actual JSONL persistence, including raw unpaired failure text; remaining matrix pending |
| 710–833 | `navigation.rs`: compaction/tree hooks, persistence, user/custom editor text | 13 frozen navigation/settings scenarios: no-op/current/missing target, user/custom editor text, hook cancellation, empty/provided/generated summaries, ignored labels, native compaction hook result and cancellation, duplicate/missing tool validation, resource/settings persistence |

## Pending deviations / audit

- **Approved deviation (2026-10-22, AGENTS.md general owned-values rule):** `getModel()` (835–837), nested resource objects (956–962), and turn snapshots return owned Rust values. Built-in-consumer audit `/tmp/pi-rust-repin-delta-agent-core/harness-getter-consumers.txt` found no identity/mutation-through-return reliance. Frozen tests asserting identity are covered by this approval.
- HTTP header names cross the core → AI boundary via fallible `JsString::to_utf8()` under the approved opaque-name policy; errors propagate through StreamFn, then frozen executeTurn's failure reporting. Header values and arbitrary metadata remain native.
- `JsObject` metadata is forwarded directly; any serialization must wrap `JsValue::Object`, never scalar `serde_json::to_value`.
- Errors retain `AgentHarnessError` name, frozen auth/error codes, native subsystem causes and aggregate error members. Generic session metadata permits both memory and JSONL storage in the same harness implementation; no alternate persistence path.
- Whole-module coverage remains `todo` until branch/export proof is complete. Current proof: seven native boundary/persistence/classification/concurrency cases, 16 direct frozen turn/queue/payload scenarios, 13 navigation/settings scenarios. `harness-concurrency-green3.log` proves abort waits through blocked settled reporting, busy prompt/compact/tree rejection, signal cancellation, next-turn retention, single-vs-aggregate abort causes and next-request snapshot refresh/deferred writes. Additional failure branches remain. All three tool scenarios now GREEN unchanged: merged native Tool label/executionMode carrier is populated at AgentTool→Tool conversion (agent-loop.ts:475–479 forwards the whole tool). Exact faux usage now matches frozen, including the serialized human label. RED/GREEN `harness-tool-carrier-{red,green}.log`.
- `before_provider_payload` (274–290) carries explicit undefined between handlers as `Option<JsValue>`; undefined does not become null or the previous payload. RED `harness-payload-red.log`; GREEN `harness-payload-green.log` covers undefined chaining, raw final payload and raw rejection through a real frozen faux factory invoking the provider callback.
- **DISCREPANCY, preserved:** navigation accepts option/hook labels (771, 790), but never appends them when moving the leaf (823–830). Differential asserts the absence of label entries; no silent fix.
- Differential RED `working-harness-matrix-red5.log` found registration-time snapshots skipped hooks added while dispatching. Native registries now use live ordered iteration and Arc-identity deduplication, matching TS Set. GREEN `working-harness-matrix-green.log`; oracle intercepts `globalThis.fetch` and prohibits unexpected HTTP, uses actual frozen faux Models/session/harness modules.

## Failure classification and aggregate cause closure

`harness_failure_boundaries` compares five direct frozen workflows using actual JSONL storage and a failing filesystem. Returned FileErrors retain their SessionError wrapper; rejected native TypeErrors use compaction/branch_summary fallback codes at compact/navigate persistence. Request-hook auth failure followed by assistant-message reporting failure retains both original AgentHarnessErrors in the AggregateError. RED `harness-failure-boundary-red3.log` exposed all three lost-code/class cases; GREEN `harness-failure-boundary-green1.log`. AgentError carries its original typed source until failure aggregation; no fabricated Error wrapper or lossy text conversion.

## Callback rejection projection

`runtime::callback_tests::provider_callbacks_reject_with_original_harness_error_data` captures the actual frozen harness-supplied Models options, invokes onPayload/onResponse directly, and compares callback rejections to native forwarded callbacks. Four frozen cases retain AgentHarnessError/auth plus absent or native TypeError causes containing unmatched units. RED `harness-callback-data-red3.log`; GREEN `harness-callback-data-green2.log`. This is callback invocation proof, not a claim that faux itself invokes onResponse.

CallbackError retains the original typed Error independently from its AbortReason projection. Built-in harness/session/compaction/branch/FileError/ExecutionError/AggregateError, native errors, raw AbortReason values and JSON SyntaxErrors retain their literal fields and causes. Native projection/source tests include a nonfinite raw cause and preserve error branding. CallbackError equality compares structured fields plus original-source Arc identity; it does not claim JS object aliasing parity.

**Approved Rust-native extension projection (parent/user direction 2026-10-22):** arbitrary extension Rust Error types remain available as the typed source; their JS projection is plain Error with the exact Display message and no fabricated code/cause. They are not rejected and do not change control flow. This is an explicit Rust-native-extension deviation, not a fallback for frozen built-ins. Stack text remains omitted under the approved stack gap.
