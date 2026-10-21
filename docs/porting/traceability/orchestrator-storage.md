# Orchestrator storage

Frozen: `packages/orchestrator/src/storage.ts` at [common pin](../source-lock.json).

| Surface | Rust | Proof |
| --- | --- | --- |
| Whole module | `crates/pi-orchestrator/src/storage.rs` | `differential::frozen_protocol_storage_bidirectional_file_bytes` |

Malformed file and I/O failures propagate; additional failure fixtures pending.

Status: initial focused proof; complete branch matrix pending. Coverage remains todo until final evidence.

`tests/errors.rs` differentials reproduce native diagnostic mismatches first: directory read, invalid parent, mkdir-through-file, rm-directory, missing/non-executable/directory executable, absent/file/NUL cwd, and closed child stdin. Shared pi-js-compat Node FS/spawn/pipe-write formatters now retain originating syscall/errno; no JS stacks fabricated. Stdin pipe errors also reach the approved fatal cleanup channel.

Additional real-filesystem differentials cover EACCES read/open/rm and ENOSPC via sandbox /dev/full. RED distinguishes write from open and fs.rmSync permission diagnostics; File::create/write_all now preserve the observed failure boundary. Only private sandbox permissions/dev-full error sink are used.
