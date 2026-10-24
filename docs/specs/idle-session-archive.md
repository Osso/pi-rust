# Idle session archive

Port frozen `idle-session-archive.ts` in the existing [archive storage](../porting/traceability/session-archive-storage.md) namespace. [Traceability](../porting/traceability/idle-session-archive.md).

## Requirements

- [x] Scan only regular `.jsonl` files one project directory below `<agentDir>/sessions`; protect live Linux process references, mailbox listeners, active stores/children/runtime owners, live health PIDs and the current session.
- [x] Archive files older than30 days, excluding resident metadata, with streaming compression and bounded-memory exact comparison including original EOF; index valid previously unindexed transcripts.
- [x] Recheck references and dev/ino/size/mtime/ctime before publication; preserve mode and source Date timestamp semantics. Publish by hard link, relocate/archive metadata, then unlink original; remove publication and unarchive metadata on the frozen failure paths.
- [x] Recover interrupted publication: matching pair completes DB relocation/source removal; surviving changed plain data supersedes archive and unarchives its metadata. Recovery applies before the age cutoff.
- [x] Public pass returns archived paths/errors. Fire-and-forget wrapper logs frozen prefixes and runtime creation starts it after detached cleanup, before awaiting the factory, unless standalone/no control DB.

## Proof status

Real pinned runtime-creation RED: the factory rejects with a fixture marker, yet source publishes the old transcript and archive metadata in background; native returned the same marker but never archived (`idle-archive-startup-red-2.log`). Implementation committed before focused GREEN and full publication/protection/recovery matrix.

## Boundaries

No parallel archive storage implementation or new dependencies. Existing exact zstd/Tokio/SQLite/Node error helpers are reused. Shared Linux reference-scan flattened error-carrier discrepancy is explicitly deferred under parent instruction; no behavior change to that helper. Runtime oddities follow [project rules](../../AGENTS.md). Broader manual archive restore/trust/restart remains4d.
