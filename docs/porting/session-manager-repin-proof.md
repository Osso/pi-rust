# Session-manager common-pin proof

Scope: full `packages/coding-agent/src/core/session-manager.ts`, frozen `4f9e6f8a4`; no live providers, deployment, push, merge or child agents. [Module trace](traceability/session-manager.md). Whole-row audit/proof remains in progress.

## Proof ledger

| Revision / scope | Command | Evidence / result | Invalidation |
|---|---|---|---|
| Baseline `6b8d6aeda` + new fixture | `cargo test -j3 -p pi-coding-agent --test suite_session_1 repin_session_manager -- --nocapture` | `/tmp/session-manager-red.log`: fixture compile error (Debug bound), not RED | Fixture corrected |
| Same production + corrected fixture | same targeted command | `/tmp/session-manager-red2.log`: four actual frozen REDs: removed-file append, empty-DB sandbox, in-memory archived switch, EISDIR error record | Production corrections committed; GREEN pending |

The archived-switch test also checks the source assignment-before-guard state. Append errors retain appended in-memory entries and advance the leaf before the write fails. Existing realistic Node filesystem failures must keep original fields rather than Rust `io::Error` text. Exotic fd/OS faults remain excluded by the parity bar.
