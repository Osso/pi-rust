# Slice 1: agent core API repin

Verified 2026-10-21 against immutable `4f9e6f8a4`, `packages/agent-core/src/{agent-loop,agent,types,proxy,node,index}.ts`.
Initial implementation revision: `f6ff6d9f`; callback-setter assertions strengthened at `65ba541c`. Verifier corrections are recorded below at `56ae8b4f`, integrated with master at `f1340ea5`.

Module-by-module comparison: [loop](agent-loop.md), [Agent](agent.md), [types](agent-types.md), [proxy](agent-proxy.md), [node](agent-node.md), [index](agent-index.md). Requirements: [core API spec](../../specs/agent-core-api.md).

## Native public prompt input (2026-10-22)

Frozen `agent.ts` prompt string path now maps to `PromptInput::Text(JsString)`, accepting owned or borrowed JsString without scalar projection. RED `agent-text-input-red.log` exposed the old String-only public boundary; GREEN `agent-text-input-green.log` proves two public prompt calls retain exact unpaired units in provider context and transcript wire. Ergonomic String/&str conversions remain direct lossless subset conversions, not alternate authorities.

## Rejectable queue callbacks (2026-10-22)

`MessageQueueHook` now returns `Result<Vec<AgentMessage>, AgentError>`; each low-level steering/follow-up poll propagates rejection without changing the error. Required by frozen harness queue drain rollback. API RED: `queue-rejection-red.log`; native-unit rejection and exact event-boundary GREEN: `queue-rejection-green.log`; 31 Agent +42 loop +4 verifier tests GREEN in `queue-rejection-callers.log`. Frozen differential: 35 retained scenarios +2 queue-rejection scenarios GREEN in `queue-rejection-differential.log`. Artifacts: `/tmp/pi-rust-repin-delta-agent-core/`.

The first differential run exposed a stale oracle-only `assistant([], "pending")` fixture in abort_stream, not frozen runtime behavior. Corrected to `"stop"` per frozen `ai/src/types.ts:407` and faux.ts:89; assertions were not normalized or weakened. Harness rollback itself remains part of the working-harness slice.

## Native iterator-return carrier (2026-10-22)

Frozen `agent-loop.ts:390–418` invokes an iterable's `return()` on early termination, not normal iterator exhaustion or non-abort next rejection. Native AI EventStream now carries an optional return callback (absent by default), preserving synchronous callback rejection separately from its returned future. RED: `iterator-return-red.log`; carrier GREEN: `iterator-carrier-green.log` (one contract test). Loop invocation RED `iterator-loop-red.log` observed missing return side effects. GREEN `iterator-loop-green.log`: two tests, including nine frozen direct-module scenarios (terminal event return, cancellation, normal exhaustion, next rejection, sink rejection, synchronous/asynchronous close rejection, cancelled rejecting close, cancellation during close). Return invocation surrounds final assistant preparation and message-end emission; default streams remain unchanged. Returned futures continue on Tokio when cancellation stops awaiting them, matching JS promise lifetime.

## Current evidence

All Cargo invocations use the mandatory `.cargo/config.toml` runner, `CARGO_BUILD_JOBS=4`, and `CARGO_TARGET_DIR=/home/osso/.worktrees/pi-rust-repin-agent-core-api/target`. Initial proof below predates the verifier corrections; no package-wide/workspace test suite, push, or delegation was run. The final targeted clippy and branch-local master merge are recorded below.

