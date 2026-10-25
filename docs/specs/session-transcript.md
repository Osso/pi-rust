# Session transcript storage

Frozen target: 4f9e6f8a4. Reads resolve a missing `.jsonl` to its compressed sibling; an existing plain path wins. Inspection must never restore, replace, or migrate originals. Mutable archive opens fail before allocating an identity. Byte/text reads propagate decompression failures. Transcript path validation and filesystem failures preserve the frozen error constructor, message, code, errno, syscall and path through callers; the required native Read trait converts IO errors at the Node boundary, not by placing Node errors inside IO errors. Public truncated-zstd streaming behavior awaits source-bug classification; see [bounded evidence and open boundaries](../porting/traceability/session-storage-integration.md).

Whole storage group remains pending whole-module acceptance; focused integration evidence and remaining consumer gaps live in the linked ledger.

Archives validate decompression before publication, preserve mode, create exclusive temporary sibling, rename then remove original; cleanup on failure. Metadata relocation and archive flag update use the control DB atomic operation. DB failure invokes reverse file operation; reverse failure replaces original error as in frozen TS. Restore prefers surviving plain file over stale archive.

Seekable archive inspection provides private0600 temporary plaintext and removes temporary state after callback/error. SDK users require no external pi executable or package setup. Off-thread decoding has a90-second synchronous deadline and preserves frozen error text. The approved blocked-OS-call cancellation/process-liveness gap and actual frozen timeout probe are recorded in the [spool trace](../porting/traceability/session-transcript-spool.md#in-process-worker-boundary-2026-10-25).

Persisted message mailboxStoreRefs are optional raw JSON: preserve explicit null/empty/malformed values and omission. This serialization alone does not implement durable delivery.

HTML export resolves stale paths and opens read-only, including in-memory legacy migration. Current history search consumes full persisted active ancestry through history iterator, without changing search population/filtering beyond storage reachability.

Native optional-ref append uses `append_message_with_mailbox_store_refs(AgentMessage, Option<&[RuntimeMailboxStoreRef]>)`; None omits and Some(empty) stores[]. Loaded raw fields remain unvalidated. `persist_for_recovery` forces initial write; `fsync(&self)` syncs the writable recipient. Runtime still owns claim/CAS ordering and duplicate suppression.
