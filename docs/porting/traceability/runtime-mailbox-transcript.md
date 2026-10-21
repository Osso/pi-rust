# runtime-mailbox-transcript.ts

Frozen source: [4f9e6f8a4](../source-lock.json). Rust: `core/runtime_mailbox_transcript.rs`.

| Export / branch | Rust / proof |
|---|---|
| `RUNTIME_MAILBOX_ACCEPTANCE_CUSTOM_TYPE` | Same string; recovery custom-entry fixture. |
| `mailboxStoreRefsEqual` | Typed field equality; recovery compares raw persisted fields without eager validation. |
| `readAcceptedRuntimeMailboxStoreRefs` | Whole transcript, source ordering, complete LF records only, messages and custom receipts, no active-branch restriction. `repin_runtime_mailbox_transcript` compares actual TS helper against raw `[{}]`, null record error, partial final valid JSON, malformed complete JSON, inactive-branch refs and custom refs. |
| Undefined/empty/missing file | Empty refs; missing file does not synthesize acceptance. |
| Other read/parse errors | Propagated before DB recovery mutation. No eager schema validation of persisted refs. |

DB recovery tests use an owned TS oracle process to create and claim canonical rows, kill that fixture owner, then replace registration with Rust. Complete receipt removes dead claim rather than requeue. This is not proof that runtime append/fsync callers persist receipts crash-safely; those caller boundaries remain separate work.
