# Session-manager common-pin proof

Scope: full `packages/coding-agent/src/core/session-manager.ts`, frozen `4f9e6f8a4`; no live providers, deployment, push, merge or child agents. [Module trace](traceability/session-manager.md). Whole-row audit/proof remains in progress.

## Proof ledger

| Revision / scope | Command | Evidence / result | Invalidation |
|---|---|---|---|
| Baseline `6b8d6aeda` + new fixture | `cargo test -j3 -p pi-coding-agent --test suite_session_1 repin_session_manager -- --nocapture` | `/tmp/session-manager-red.log`: fixture compile error (Debug bound), not RED | Fixture corrected |
| Same production + corrected fixture | same targeted command | `/tmp/session-manager-red2.log`: four actual frozen REDs: removed-file append, empty-DB sandbox, in-memory archived switch, EISDIR error record | Production corrections committed; GREEN pending |

Additional proof: `/tmp/session-manager-green1.log` at `e1e3aeebb` passes all four initial regressions. `/tmp/session-manager-batch2-red.log` proves the reciprocal lifecycle/forks (including exact write bytes) and catches initial exclusive-open EEXIST. `/tmp/session-manager-initial-green.log` at `21a9e4bb1` passes initial-write and existing-file fork errors. Initial recent-session fixture incorrectly assumed Stats truncates (its assertion against TS failed; not production RED). Node Stats rounds mtime before Date construction. `/tmp/session-manager-recent-corrected-red.log` is the actual RED against restored fractional-time code: Rust b vs frozen a at 0.1/0.4ms. Corrected proof includes 0.9/1.1ms rounding ties too. The correction sorts scandir names before a stable rounded-Date-millisecond sort; info fallback mtime uses the same Stats boundary.

The archived-switch test also checks the source assignment-before-guard state. Append errors retain appended in-memory entries and advance the leaf before the write fails. Existing realistic Node filesystem failures must keep original fields rather than Rust `io::Error` text. Exotic fd/OS faults remain excluded by the parity bar.
