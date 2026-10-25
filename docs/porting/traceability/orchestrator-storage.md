# Orchestrator storage

Source: `packages/orchestrator/src/storage.ts` at the [common pin](../source-lock.json). Rust: `crates/pi-orchestrator/src/storage.rs`.
Status: **done**. [Q3 proof and native approvals](../orchestrator-proof.md).

| Export/branch | Behavioral proof |
| --- | --- |
| loadMachine/saveMachine/deleteMachine | absent, present, delete idempotence, corrupt JSON, TS→Rust→TS bytes: foundation/differential |
| loadInstances/saveInstances/getInstance/upsertInstance/removeInstance | empty/default, insert/update order, replace/remove and unknown fields: foundation/differential |
| read/open/mkdir/rm/write failures | errors.rs compares nine real FS failure messages to frozen, including private EACCES fixtures and /dev/full ENOSPC |

Proof is bounded to the listed representable runtime/serialization cases; no exhaustive platform or mutation-coverage percentage is claimed.

Storage-lane shared rm correction2026-10-25: frozen `storage.ts:27-32` uses existsSync then rmSync(machinePath), not unlinkSync. The existing `node_rm_error` adapter now delegates to the shared C++ rm error record: positive errno/syscall`rm` for filesystem failures, while nonrecursive EISDIR validation remains distinct. Missing-path/permission-message behavior is unchanged. [Source lock and cleanup proof](session-transcript-spool.md#cleanup-failure-precedence-2026-10-25). No orchestrator-specific code was changed.

Native UTF-16 integration: [lossless orchestrator proof](lossless-orchestrator.md).
