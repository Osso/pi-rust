# idle-session-archive.ts

Whole-module realistic-input correspondence renewed by storage-3 literal audit2026-10-25 of both public exports and all private branches below; existing focused proof remains applicable because this module is unchanged. Global integration acceptance is separate.

Common source [lock](../source-lock.json); [contract](../../specs/idle-session-archive.md). Native `core/session_archive_storage/idle.rs`, reexported by existing archive storage; no parallel storage authority. Frozen source stays unchanged.

## Capability/proof matrix

| Frozen symbol/branch | Native behavior | Proof level |
|---|---|---|
| `archiveIdleSessionTranscripts` :48-69 | age/current/protected guards, per-file errors, ordered archived paths | real frozen differential:14 pass/layout cases,19 live-DB/rollback cases,7 comparison/error cases |
| `runIdleSessionArchive` :71-78 | detached task; exact per-file/top-level console prefixes | literal branch audit; actual runtime startup differential below proves detached invocation, not every console failure |
| `listProjectTranscripts` :80-89 | one project level; regular `.jsonl` only; native filename-byte ordering before Node UTF-8 decoding | real differential preserves root-level files, nested sessions, uppercase suffix, symlink project/file and `.jsonl` directory |
| `isProcessPresent` / `readProtectedTranscriptPaths` :91-120 | signal0; asserted listener path, live/dead health PID; active stores, child paths, owner identity |19-case differential: current actual PID, absent PID2147483647, null/empty/unasserted paths, missing/malformed lifecycle, completed/failed/aborted, separate active child, null/empty/nonempty owner identity, SQL-trigger rollback |
| `archiveIdleTranscript` / `indexTranscript` :122-146,185-199 | cutoff inclusive; resident metadata/header; streaming compression; exact bytes/EOF; optional index; temp cleanup; mode/Date times | differential: old/fresh/cutoff/cutoff-minus1, fractional time, metadata/header supervisor, live open descriptor, large valid transcript, invalid header; compressed bytes, decoded bytes, metadata, mode, mtime and no temps asserted |
| `recoverPublishedArchive` :149-165 | matching archive completes relocation/unlink before age guard; changed surviving plain supersedes snapshot/unarchives | real differential: unindexed matching pair, indexed fresh pair, original-longer/archive-longer/equal-length mismatch, large multi-buffer matching/mismatch |
| `canPublishArchivedTranscript` :167-183 | reread references/DB and compare dev/ino/size/mtime/ctime | real native filesystem/SQLite boundary tests:7 file mutations and4 late DB guards; source branch audit. Tests call publication recheck after snapshot mutation, not a timing-dependent whole-pass race |
| `readArchiveMatchesOriginal` :202-223 | bounded streaming, partial-read loop, byte comparison and original EOF | source differential above; archive directory EISDIR/read; malformed frame; late truncated frame process proof below |
| `publishArchivedTranscript` :225-240 | no-overwrite hard link; archive DB relocation then source unlink; cleanup/unarchive on failure | full-pass real frozen/native SQL-trigger relocation failure; native real-filesystem EEXIST collision and EISDIR unlink rollback, checking surviving files and metadata. Rare syscall races are branch-level, not whole-pass scheduling proof |
| runtime creation :699 | after detached cleanup, before factory await; archive task not awaited | actual pinned runtime factory-rejection RED→GREEN: source/native archive old data and metadata despite factory rejection |

No old-only test deleted in this slice. No timing emulation or added dependencies. Final integration gates belong to main; this module evidence does not imply whole runtime acceptance.

## Discrepancies and approved gaps

