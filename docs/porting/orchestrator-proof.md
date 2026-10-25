# Q3 orchestrator proof (2026-10-21)

Historical ledger. The 2026-10-25 [runtime-services successor proof](runtime-services-successor-proof.md) closes supervisor and serve under the approved lighter-proof rule; their linked traces supersede the old open-matrix notes below. Current project status is in [coverage](coverage.md).

All 13 frozen orchestrator source modules have Rust counterparts in `crates/pi-orchestrator`, a workspace member. Frozen package.json declares no bin; no new executable target is registered. The frozen package has no test directory at this pin. Tests below supply new runtime/filesystem/process differentials without changing reference bytes.

## Native decisions

- Library-only crate; native child command is sibling `pi --mode rpc`. A copied embedding executable and private fake sibling prove actual argv and cwd without live agents.
- Async serve returns Result after signal/fatal cleanup; CLI embedding returns exit code 0/1. JS uncaughtException/unhandledRejection map to propagated errors, not process-global panic hooks.
- Duplicate in-flight request IDs retain the original unresolved caller through disposal, matching the overwritten frozen resolver. Malformed child JSON and stdin errors use the normal fatal cleanup path.
- Frozen IPC coalesced-line stall is an approved source bug. [Locked delta](delta-lock.json) `f11463802d385787413b3c434dd71d171a58ac37` fixes blank prefixes and immediate buffered-frame draining. Rust preserves the frozen discrepancy assertion and has a separate byte-identical comparison to the verified delta module. Ordinary connections still end after one response.

## Capability matrix

| Module | Status | Implemented/proved capability |
| --- | --- | --- |
| [config](traceability/orchestrator-config.md) | done | native flag and frozen package version in differential config matrix |
| [types](traceability/orchestrator-types.md) | done | all five concrete serde wire values in foundation |
| [storage](traceability/orchestrator-storage.md) | done | absent, present, delete idempotence, corrupt JSON, TS→Rust→TS bytes: foundation/differential |
| [ipc-protocol](traceability/orchestrator-ipc-protocol.md) | done | JS number and integer-key order, LF framing, cast-only success and SyntaxError messages: foundation/differential + IPC errors |
| [ipc-client](traceability/orchestrator-ipc-client.md) | partial | TS client→Rust server and Rust client→frozen TS server; first JSONL response and fragment buffering: ipc.rs |
| [ipc-server](traceability/orchestrator-ipc-server.md) | partial | bidirectional IPC, live rejection, stale removal, bind and unlink error identity: ipc/errors |
| [rpc-process](traceability/orchestrator-rpc-process.md) | partial | native spawn method; sibling pi --mode rpc and cwd proved by copied embedding test with sandbox fake sibling |
| [handler](traceability/orchestrator-handler.md) | partial | frozen full child lifecycle differential; unknown status/stop/rpc/rpc_stream fixtures: supervisor |
| [supervisor](traceability/orchestrator-supervisor.md) | partial | native runtime shares one supervisor and weak Radius coordinator; spawn/list/status/stop persistence and frozen lifecycle differential |
| [radius](traceability/orchestrator-radius.md) | partial | URL/credential precedence native tests; actual fake-fetch frozen registration differential; coding-agent AuthStorage reused |
| [serve](traceability/orchestrator-serve.md) | partial | native start startup-failure cleanup, frozen startup logs and source SIGTERM exit-0 socket cleanup: supervisor |
| [cli](traceability/orchestrator-cli.md) | partial | actual frozen subprocess stdout/stderr/exit codes compared byte-for-byte: cli |
| [index](traceability/orchestrator-index.md) | done | native index.rs reexports config/handler/IPC/process/serve/storage/supervisor/types; root imports exercised by errors and process-boundary fixtures |

`partial` means implementation exists but the named branch proof in its traceability document is incomplete; it is not an unported whole module. No unconditional full-package parity claim is made.

## Final proof

Code revision: `db5c1a36` (later documentation-only commit does not invalidate code proof). All Cargo tests use the mandatory runner, `CARGO_BUILD_JOBS=4`, and this worktree's `target`.

