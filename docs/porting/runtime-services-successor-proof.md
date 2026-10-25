# Runtime-services successor proof — 2026-10-25

Common pin: `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`, read-only. Worktree `/home/osso/.worktrees/pi-rust-repin-runtime-services`, branch `repin/runtime-services`. Proof follows the approved whole-module audit plus existing tests rule; no new frozen differential or matrix.

## Coverage

| Assigned module | Status / capability | Proof |
|---|---|---|
| [ensure-running](traceability/supervisor-ensure-running.md) | done: invocation, version/replacement, shared startup, real stale-lock coordination, detached spawn and deadlines | native5; existing frozen seven-case scenario and startup errors; new native mkdir RED→GREEN |
| [architect/main](traceability/architect.md#maints) | done: queue/settings/store, request completion/gates, cycle/stop/console/service wiring with parent AuthStorage/ModelRegistry | native18; existing frozen request-cycle and prompt; lossless4; new native renewal-error RED→GREEN at real 30s boundary |
| [resident-console-transport](traceability/resident-console-transport.md) | done: attach/probe, ownership, prompts/events, snapshot/identity, socket/client lifetime | transport13 + drift2 + command10; reciprocal frozen wire2; lossless2; four new native RED→GREEN regressions |
| [agent-lifecycle-trace](traceability/agent-lifecycle-trace.md) | done: readonly ownership/BFS/transcripts/outbox, stable sorting and complete formatter | whole audit; existing seven same-DB/JSONL frozen viewer cases in one passing test |
| [orchestrator supervisor](traceability/orchestrator-supervisor.md) | done: lifecycle/persistence, metadata refresh, RPC streams and Radius cleanup | existing supervisor10; enabled native lifecycle and failed-disconnect proof; reciprocal IPC retained |
| [orchestrator serve](traceability/orchestrator-serve.md) | done: startup, enabled Radius, ordered/idempotent shutdown, signal/fatal CLI mapping | enabled native real IPC/persistence/fake-child test; existing native/frozen CLI normal-signal and failure tests |

Each trace has the approved one-line malformed/adversarial/runtime gap. Native APIs retain existing lossless carriers and Result errors. No new dependency, fallback, source delta or cross-lane algorithm change. No old-only API was found in these six modules that required deletion.

## Changes and preserved source bugs

- `c85751d83`: resident snapshot precedes subscription events; remote disconnect closes the prompt API; EOF before snapshot gets the frozen message; file-as-parent fails as mkdir EEXIST.
- `2085e7ff8`: ensure-running mkdir failure preserves Node message/code/syscall/path and prevents launch. `0410ab54f` closes the unchanged lifecycle trace audit.
- `a27a40e22`: Architect renewal failure returns through existing cycle/service Result instead of disappearing in a detached task. The timer and pending prompt end with the failed cycle. No API widening/caller fallout.
- Preserved DISCREPANCY rows: frozen catalog lacks the required default Architect sol model; resident snapshot parsing accepts either attached/probed tag; Radius disconnect failure can remove an instance without disposing its child; serve cleanup short-circuits on rejection. The native failure test retains and explicitly disposes its own child after proving the source bug.

## Proof ledger

Logs are `/tmp/runtime-successor-<name>.log`. Tests run through the mandatory Cargo sandbox runner. Commands use `cargo test -j3 -p pi-coding-agent --test <target> <filter>` unless specified.

| Code scope | Command / result | Log names |
|---|---|---|
| `c85751d83` transport | Core4 `resident_console_`:25 PASS; Core2 `differential_resident_console::`:2 PASS; Core3 `lossless_resident_console::`:2 PASS | transport-final, transport-wire, transport-lossless |
| `2085e7ff8` ensure | standalone supervisor_ensure_running:5 PASS/one parent-invoked helper; Core5 supervisor_:3 PASS | ensure-native, ensure-scenarios |
| unchanged lifecycle reader at `0410ab54f` | Core1 differential_agent_lifecycle_trace:::1 PASS/seven cases | lifecycle-trace |
| `a27a40e22` architect | standalone architect:18 PASS; Core1 differential_architect_request_loop:::1 PASS; Core3 lossless_architect:4 PASS; existing prompt proof also PASS | architect-final, architect-cycle-final, architect-lossless, architect-cycle |
| `3f94affc7` plus final native test file | `cargo test -j3 -p pi-orchestrator --test runtime_services --test supervisor --test ipc --test lossless_ipc --test differential`:23 PASS; `--test cli_serve_process`:2 PASS/one parent-invoked helper | orchestrator, orchestrator-signals |

Merge `3f94affc7` incorporates parent `c52956b51`. No assigned production module changed in that merge; earlier focused proof remains valid. Workspace/all-target check passed after each production change and after the merge. Source inventory counts were recomputed after merging and each row closure; locked totals remain 686 rows / 186679 lines.

Warning-mode clippy passes for changed libraries and native test targets. No changed-line warnings; pre-existing dependency/test warnings remain (including duplicate oracle modules and unchanged Architect get-control-path helper). Manual changed-line readability audit completed. Final formatting, workspace check and inventory artifacts use `final-fmt`, `final-check`, `final-inventory`; no broad test suite, independent agent/model, push, deployment, SSH, live provider or external network was used.
