# Agent-core filesystem metadata traceability

Pinned source: `packages/agent/src/harness/types.ts` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

This bounded slice ports only the dependency-free `FileKind` and `FileInfo` value contracts. Focused Rust proof is complete; pinned type and binary64 differential proof remains pending. Filesystem adapters and their effectful branches remain deferred.

| TypeScript source | Observable contract | Rust proof or gap |
|---|---|---|
| `types.ts:133-134` | `FileKind` is the closed union `"file" | "directory" | "symlink"` | `FileKind::{File, Directory, Symlink}`, exact `as_str()` assertions, and public re-export test in `file_info.rs` |
| `types.ts:213-224` | Mutable required fields occur in source order: `name`, `path`, `kind`, `size`, `mtimeMs` | `FileInfo` exposes public `String`, `FileKind`, `f64`, and `mtime_ms: f64` fields in the corresponding Rust representation order; focused construction and mutation assertions |
| `index.ts:125-126` | `FileInfo` and `FileKind` are public type-only package exports | `pi_agent_core::harness::{FileInfo, FileKind}` imports compile in the focused test |
| `types.ts:260-262` | `FileSystem.fileInfo` and `listDir` consume the metadata values | Consumer mapping only; filesystem traits and implementations remain deferred |
| `skills.ts:68-95,128-174,328-347` | Skill discovery reads `FileInfo.path`, `name`, and `kind`, and resolves symlinks through the environment | Consumer mapping only; skills and filesystem integration remain deferred |
| `prompt-templates.ts:40-58,168-187` | Prompt-template discovery reads `FileInfo.path`, `name`, and `kind` | Consumer mapping only; prompt-template and filesystem integration remain deferred |
| `env/nodejs.ts:67-91` | `fileKindFromStats` checks file, directory, symlink in order; `fileInfoFromStats` copies basename/path/kind/size/mtimeMs or returns an invalid-type error | Deferred Node adapter branch; no filesystem implementation in this slice |
| `env/nodejs.ts:602-607` | `fileInfo` resolves and lstat's a path, then maps backend failures to `FileError` | Deferred async filesystem and error-production behavior |
| `env/nodejs.ts:611-639` | `listDir` creates metadata values from directory entries and preserves cancellation/error behavior | Deferred directory listing, ordering, cancellation, and backend mapping |

## Numeric boundary

Source `size` and `mtimeMs` are JavaScript `number` fields with no validation. Rust uses direct `f64` fields and focused tests compare `to_bits()` for signed zero, subnormals, fractions, signed unit values, unsafe-integer boundaries, finite extremes, infinities, and canonical, payload, signaling, and negative NaNs. No integer, nonnegative, finite, or normalization policy is added.

## Explicit gaps

- Pinned proof for erased TypeScript types requires a strict generated type fixture; no runtime source constructor exists for `FileInfo`.
- Binary64 differential proof and source public-index comparison remain the next slice.
- JavaScript property descriptors, serialization, prototype identity, and runtime type erasure are not modeled by the typed Rust value API.