| Command/scope | Result | Artifact |
| --- | --- | --- |
| Eight explicit pi-orchestrator test targets (foundation, differential, ipc, rpc_process, radius, supervisor, cli, errors) | 51 passed; zero failed/ignored | `/tmp/pi-rust-repin-orchestrator/final-tests.log` |
| pi-js-compat node_fs_errors target | 6 passed; zero failed | `/tmp/pi-rust-repin-orchestrator/shared-node-fs-tests.log` |
| cargo check --workspace --all-targets | exit 0, zero warnings | `/tmp/pi-rust-repin-orchestrator/final-check.log` |
| cargo fmt --all --check | exit 0 before each commit | final formatter artifact in report directory |
| 12 core mutants + 3 delta/orphan mutants | 15/15 killed by behavioral assertions, not compilation errors | `mutants.json`, `mutants-delta.json` in report directory |

Both ordinary TS-client→Rust-server and Rust-client→TS-server paths run in the sandbox. Storage TS-created fixtures are read/replaced by Rust and re-read by TS with exact file-byte checks. Radius uses fake Fetch/timers only; virtual deadlines and recovery request sequences are compared against frozen methods. Actual private filesystem errors compare read/open/mkdir/rm/write messages; actual fake-child errors compare spawn/stdin/exit text. Shared Node IO formatting is reused, including native/libuv syscall distinctions. No JS stacks are fabricated.

## Proof limits

- Handler success spawn/rpc_stream dispatch, exhaustive metadata-changing commands, Radius re-registration/disappearance failure cases, and direct native signal/CLI-serve embedding matrices remain explicitly partial.
- This is bounded conformance, not measured statement/branch coverage or exhaustive mutation coverage. Type-only TS records use concrete serde/storage shape proof rather than runtime imports of erased declarations.
- Automated readability/complexity executables are unavailable. Manual line/outline audit found small over-30-line dispatcher/retry/setup functions; recorded in the report, not claimed clean. No warning/lint suppressions were added.
- No clippy, broad workspace test suite, independent delegation, push, merge, deployment or live network/state was used, per the slice constraints.

## Verifier corrections (2026-10-21)

The prior IPC client/server and RPC process done claims are withdrawn: 5 done / 8 partial. Probe RED reproduced child-exit delay, disconnected-work cancellation, BOM framing rejection and successful Radius body errors. Fixes use child exit before reader EOF, a detached accepted-work queue, shared `pi_js_compat::string::js_trim`, and status-only success for maybePost. Renewed proof is recorded separately; the original ledger above is historical, not current acceptance.

Renewed focused proof at `a95ac55c`: nine tests across cli/frame_whitespace/radius_body/verifier_regressions pass; seven independent semantic mutants killed. Artifacts: `/tmp/pi-rust-repin-orchestrator-fixes/*-red.log`, `fix-regressions-green.log`, `mutants.json`, `mutant-*.log`. The 31-code-point matrix uses Node language semantics rather than importing each frozen module; frozen-module probes separately cover BOM server, disconnect completion, descendant-held pipes and Radius disconnect. Existing partial gaps remain open.

## Integrated verifier-fix acceptance (2026-10-21)

Master snapshots `166eac08` and then `65c7acce` (concurrent TUI integration) merged branch-locally as `0ca61cfd` and `11d2a7ad`. Each sole conflict was the coverage count header; every one of 686 rows verified as the three-way parent union. Frozen 13-module bytes and every locked delta HEAD/SHA256 verified; orchestrator production bytes unchanged by either merge. No source conflict, push or canonical merge.

| Current proof scope at 11d2a7ad | Result | Artifact under `/tmp/pi-rust-repin-orchestrator-fixes` |
| --- | --- | --- |
| `cargo test -p pi-orchestrator` | 56 passed, zero failed/ignored | latest-orchestrator.log |
| `cargo test -p pi-js-compat --test node_fs_errors` | 6 passed; scope unchanged by TUI merge, retained proof at 0ca61cfd | final-node-fs.log |
| `cargo check --workspace --all-targets` | exit0, zero warnings | latest-workspace-check.log |
| `cargo clippy -p pi-orchestrator -p pi-js-compat -j4 --all-targets -- -D warnings` | exit0, zero warnings | latest-clippy.log |
| `cargo fmt --all --check` | exit0 before final documentation commit | latest-handoff-fmt.log |

The TUI dependency changes invalidate earlier consumer proof, justifying the final test/check/clippy rerun. Shared node_fs scope is unchanged; its passing proof is retained. All commands use jobs4/offline/worktree target; test binaries use mandatory sandbox runner. Detailed command/revision/invalidation ledger: `proof-ledger.md` in that artifact directory. No independent verifier/delegation or network. Coverage remains 5 done / 8 partial; this does not close named parity gaps.
