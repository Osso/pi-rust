# empty-session-cleanup.ts

Source common pin [source lock](../source-lock.json); [contract](../../specs/empty-session-cleanup.md). Native `core/empty_session_cleanup.rs`.

| Source symbol/branch | Native | Proof |
|---|---|---|
| isResidentSession | exact IDs + slash/backslash directory segments | supervisor/architect IDs and both separator patterns in10-case remove matrix; resident rows also preserved by sweep |
| hasMessageEntry | literal entry_type `message` | actual persisted user-message guard; raw unknown projection shares SessionEntry::entry_type (static mapping, no separate malformed-record claim) |
| removeAbandonedEmptySession | falsey input/target guards, resident/message exclusion, metadata before unlink |10 real native manager vs pinned-module cases: normal/no DB/no file/resolved same target/two IDs/two path patterns/message/missing file |
| sweepAbandonedEmptySessions | collect DB-selected absent/dead candidates first, then remove, return count |10 real SQLite rows: absent/dead/zero removed; live own PID/existing/residents/message/archived/subagent retained; exact count3 and remaining IDs match frozen |
| runtime :228 | after beforeInvalidate, before dispose; target retained | real persisted-empty runtime RED→GREEN; exact EISDIR fields/UTF-16 path+message prove metadata removed before failure and session remains live; then cleanup succeeds after fixture repair |
| main.ts :1063 | sweep before session selection/name validation unless standalone | actual frozen/native CLI RED: native retained orphan metadata while source swept it; native call committed before GREEN/standalone assertion |

Proof artifacts under `/tmp/pi-rust-repin-lossless-interactive/`: `runtime-empty-cleanup-{red,green-2}.log`, `empty-cleanup-module-matrix.log`, `runtime-empty-cleanup-unlink-error.log`, `runtime-empty-cleanup-regression.log` (28/28 plus ignored child explicitly invoked), `empty-cleanup-startup-red.log`.

No stringify/IO error flattening: ControlDbError converts through CallbackError; unlink uses Node native metadata. All test databases/paths and liveness identity checks are owned fixtures. Full module gates/updated startup GREEN remain pending; no whole runtime acceptance.
