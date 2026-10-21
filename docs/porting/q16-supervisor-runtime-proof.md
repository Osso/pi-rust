# Q16 supervisor / architect / resident / runtime re-pin

Reference: immutable [common lock](source-lock.json), `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`.
Base: `cb1b6ff3`; code/test revision: `6f75462b0e1b19dac6058cd3b39170b3332e751e`.

**Partial, not ready for whole-slice parity.** Two unignored dependency REDs remain; default process effects and some branch matrices are not fully differential. Source checkout remained clean. Approved resident regex [delta](delta-lock.json) is unchanged. Q14 standalone behavior, host relay, shared text/argument representation, and live services were not reimplemented.

## Capability matrix

| Frozen module | Current capability / proof | Remaining boundary |
|---|---|---|
| supervisor/approval-reviewer | Ten-case actual frozen escalation/error differential + native fixtures | None in tested typed contract; [map](traceability/supervisor-approval-reviewer.md) |
| supervisor/client | Startup, pre-abort/startup-abort, durable polling cancellation, completion, retry cancellation/timeout/zero attempts; eight-case frozen response/effects/row matrix | Random jitter/non-finite timer inputs not exhaustive; [map](traceability/supervisor-client.md) |
| supervisor/ensure-running | Invocation, local coalescing, cross-process lock protocol, version reuse/replacement, readiness, timeout/error; six frozen fake-process traces | Raw package version mismatch; default process effects compiled only; [map](traceability/ensure-running.md) |
| supervisor/main | Model/auth selection, veto/reviewer/evaluator, hidden policy feedback, identity, resident opening, cancellation across compaction/prompt/review | Full transcript dependency RED; live signal/service wiring deliberately not run; [map](traceability/supervisor-main.md) |
| supervisor/project-resolver | Frozen resolver matrix plus BOM/no-slash URL/read-error counterexamples; native git fixtures | Exhaustive malformed URL/FS matrix not claimed; [map](traceability/project-resolver.md) |
| supervisor/request-wake | Nine fixtures and frozen close/start/new-wake generation differential; old shutdown/watch bugs removed | Non-Linux Unix environments not executed; [map](traceability/request-wake.md) |
| supervisor/response-tool | Exact frozen metadata/schema and eight execution cases | [Map](traceability/supervisor-response-tool.md) |
| supervisor/service | Cumulative authority prompts, exact veto token/context/feedback, empty-text parser branch, cancellation precedence/evaluator failure | Exhaustive cancellation/preemption interleavings not claimed; [map](traceability/supervisor-service.md) |
| architect/main | Resident-file preparation; native queue/console/gate/cycle fixtures and frozen request-cycle differential | Old `index_message_text: Some(false)` store option awaits shared SessionManager API reconciliation; live service not executed; [map](traceability/architect-main.md) |
| architect/observer | Query/observation/state/claim/completion fixtures + frozen request-cycle differential | Exhaustive SQL/filter/sort differential not claimed; [map](traceability/architect-observer.md) |
| architect/project-spec | Canonical project root, TS dotdot prefix policy, Node UTF-8 spec/index reads and missing-realpath errors; frozen counterexamples | Exhaustive failure metadata/cwd environment boundary not claimed; [map](traceability/architect-project-spec.md) |
| architect/prompt | Exact frozen constants and serialized prompt differential | [Map](traceability/architect-prompt.md) |
| core/resident-console-transport | Probe without owner takeover; identity preservation/validation; cross-language attach/prompt/events fixtures | Exhaustive malformed-wire/lifetime matrix not claimed; [map](traceability/resident-console-transport.md) |
| core/resident-session | Frozen exact-discrepancy and approved-regex-delta matrices pass; preparation reused by resident openers | Existing Unix/Windows boundary retained; [map](traceability/resident-session.md) |
| core/self-restart | Empty handoff/model resets, pending results, default EPERM probe corrected; frozen pending-results and native exec/spawn/env fixtures | Persisted detached-job result branch/full-module differential not exhaustive; [map](traceability/self-restart.md) |
| core/runtime-process | Frozen command-line and superseded-incarnation differentials; existing Linux identity behavior | Darwin backend absent; [map](traceability/runtime-process.md) |
| core/runtime-process-darwin | Explicit platform/native gap classification with frozen binding/validation evidence | No Rust Darwin backend or new parity approval; [map](traceability/runtime-process-darwin.md) |

## Dependency REDs and integration notes

1. `q16_startup::advertised_runtime_version_matches_frozen_config_dependency`: Rust embedded package reports `0.84.1`; frozen `config.VERSION` reports `0.80.3`. Kept unignored. Startup algorithm traces parameter-normalize the version only; separate raw equality protects the invariant.
2. `differential_supervisor_request_loop`: actual frozen transcript contains `codex_restriction_context` custom entry absent from Rust AgentSession. Full comparison remains; no entry filtering or weakened assertion. Rust-owned request results otherwise match the exercised approval case.
3. Frozen `SessionManager.setMetadataControlDbPath` no longer accepts `indexMessageText`. Rust shared API still exposes it; Architect store passes the old `Some(false)` option. Shared session-manager owner must reconcile it. Three request caller literals received only mechanical `signal: None`; caller-specific cancellation belongs to their owners.
4. Parent approved mandatory external `setsid` (2026-10-21) because unsafe pre-exec hooks are forbidden. No shared generic detached spawn helper existed. Default target-exec failure timing is not proven by the fake-process matrices; approval is not treated as proof of identical launch errors.
5. Lossless-content integration remains parent-owned: existing String/serde_json request/session surfaces were not globally migrated here. Reconcile the new evaluator/reviewer interfaces with that lane before claiming UTF-16 parity.

