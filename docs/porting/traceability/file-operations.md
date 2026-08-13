# Agent-core file-operation formatting traceability

Pinned source: `packages/agent/src/harness/compaction/utils.ts:61-73` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust proof or gap |
|---|---|---|
| `utils.ts:61-67` | Build a read-files section only for a non-empty read list; join entries with LF and preserve raw values | `harness::compaction::utils::format_file_operations`; focused read-only, ordering, duplicate, empty-entry, Unicode/NUL/XML-sensitive, and newline cases in `tests/file_operations.rs`; direct differential proof pending |
| `utils.ts:68-70` | Build a modified-files section only for a non-empty modified list; join entries with LF and preserve raw values | `format_file_operations`; focused modified-only and raw-content cases; direct differential proof pending |
| `utils.ts:71-73` | Return empty output for two empty lists; otherwise prefix with two LF characters and separate sections with two LF characters | Focused empty and exact two-section layout cases; direct differential proof pending |

## Rust boundary

- The module-only API is `pi_agent_core::harness::compaction::utils::format_file_operations(&[String], &[String]) -> String`.
- No harness-root re-export, generic `AsRef<str>` form, owned-list parameter, path normalization, escaping, sorting, deduplication, or filesystem access is part of this slice.
- Rust strings preserve valid Unicode scalars, NUL, punctuation, XML-sensitive text, and embedded or trailing newline content. Lone UTF-16 surrogates remain outside the typed boundary.

## Proof status

- Focused RED: `/tmp/pi-rust-file-operations-red.out` records exit 101 with only the missing `harness::compaction` module/API.
- Focused GREEN: `/tmp/pi-rust-file-operations-green.out` records exit 0 with 7 passed and 0 failed.
- Direct differential proof remains pending across exactly seven reserved request groups (`13089..=13095`), covering empty output; read-only and modified-only sections; both-section separators; order and duplicates; empty, Unicode, NUL, punctuation, XML-sensitive, embedded-newline, trailing-newline, and larger-list values.

## Exclusions

File-operation accumulation, extraction, serialization, message types, sorting or computation, compaction algorithms, session mutation, filesystem access, path APIs, normalization, escaping, fallback behavior, and new dependencies remain deferred.
