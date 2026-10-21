# Slice 1: agent core API repin

Verified 2026-10-21 against immutable `4f9e6f8a4`, `packages/agent-core/src/{agent-loop,agent,types,proxy,node,index}.ts`.
Production implementation revision: `f6ff6d9f`; callback-setter assertions strengthened at `65ba541c`. Later commits update documentation only.

Module-by-module comparison: [loop](agent-loop.md), [Agent](agent.md), [types](agent-types.md), [proxy](agent-proxy.md), [node](agent-node.md), [index](agent-index.md). Requirements: [core API spec](../../specs/agent-core-api.md).

## Current evidence

All Cargo invocations use the mandatory `.cargo/config.toml` runner, `CARGO_BUILD_JOBS=4`, and `CARGO_TARGET_DIR=/home/osso/.worktrees/pi-rust-repin-agent-core-api/target`. No clippy, package-wide/workspace test suite, push, merge, or delegation was run.

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
- TS iterateWithAbort calls iterator.return() on cancellation. Rust drops receiver futures; arbitrary producers do not receive a JavaScript-style return callback. Unknown proxy event warning logs are not reproduced. Proxy full text/thinking/malformed SSE differential matrix is not claimed.
- Existing typed exclusions include non-object tool-call argument runtime values and lone surrogate strings; shared lossless content/details work belongs to its separate lane. This slice only changes nullish after-hook value selection, not its representation.

All six coverage rows remain todo. Root exports outside core (harness/session/telemetry/tools/search and consumed implementation helpers) remain for owning slices, per parent instruction. Scope and failures are not silently narrowed into a full-parity claim.