| Scope / command | Revision/scope | Result / artifact under `/tmp/pi-rust-repin-agent-core-api/` |
|---|---|---|
| `cargo test -p pi-agent-core --test agent --test agent_loop --test agent_repin --test differential_agent_loop` | f6ff6d9f | exit 0; 31 Agent + 41 loop + 7 repin tests; 35 frozen differential scenarios in one integration test; `green-core-final.log` |
| `cargo test -p pi-agent-core --test proxy` | c6dbbb47 proxy behavior; later changes only its source-path comment | exit 0, 6 tests; `green-proxy-final.log` |
| `cargo test -p pi-agent-core --test agent_abort --test agent_tool_execution_context --test agent_types --test before_tool_call_result` | a01f43c3 assertions; later changes do not invalidate their tested nonempty-error behavior or type fields | exit 0, 17 tests; `green-interruptions.log` |
| `cargo test -p pi-agent-core --test agent_continue_queue` | 56968c16+, unchanged queue scope | exit 0, 1 test; `green-options.log` |
| `cargo test -p pi-agent-core --test queue_mode --test tool_execution_mode --test thinking_level` | f6ff6d9f | enum proof in `green-core-enums.log` |
| 32 selected manual production mutants, each one focused target/filter | original 22 mutation sites unchanged; 2 nullish/truthy sites on f6ff6d9f and 8 mutable setter sites at 65ba541c; every edit restored byte-for-byte | 32/32 killed by behavioral failures, not compiler failures; `mutants.json`, `mutant-*.log`, `run-mutants.py`. One initial proxy event-arguments survivor prompted a stronger observable assertion, then was killed. Not an exhaustive mutation score. |
| `cargo test -p pi-agent-core --test agent forwards_frozen_agent_callbacks` | 65ba541c setter assertion expansion | exit 0; callbacks cleared and API key/budgets/retry/preparation changed across two runs; `green-mutable-setters.log` |
| `cargo check --workspace --all-targets` | current production code plus 65ba541c tests, all mutations restored | exit 0, zero warnings; `workspace-check-final.log` |
| `cargo fmt --all -- --check` | before each commit | exit 0 |

Existing `differential_agent_loop` already used `packages/agent-core/src/agent-loop.ts` at the branch base. The oracle loads adjacent frozen `agent.ts`; its support verifies source HEAD against source-lock. No stale core runtime oracle path needed replacement. Old unrelated harness/session/public-index typecheck paths remain outside this slice.

RED-first logs cover restoration, partial failure/reset, blocked termination (differential), replaced context, raw prepared call, duplicate metadata preparation, blocked reason, default provider, listener rejection/Set behavior, result metadata, proxy cancellation/error/options, after-result hints, parallel sibling lifetime, dynamic preparation, callback API, empty error state and nullish hook details. API-only additions initially failed compilation as absent fields/methods; runtime regressions failed observable assertions.

## Consumer proof and outstanding failures

Mechanical consumers migrate obsolete result fields; shared pi-ai ToolResultMessage, TextContent and Option<Value> representation are unchanged. Extension wrappers no longer diff/inject active-tool names. Core stop hooks no longer pass a signal; consumer adapters migrated.

- Direct changed coding-agent targets: 8/9 ordinary targets fully pass; `extensions` has 27 passing tests (including the wrapper migration) and 2 failures: missing frozen `test/model-runtime-test-utils.ts` in its existing oracle; unchanged registry registration-order test observed b.ts before a.ts.
- Direct changed differential targets: edit-preview, list-extensions, render-utils and tool-renderers pass. Interactive components fails in existing markdown-transform case (expected null vs Rust uppercase/appended strings), before migrated tool-result fixtures.
- Changed example consumers: dynamic-tools, hello, shutdown-command and structured-output pass. Eight targets fail: missing frozen helper/module paths (kimi-deferred-tools, tic-tac-toe, todo, tool-override, truncated-tool, with-deps); reload-runtime description drift ("and context files" extra); subagent nested model drift (`fixture/inherited` vs `child/model`). These are not fixed by changing core behavior.
- AgentSession image-result normalization tests (2) and real-session late dynamic-tool refresh (1) pass after consumer migration.

Logs: `coding-consumers.log`, `coding-consumer-differentials.log`, `coding-examples.log`, `agent_session-migration.log`, `agent_session_runtime-migration.log`. Full consumer acceptance is blocked; no all-green consumer claim. No live-provider e2e tests were run.

## Exact reference/representation gaps

Parent directed preserving these as explicit gaps for user classification; that is not approval of deviations or full parity.

