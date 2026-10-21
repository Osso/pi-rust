# Control DB v15 slice proof

Branch: `port/ca-db-v15`, based on `6ffdddf`. Source behavior:
[authorized c248b0067 reference](source-lock.json), except unchanged
broad AgentSession/RPC differentials, which retain the general pin and exercise
name commands after JSONL snapshots. No output field/event is removed to force parity.

## Current relevant proof

| Scope | Revision | Command (all test invocations bounded by `timeout 600`) | Result |
|---|---|---|---|
| Core names/consumers | `0dc6267` | `cargo test -p pi-coding-agent --test session_control_db --test session_manager --test agent_session --test agent_session_runtime --test session_picker --test startup_session_name --test rpc_mode_session_name` | 163 passed |
| Autoname | `f9fba82` | `cargo test -p pi-coding-agent --test session_autoname` | 22 passed, including real 15-second cancellation |
| Shared UTF-16 conversion | `0dc6267` | `cargo test -p pi-js-compat --test string_utf16` | 1 passed |
| Cross-language v15 DB | `26a58e3` | `cargo test -p pi-coding-agent --test cross_implementation_control_db` | 4 passed |
| Other differentials | `0dc6267` | `cargo test -p pi-coding-agent --test differential_control_db --test differential_agent_session --test differential_rpc_mode`; `cargo test -p pi-coding-agent --test differential_session_tools resume_session_tool_differential` | 9 passed |
| CLI registration | `0dc6267` | `cargo test -p pi-coding-agent --test main_process extensions_command_lists_the_ported_first_party_extensions` | 1 passed |

Total: **200 passing tests**, including **43 added tests**. Later changes only affect
the explicitly replaced test scopes or documentation. Machine-readable command/revision/log
ledger: `target/ca-db-v15-proof.json` in the slice checkout.

Differential coverage: 74 DB operation steps, independently initialized full schema/PRAGMA
comparison, existing TS-v15 → Rust update → TS readback, concurrent writers, 12 generated-title
inputs through the real TS extension, and 13 by-name/path/id resume fixtures. All database
fixtures are isolated temporary files; live config/state was neither opened nor copied.

## Limits and discrepancies

- Approved intermediate UTF-16 helper difference: [exact counterexample and SQLite proof](traceability/session-autoname.md#approved-intermediate-string-gap-2026-10-20). Stored SQL names match U+FFFD exactly.
- Writer-lock migration-state recheck is implemented; its race branch has no deterministic process-contention test. Quiescence blocking/retry and PID error ordering have behavioral proof.
- Initial core migration/name-state tests were added after implementation; an initial RED run is not recorded. Subsequent PID-order, empty-file, whitespace/registration and split-surrogate storage regressions have explicit RED/GREEN evidence.
- No dependencies added. No broad lint/check gates or independent verifier spawned: main owns integration Clippy/check/acceptance per coordination instructions. Formatting passes before each commit.

Integration must preserve newer main-branch lifecycle/shutdown changes, expanded inventory
rows/totals, and telemetry approvals. Replace main's session-autoname TODO row with this
slice's row rather than retaining duplicate source rows.
