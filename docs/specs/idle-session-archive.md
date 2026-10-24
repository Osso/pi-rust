# Idle session archive

Port frozen `idle-session-archive.ts` in the existing [archive storage](../porting/traceability/session-archive-storage.md) namespace. [Traceability](../porting/traceability/idle-session-archive.md).

## Requirements

- [x] Scan only regular `.jsonl` files one project directory below `<agentDir>/sessions`; protect live Linux process references, mailbox listeners, active stores/children/runtime owners, live health PIDs and the current session.
- [x] Archive files older than30 days, excluding resident metadata, with streaming compression and bounded-memory exact comparison including original EOF; index valid previously unindexed transcripts.
- [x] Recheck references and dev/ino/size/mtime/ctime before publication; preserve mode and source Date timestamp semantics. Publish by hard link, relocate/archive metadata, then unlink original; remove publication and unarchive metadata on the frozen failure paths.
- [x] Recover interrupted publication: matching pair completes DB relocation/source removal; surviving changed plain data supersedes archive and unarchives its metadata. Recovery applies before the age cutoff.
- [x] Public pass returns archived paths/errors. Fire-and-forget wrapper logs frozen prefixes and runtime creation starts it after detached cleanup, before awaiting the factory, unless standalone/no control DB.

## Proof status

Real pinned runtime-creation RED→GREEN: factory rejection does not block background publication.40 pass/layout/live-DB/comparison differential cases plus4 native publication-boundary tests cover current capability; exact commands/revisions/artifacts in [traceability](../porting/traceability/idle-session-archive.md). Late decoded-stream truncation preserves frozen's uncaught Z_BUF_ERROR process failure; early schedule-dependent truncation is caught per file under approved runtime-oddity policy. Shared Linux-reference error carrier remains deferred; no whole-runtime acceptance inferred.

## Boundaries

No parallel archive storage implementation or new dependencies. Existing exact zstd/Tokio/SQLite/Node error helpers are reused. Shared Linux reference-scan flattened error-carrier discrepancy is explicitly deferred under parent instruction; no behavior change to that helper. Runtime oddities follow [project rules](../../AGENTS.md). Broader manual archive restore/trust/restart remains4d.
