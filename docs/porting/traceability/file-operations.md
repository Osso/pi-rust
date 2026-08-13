# Agent-core file-operation formatting traceability

Pinned source: `packages/agent/src/harness/compaction/utils.ts:61-73` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust proof or gap |
|---|---|---|
| `utils.ts:61-67` | Build a read-files section only for a non-empty read list; join entries with LF and preserve raw values | `harness::compaction::utils::format_file_operations`; focused read-only, ordering, duplicate, empty-entry, Unicode/NUL/XML-sensitive, and newline cases in `tests/file_operations.rs`; direct differential groups `13089..=13095` |
| `utils.ts:68-70` | Build a modified-files section only for a non-empty modified list; join entries with LF and preserve raw values | `format_file_operations`; focused modified-only and raw-content cases; direct differential groups `13089..=13095` |
| `utils.ts:71-73` | Return empty output for two empty lists; otherwise prefix with two LF characters and separate sections with two LF characters | Focused empty and exact two-section layout cases; direct differential groups `13089..=13095` |

## Rust boundary

- The module-only API is `pi_agent_core::harness::compaction::utils::format_file_operations(&[String], &[String]) -> String`.
- No harness-root re-export, generic `AsRef<str>` form, owned-list parameter, path normalization, escaping, sorting, deduplication, or filesystem access is part of this slice.
- Rust strings preserve valid Unicode scalars, NUL, punctuation, XML-sensitive text, and embedded or trailing newline content. Lone UTF-16 surrogates remain outside the typed boundary.

## Proof status

- Focused RED: `/tmp/pi-rust-file-operations-red.out` records exit 101 with only the missing `harness::compaction` module/API.
- Focused GREEN: `/tmp/pi-rust-file-operations-green.out` records exit 0 with 7 passed and 0 failed; production commit `54c5010` implements the formatter.
- Differential RED: `/tmp/pi-rust-file-operations-differential-red.out` records all prior requests through `13088` passing before request `13089` reaches the unchanged oracle's unsupported operation and returns non-JSON.
- Differential GREEN: `/tmp/pi-rust-file-operations-differential-green.out` records exit 0 with one persistent test passing; exactly seven request groups (`13089..=13095`) cover empty output; read-only and modified-only sections; both-section separators; order and duplicates; empty, Unicode, NUL, punctuation, XML-sensitive, embedded-newline, trailing-newline, and larger-list values.
- Oracle typecheck: `/tmp/pi-rust-file-operations-oracle.out` records `./run-tests.sh oracle` exit 0 with silent stdout/stderr.
- Differential proof commit: `84603a4` (`test(agent): compare file operation formatting`).
- Readability follow-up: `d5c3eb0` (`test(agent): simplify file operation fixtures`) resolves the only Rust length finding; Rust re-audit and the TypeScript oracle audit report no in-scope violations.
- Final gates at `d5c3eb0`: 213 workspace tests passed with 0 failures and 2 ignored; workspace all-target check, all-target/all-feature Clippy with denied warnings, formatting, documentation audit, exact pinned source HEAD, clean worktrees, and commit-range diff checks passed.
- Inherited verification completed with `OVERALL: PASS` for the bounded slice at `ff2d0ee`.

## Exclusions

File-operation accumulation, extraction, serialization, message types, sorting or computation, compaction algorithms, session mutation, filesystem access, path APIs, normalization, escaping, fallback behavior, and new dependencies remain deferred.