- TS `agent.state` returns live mutable state: `state.messages.push(...)`, direct property assignment, and mutation of a returned message/tool can affect the next request. Rust state/messages/tools accessors return owned vectors/message values; mutation of a returned snapshot has no effect until a setter is called. Callback contexts/events likewise carry clones instead of shared TS object references. In-place prepareArguments mutation cannot mutate Rust's original raw call through object identity.
- TS proxy events retain the same mutable partial object; retaining a start/delta event and examining it later can show content/usage added by later events, and caller mutation can affect that shared object. Rust events hold owned snapshots, so earlier events remain unchanged and consumer mutation does not change future output.
- TS proxy assignment at contentIndex beyond length creates holes; JSON serialization yields null holes. Rust AssistantMessage.content is a dense typed vector. Sparse start indices are rejected with `Sparse proxy content index`, not materialized as holes.
- Iterator-return side effects are now explicitly ported and differentially proven (nine scenarios above); the former arbitrary-producer return gap is closed. Unknown proxy warning and full text/thinking/malformed SSE differential still await the proxy slice.
- Existing typed exclusions include non-object tool-call argument runtime values and lone surrogate strings; shared lossless content/details work belongs to its separate lane. This slice only changes nullish after-hook value selection, not its representation.

All six coverage rows remain todo. Root exports outside core (harness/session/telemetry/tools/search and consumed implementation helpers) remain for owning slices, per parent instruction. Scope and failures are not silently narrowed into a full-parity claim.

## Verifier correction proof — 2026-10-21

Code `56ae8b4f`; final integration `f1340ea5031157fac036f0800a7066bccd563ac3`, merging master `88b0295d8acb390a75f5d01a2acc7c8d3bb1b639`. Merge completed without conflicts; core source/tests identical to the fix commit. Frozen source remains `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`.

| Counterexample | Test in `agent_verifier_regressions` | Proof |
|---|---|---|
| End-turn reminder survives new steering | `steering::steering_clears_end_turn_reminder` | Second request contains assistant + new user request only |
| Rejected update waits forever for pending sibling | `lifetime::rejected_update_does_not_wait_for_pending_sink` | Low-level run rejects `update rejected` within the probe bound |
| Abort drops signal-ignoring tool | `lifetime::aborted_tool_keeps_ignored_signal_side_effect` | Run rejects `Agent run aborted`; tool's later side effect still occurs |
| Failing HTTP error-body read hides status | `proxy_body::failed_error_body_preserves_http_status` | Final error remains `Proxy error: 502 Bad Gateway` |

All four tests were derived from the verifier's frozen/Rust probes and failed behaviorally before production edits (`red.log`). All four pass (`green.log` and final `core-tests.log`). Four selected production mutants each fail its regression at the behavioral assertion (`mutants.json`, `mutant-*.log`); all mutations restored byte-for-byte. This is not exhaustive mutation coverage.

Tool execution now owns a Tokio task. Aborting races the JoinHandle without cancelling the task; a detached observer awaits completion, discards the late result, and logs any JoinError. The caller closes updates on race settlement. A signal-ignoring task may continue until completion or runtime shutdown; no hard-stop or indefinite runtime-lifetime guarantee is claimed. Tokio captures task panics (its panic hook may still print); they do not unwind the loop.

Final gates at `f1340ea5`: **118 core tests in 14 targets** (including 35 pinned differential scenarios), **108 report-GREEN coding-agent tests in 16 targets**, **2 AgentSession image cases**, **1 real late-tool refresh case**. Every target ran through the mandatory Cargo sandbox runner with `CARGO_BUILD_JOBS=4` and the worktree-local target directory. Workspace all-targets check: exit 0, zero warnings. `cargo clippy -p pi-agent-core -j4 --all-targets -- -D warnings` and fmt: exit 0. Manual changed-line readability audit found no new actionable violations; metrics binary unavailable. Full commands/revision/results: `/tmp/pi-rust-repin-agent-core-api-fixes/final-ledger.json`; logs beside it. Subsequent documentation-only commits do not invalidate this evidence.

The existing identity/sparse/iterator/typed gaps above remain unclassified; six coverage rows remain todo. Previously failing unrelated consumer targets were not rerun or claimed fixed.
