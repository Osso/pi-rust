# CompactionDetails traceability

Pinned source: `packages/agent/src/harness/compaction/compaction.ts` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| Source | Contract | Rust proof |
|---|---|---|
| `compaction.ts:30-35` | Mutable `readFiles: string[]` and `modifiedFiles: string[]` fields in source order | `harness::compaction::CompactionDetails` uses public `Vec<String>` fields `read_files` and `modified_files`; focused tests and eight differential groups `13156..=13163` |
| `index.ts:58-75` | Package index does not export `CompactionDetails` | No harness-root/package stand-in; direct-module type proof only |

## Boundary decisions

Rust strings preserve representable Unicode scalars, NUL, punctuation, empty values, order, duplicates, and embedded/trailing newlines. Invalid JavaScript runtime values and lone UTF-16 surrogates remain outside the typed Rust boundary.

The differential adapter reconstructs and compares final source-shaped records only. It does not call or reimplement file-operation extraction, session traversal, or compaction orchestration.

## Proof artifacts

- Focused RED: `/tmp/pi-rust-compaction-details-red.out`
- Focused final: `/tmp/pi-rust-compaction-details-final-focused.out`
- Differential final: `/tmp/pi-rust-compaction-details-final-differential.out`
- Oracle/type proof final: `/tmp/pi-rust-compaction-details-final-oracle.out`
- Inherited verification: at clean HEAD `859641f`, focused results were 2 passed and 0 failed, differential results were 1 passed and 0 failed, and oracle, formatting, and diff checks exited 0. Saved workspace results of 254 passed, 0 failed, and 2 ignored plus workspace check and all-feature Clippy with `-D warnings` remain valid. `OVERALL: PASS`.