- **DISCREPANCY, preserved literally** (parent classification2026-10-23): frozen :205 composes a decompressor without its own error listener. Late decoded-stream truncation emits uncaught `Error: unexpected end of file`, `code=Z_BUF_ERROR`, `errno=-5`, exits1. Native calls shared `throw_native_uncaught`, not per-file Result. Child-process differential compares status1/first Error line and verifies native error fields; V8 event preamble/stack omitted under approved stack gap.
- **Gap, runtime oddity** (parent classification2026-10-23): early ten-byte truncation is schedule-dependent: source exits1 in `idle-archive-read-boundaries-red.log`, but catches per-file/exits0 for the identical input in `idle-archive-truncated-process-green-attempt-2.log`. Native catches errors before decoded output; no scheduling emulation.
- Gap: non-Linux process-reference discovery, exceptional `/proc`/filesystem faults and cross-producer scheduling follow current platform/OS/runtime exclusions; no new fault carrier or scheduling emulation is required.
- Top-level rejection/console prefixes and rare chmod/utimes/cleanup failures have literal branch audit, not exhaustive fault-injection differential. Current publication race proof covers actual snapshots/DB/filesystems without claiming cross-producer scheduling parity.

## RED→GREEN and proof ledger

Artifacts `/tmp/pi-rust-repin-lossless-interactive/`:

| Scope | Evidence | Revision covered |
|---|---|---|
| Actual startup | `idle-archive-startup-red-2.log`; `idle-archive-startup-green-attempt.log` | startup port3434ed80; integration regression also in `idle-archive-read-boundaries-green-attempt.log` |
| Fractional stat Date | `idle-archive-pass-matrix-3.log` RED; `idle-archive-pass-matrix-4.log` GREEN |3328438c; unchanged semantic path |
| Live DB/PID guards | `idle-archive-db-guards-2.log` |189271c3; extended relocation-trigger GREEN `idle-archive-relocation-rollback.log` |
| EISDIR/read error | `idle-archive-read-boundaries-red-2.log` RED; comparison case GREEN in `idle-archive-read-boundaries-green-attempt.log` |5dfd7d09; only later truncation policy changes intersect decoder |
| Late truncation | `idle-archive-truncated-process-red.log`; `idle-archive-truncated-process-green.log` |37b9fcaf: exact process boundary and native error record |
| Layout/full pass matrix | `idle-archive-layout-boundaries-green.log` |7d9ea9ed |
| Publication boundary | `idle-archive-publication-boundaries-green.log` |7d9ea9ed:4 tests,13 actual filesystem/DB fixture transitions |

## Checkpoint gates

Revision43649e0c includes parent1fdc9d2a. `cargo test -j4 -p pi-coding-agent --test idle_session_archive -- --nocapture`:5/5 GREEN (`idle-archive-final-pass.log`); `cargo test -j4 -p pi-coding-agent --lib core::session_archive_storage::idle::tests -- --nocapture`:4/4 GREEN (`idle-archive-final-publication.log`). No ignored cases. `cargo check -j4 --workspace --all-targets --keep-going --message-format=json`: exit0,0 errors/0 warnings (`idle-archive-final-workspace.jsonl`, `.log`). Scoped `cargo clippy -j4 -p pi-coding-agent --lib --test idle_session_archive --message-format=json`: exit0,0 owned diagnostics;536 existing dependency/other-module warnings remain explicit (`idle-archive-final-clippy{.jsonl,.log,-owned.json}`). No suppression or unrelated cleanup.

Strict source-config `tsgo` oracle typecheck and Biome pass (`idle-archive-{typecheck,biome-final}.log`); formatter before every commit. Manual Rust/TS changed-line readability audit completed; structural analyzer unavailable. Later edits only correct source-line citations/add this ledger, so existing behavior/check proof stays valid; main owns independent integration acceptance.

Fractional stat copying is grounded in shipped Node `internal/fs/utils` `dateFromMs`: `new Date(MathRound(Number(ms)))`; original ...0123.75ms becomes Date ...0124ms. Native uses existing JS Math.round when copying source atime/mtime, not truncation. This is explicit filesystem Date conversion, not runtime scheduling/cache emulation.
