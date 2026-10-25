# Session manager persistence parity

Session management follows `packages/coding-agent/src/core/session-manager.ts` at the [common pin](../porting/source-lock.json). It manages append-only JSONL trees, active context, forks and resume discovery. [Branch and proof map](../porting/traceability/session-manager.md).

## What it must do

- [x] Read TypeScript-created sessions and let TypeScript read native-created and appended records. Preserve entry order, unknown fields, omission versus null, UTF-16 carriers and frozen write bytes. Only independently allocated IDs/clocks differ; native assistant key order follows the existing approved exception.
- [x] Defer initial persistence until an assistant arrives, unless recovery persistence is requested. Use exclusive initial creation; a later append recreates a removed file without inventing a new header. Advance in-memory entry/leaf state before a failed append, as frozen does.
- [x] Preserve branch, label, compaction and context semantics. Extracted branches rechain entries after removing label records and recreate resolved labels. Fork all, none or a selected path; keep the source parent path.
- [x] Migrate legacy files in frozen mutation/write order; read-only inspection does not rewrite them. Retain the approved v1 generated-ID collision-set fix.
- [x] Match open/continue/list/session-info behavior, plain-only resume discovery, progress counts, rounded filesystem mtime dates, metadata-authoritative names and settings, archive-aware cross-session goals, and source-visible errors. Relocation preserves bytes and file mode; errors retain their original path/destination fields.

## How it works

- [Whole-module traceability](../porting/traceability/session-manager.md).
- [Exact command/revision ledger](../porting/session-manager-repin-proof.md).
- [Lossless record contract](lossless-content.md).

## Implementation inventory

- `crates/pi-coding-agent-storage/src/core/session_manager.rs` — manager, record/context projections, migration, persistence and listing.
- `crates/pi-coding-agent-storage/src/core/readonly_session_manager.rs` — existing isolated read facade and explicit goal mutators; separate coverage row.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/repin_session_manager.rs` — current frozen reciprocal lifecycle, migrations, discovery, errors and relocation.
- `crates/pi-coding-agent/tests/session_manager.rs` and `differential_session_manager.rs` — traversal, metadata, settings, caps, writes and source comparisons.
- `crates/pi-coding-agent/tests/session_manager_truthiness.rs`, `lossless_session_manager_io.rs` and `repin_session_storage.rs` — truthiness, raw UTF-16, archive/stream/inactive-history boundaries.
- `record_identity_tests` in the manager module — locked v1 collision delta versus frozen discrepancy.

## Known gaps (current cycle)

No open realistic-input module item. Final merge-train integration acceptance belongs to the parent.

## Out of scope

Adversarial/wrong-typed records, exceptional OS/runtime faults, numeric extremes and scheduling interleavings follow the approved parity exclusions. Existing lossless carriers remain required. Other modules' algorithms and provider execution are not changed by this row.
