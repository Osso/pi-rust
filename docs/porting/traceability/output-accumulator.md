# Output accumulator — current-pin prerequisite

Frozen `packages/coding-agent/src/core/tools/output-accumulator.ts` at `4f9e6f8a4`. This records the cleared temp-path prerequisite, not whole-module parity.

| Frozen branch | Native behavior | Proof |
|---|---|---|
| `defaultTempFilePath`, line21: `join(tmpdir(), ...)` | [Shared POSIX getter](../../specs/node-tmpdir.md) | Actual Node getter11; frozen consumer histories below |
| Constructor57–61 stores limits/prefix only; ensureTempFile217–219 computes the first path | Default path generated at first spill, not construction; existing native explicit-path constructor remains available | `differential_tmpdir_consumers::accumulator_reads_tmpdir_at_first_spill_not_construction`, four mutable TMPDIR/TMP/TEMP/trailing-slash cases |
| ensureTempFile215–219 guards `tempFilePath`, not the live stream | Keep first path after close; persisting another snapshot neither reopens nor truncates the file | Same histories close the real file, persist a second snapshot and compare full bytes/path |

Valid `tmpdir-accumulator-red-2.log` proves wrong constructor-time root and empty full-file bytes after post-close snapshot, against actual frozen output (`payload`). Initial `tmpdir-accumulator-red.log` was only a test import compile failure and is not behavioral evidence. Production correction follows the source predicate; no new queue, hook or fallback added. GREEN/gates follow the coherent commit.

Whole-module exported options/snapshots, numeric limits, raw-byte retention/decoder, append/finish errors, IO events and random suffix implementation still require their complete current-pin audit; this prerequisite does not close the coverage row.
