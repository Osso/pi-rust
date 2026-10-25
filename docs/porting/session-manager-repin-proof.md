# Session-manager common-pin proof

Scope: full `packages/coding-agent/src/core/session-manager.ts`, frozen `4f9e6f8a4`; no live providers, deployment, push, merge or child agents. [Module trace](traceability/session-manager.md). Whole-row audit/proof remains in progress.

## Proof ledger

| Revision / scope | Command | Evidence / result | Invalidation |
|---|---|---|---|
| Baseline `6b8d6aeda` + new fixture | `cargo test -j3 -p pi-coding-agent --test suite_session_1 repin_session_manager -- --nocapture` | `/tmp/session-manager-red.log`: fixture compile error (Debug bound), not RED | Fixture corrected |
| Same production + corrected fixture | same targeted command | `/tmp/session-manager-red2.log`: four actual frozen REDs: removed-file append, empty-DB sandbox, in-memory archived switch, EISDIR error record | Production corrections committed; GREEN pending |

Additional proof: `/tmp/session-manager-green1.log` at `e1e3aeebb` passes all four initial regressions. `/tmp/session-manager-batch2-red.log` proves the reciprocal lifecycle/forks (including exact write bytes) and catches initial exclusive-open EEXIST. `/tmp/session-manager-initial-green.log` at `21a9e4bb1` passes initial-write and existing-file fork errors. Initial recent-session fixture incorrectly assumed Stats truncates (its assertion against TS failed; not production RED). Node Stats rounds mtime before Date construction. `/tmp/session-manager-recent-corrected-red.log` is the actual RED against restored fractional-time code: Rust b vs frozen a at 0.1/0.4ms. Corrected proof includes 0.9/1.1ms rounding ties too. The correction sorts scandir names before a stable rounded-Date-millisecond sort; info fallback mtime uses the same Stats boundary.

`/tmp/session-manager-recent-final-green.log` passes both rounded-mtime ties at `010918d60`. Parent `de46a2789` merged (new SharedSessionManager/readonly facade APIs retained). `/tmp/session-manager-goal-red.log` proves source archive-aware goal lookup versus old native null; restored transcript-path resolution passes. `/tmp/session-manager-relocation-red.log` proves lost 0600 source permissions (native 0644); `/tmp/session-manager-copy-red.log` proves missing original copyfile path/dest metadata. The exclusive copy now retains permissions and original two-path errors. Control-DB errors from relocation/sandbox are mechanically propagated through the existing adapter rather than formatted to text.

`/tmp/session-manager-batch4.log`: all 12 new frozen-source tests pass on the current code, including actual v1/v2 migration rewrites, branch extraction with recreated labels, reciprocal all/none/selected forks, listing progress (including invalid files), exact session info and continue/open. The lifecycle fixture originally collided on fixed fork IDs in the same millisecond; each peer's native bytes are now saved while removing the filename between independent allocations. No assertion was loosened and no runtime fallback was added. Earlier lifecycle harness errors were fixture clock normalization defects, not production RED.

The archived-switch test also checks the source assignment-before-guard state. Append errors retain appended in-memory entries and advance the leaf before the write fails. Existing realistic Node filesystem failures must keep original fields rather than Rust `io::Error` text. Exotic fd/OS faults remain excluded by the parity bar.
