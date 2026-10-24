# idle-session-archive.ts

Common source [lock](../source-lock.json); [contract](../../specs/idle-session-archive.md). Native `core/session_archive_storage/idle.rs`, reexported by the existing `session_archive_storage` module; no parallel storage authority.

| Frozen symbol/branch | Native | Proof |
|---|---|---|
| archiveIdleSessionTranscripts / runIdleSessionArchive | blocking IO/compression task + native async background wrapper; same guard/result/error prefixes | real source/runtime startup RED with rejecting factory; GREEN/matrix pending |
| listProjectTranscripts | one project-directory level, regular `.jsonl`, deterministic native filename-byte order | pending file-layout matrix |
| isProcessPresent / readProtectedTranscriptPaths | signal0, listener/health protection, exact active-agent SQL | pending real DB/PID/reference matrix |
| archiveIdleTranscript / indexTranscript | cutoff/resident/preexisting archive, temp stream/comparison/index/recheck/permissions/times/final cleanup | pending content/stat/metadata matrix |
| recoverPublishedArchive | equal archive completes publication; changed plain wins and removes stale archive | pending interrupted publication matrix |
| canPublishArchivedTranscript | second references/DB/stat snapshot check | pending race/state boundary proof |
| readArchiveMatchesOriginal | two fixed64KiB buffers, streaming decompression, partial reads and final source EOF | pending unequal/prefix/trailing/corrupt cases |
| publishArchivedTranscript | hard-link publication, DB relocation, unlink; unarchive/remove on the frozen failure path | pending error/rollback matrix |
| runtime :699 | after detached cleanup, before factory await; detached archive not awaited | `idle-archive-startup-red-2.log`: actual source archives despite factory failure, native omitted task |

Fractional stat Date RED (`idle-archive-pass-matrix-3.log`): source copies ...0124ms for an original ...0123.75ms; native incorrectly truncated to ...0123ms. Node's shipped `internal/fs/utils` explicitly implements `dateFromMs` as `new Date(MathRound(Number(ms)))`; native now uses the existing JS Math.round helper when copying stat Dates. This preserves the filesystem API's explicit Date conversion, not cwd/cache/scheduling oddities.

Dependency: `detached_job_cleanup::read_linux_process_references` exposed pub(crate) unchanged by parent approval. Its existing flattened JsString error carrier remains deferred, not silently repaired or accepted as a new parity deviation here. Streaming/compression/protection errors still require bounded source differential evidence before module acceptance.
