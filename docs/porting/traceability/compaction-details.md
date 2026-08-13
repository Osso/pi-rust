# CompactionDetails traceability

Pinned source: `packages/agent/src/harness/compaction/compaction.ts` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| Source | Contract | Rust proof |
|---|---|---|
| `compaction.ts:30-37` | Mutable `readFiles: string[]` and `modifiedFiles: string[]` fields in source order | `harness::compaction::CompactionDetails` uses public `Vec<String>` fields `read_files` and `modified_files`; focused tests and eight differential groups `13156..=13163` |
| `index.ts:58-75` | Package index does not export `CompactionDetails` | No harness-root/package stand-in; direct-module type proof only |

## Boundary decisions

Rust strings preserve representable Unicode scalars, NUL, punctuation, empty values, order, duplicates, and embedded/trailing newlines. Invalid JavaScript runtime values and lone UTF-16 surrogates remain outside the typed Rust boundary.

The differential adapter reconstructs and compares final source-shaped records only. It does not call or reimplement file-operation extraction, session traversal, or compaction orchestration.

## Proof artifacts

- Focused RED: `/tmp/pi-rust-compaction-details-red.out`
- Focused GREEN: `/tmp/pi-rust-compaction-details-green.out`
- Differential GREEN: `/tmp/pi-rust-compaction-details-differential-green.out`
- Oracle/type proof: `/tmp/pi-rust-compaction-details-oracle.out`