## Proof ledger

All Cargo commands use cwd `/home/osso/.worktrees/pi-rust-repin-supervisor-runtime`, `CARGO_BUILD_JOBS=4`, `CARGO_TARGET_DIR=<worktree>/target`; tests enter `.cargo/config.toml`'s mandatory `scripts/cargo-runner.sh`. No live Supervisor, Architect or external provider was started; providers/processes are fakes and sockets are private fixture sockets. No clippy, workspace test suite, push, merge or delegation.

Raw evidence: `/tmp/pi-rust-repin-supervisor-runtime/`.

| Scope / command | Revision / validity | Result / artifact |
|---|---|---|
| Focused 16-target Cargo batch: architect, prompt/cycle, approval, response/service, resolver, wake, resident transport/delta, restart/env, identity/token/open/client | `5daa8a8b`; later intersecting targets have narrower follow-up proof | 114 tests passed; `focused-revalidation.log` |
| `cargo test -p pi-coding-agent --test q16_runtime_repin` | `6d4714c1`; later self-restart syscall predicate does not change these fixture paths | 11 passed; `green-architect-node-io.log` |
| `cargo test -p pi-coding-agent --test q16_client` | `6f75462b` | Eight actual frozen scenarios in one test pass; `client-differential.log` |
| `cargo test -p pi-coding-agent --test q16_approval` | `b5a724cf`; reviewer unchanged later | Ten actual frozen cases pass; `approval-differential.log` |
| Changed-target batch: q16_new_exports, q16_startup, q16_veto_matrix, request_wake, supervisor | `ff71b54a`; cancellation listener later receives targeted regression proof | 34 passed, one intentional dependency RED; `final-changed-targets.log` |
| `cargo test -p pi-coding-agent --test supervisor cancelling_during_compaction` | `457190b5`; later edits unrelated | RED timeout then GREEN; `red-compaction-cancellation.log`, `green-compaction-cancellation.log` |
| `cargo test -p pi-coding-agent --lib self_restart_wait_treats_permission_errors` | `e4b6affd`; final code unchanged later | Raw syscall-result predicate RED/GREEN; `red-restart-permission-probe.log`, `green-restart-permission-probe.log` |
| `cargo test -p pi-coding-agent --test q16_veto_matrix` and `--test supervisor process_request` | `6d4714c1`; later self-restart/test-only changes do not intersect | Exact nine-case frozen feedback/response matrix and stale-boundary fixture pass; `final-veto-matrix.log`, `final-request-boundary.log` |
| `cargo test -p pi-coding-agent --test differential_core_leaves project_resolver_matches_typescript` and `runtime_process_command_line_detection_matches_typescript` | `b5a724cf`; relevant source unchanged later | Both pass; correspondingly named `.log` artifacts |
| `cargo check --workspace --all-targets` | `6f75462b`; only documentation changes afterward | Exit 0, zero warnings; `cargo-check-workspace-all-targets.log` |
| `cargo fmt --all --check` | Before each commit; final docs-only commit does not change Rust | Exit 0 |

Early RED/GREEN artifacts also retain the objective prompt, empty text, handoff/model reset, probe/identity, resident opening, URL/config, canonical-path, Node decoding, evaluator error, cancellation and console-error counterexamples. New absent-export/API tests initially failed compilation, not behavior; those are not represented as behavioral REDs. No RED was hidden by weakening an assertion.

## Mutation evidence

Three independent temporary Rust mutants each compiled and failed behavioral assertions, then original bytes were restored in `finally`:

| Mutant | Killing test | Result |
|---|---|---|
| Drop replacement incarnation-presence requirement | `q16_new_exports::superseded_identity_matches_frozen_matrix` | Missing incarnation incorrectly supersedes; killed |
| Trim instruction decision token | `q16_new_exports::instruction_review_tokens_and_context_match_frozen` | `ACCEPT\n` incorrectly accepted; killed |
| Preserve rejected tactical instructions | `q16_veto_matrix::full_veto_decision_and_feedback_matrix_matches_frozen` | Exact rejected response differs; killed |

Exact mutated file SHA256, exit 101 and logs: `mutants.json`, `mutant-*.log`. Later edits do not change these predicates; final relevant original-source tests pass. Historical resident mutants are not counted as new Q16 evidence.

## Coverage / audit boundary

Only newly proven resident-session is promoted from todo to done. Existing approval/prompt/response/resolver/wake rows receive actual frozen revalidation references; incomplete rows remain unpromoted. Darwin becomes an explicit gap, not done. Matrix counts are recomputed from all 686 rows.

Changed Rust was formatted; changed TS oracles were formatted using the frozen Biome executable without modifying frozen files. Manual readability review found substantial remaining size in supervisor/main.rs and the multi-operation test oracle; no automated readability or broad lint pass is claimed. Per request there was no independent delegation gate. Whole-slice completion remains open for the dependencies and residual proof boundaries above.
