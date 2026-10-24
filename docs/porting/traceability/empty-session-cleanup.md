# empty-session-cleanup.ts

Source common pin [source lock](../source-lock.json); [contract](../../specs/empty-session-cleanup.md). Native `core/empty_session_cleanup.rs`.

| Source symbol/branch | Native | Proof |
|---|---|---|
| isResidentSession | exact IDs + slash/backslash directory segments | module matrix pending |
| hasMessageEntry | literal entry_type `message`, including unknown typed message records | module matrix pending |
| removeAbandonedEmptySession | falsey input/target guards, resident/message exclusion, metadata before unlink | actual frozen/runtime teardown RED (`runtime-empty-cleanup-red.log`); GREEN and branch matrix pending |
| sweepAbandonedEmptySessions | collect DB-selected absent/dead candidates first, then remove, return count | module matrix pending |
| runtime :228 | between beforeInvalidate and dispose; original target path retained | same persisted-empty runtime RED; cleanup failure/disposal boundary pending |

No stringify/IO error flattening: ControlDbError converts through CallbackError; unlink uses Node native metadata. Source and native operate only owned fixture databases/paths in tests. No whole runtime acceptance.
