# Session archive and deletion

Frozen contract: `4f9e6f8a4`, `session-delete.ts` and `extensions/session-archive/src/index.ts`.

- `/archive`, `/delete`, `/unarchive` accept no arguments; require a control DB and persisted current session, with source notification strings.
- Unarchive changes metadata immediately, reporting already-unarchived sessions.
- Archive and confirmed delete request shutdown; only quit teardown executes the captured action, once, after final writes. Other shutdown reasons discard it. Failures print to stderr after TUI shutdown.
- Delete refuses active child agents with transcript paths; confirmation includes child sessions.
- Tree deletion enumerates descendants before root, batches by 64 KiB UTF-8 argv budget, invokes `trash --`, then unlinks files still present. Partial deletion cleans only removed transcripts’ metadata, including already-missing files. Unrelated sessions remain intact.

Proof and remaining limits: [archive trace](../porting/traceability/session-archive.md), [delete trace](../porting/traceability/session-delete.md).
