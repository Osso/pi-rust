# tool path utilities traceability

## Agent-core tool path utilities

Pinned source: `packages/agent/src/harness/tools/path-utils.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `crates/pi-agent-core/src/harness/tools/path_utils.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `path-utils.ts:4-9` | Replace Unicode spaces with ASCII space and strip one leading `@` | `normalize_tool_path`/`is_unicode_space`; used by read/write/edit differential fixtures |
| `path-utils.ts:11-13` | `resolveToolPath` = `getOrThrow(env.absolutePath(...))` | `resolve_tool_path` returning `Result<String, FileError>` |
| `path-utils.ts:15-27` | Read variants: resolved, AM/PM narrow-NBSP, NFD, curly apostrophe, NFD+curly apostrophe; first existing wins; otherwise resolved | `resolve_read_tool_path`/`replace_am_pm`/`replace_apostrophes`; read tests and differential fixtures |

## Rust boundary and discrepancies

- JavaScript regex replacements are implemented as character scans; outputs match the source for valid Unicode.
- NFD uses `pi_agent_core::unicode::normalize_nfd` (`icu_normalizer = 2.3.0`), the workspace-wide ICU4X implementation; `differential_unicode_normalize` proves NFD against Node over Hangul, combining marks, compatibility characters, and emoji.
- The source builds a `Set` to deduplicate variants; Rust preserves first occurrence order with a vector.

## Coding-agent path utilities

Pinned source: `packages/coding-agent/src/core/tools/path-utils.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `pi_coding_agent::core::tools::path_utils`. Normalization and resolution delegate to the already-ported `utils::paths::{normalize_path, resolve_path}` with `{ normalizeUnicodeSpaces: true, stripAtPrefix: true }`, which is the exact option pair the source passes.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `path-utils.ts:5` | `NARROW_NO_BREAK_SPACE = U+202F` | `NARROW_NO_BREAK_SPACE`; screenshot fixtures |
| `path-utils.ts:7-9` | `tryMacOSScreenshotPath` rewrites ` AM.`/`am.`/`PM.`/`pm.` after a space to `\u202F` | `replace_am_pm` with an ASCII-only `([AaPp][Mm])` class (JavaScript `i` without `u` folds ASCII only); focused AM/PM tests and differential fixtures |
| `path-utils.ts:11-14` | `tryNFDVariant` applies `String.prototype.normalize("NFD")` | `to_nfd` delegates to `pi_agent_core::unicode::normalize_nfd` (ICU4X); focused NFD and NFD+curly tests and differential fixtures |
| `path-utils.ts:16-21` | `tryCurlyQuoteVariant` maps every U+0027 to U+2019 | `replace_with_curly_quote`; focused curly-quote tests |
| `path-utils.ts:22-29` | `fileExists` uses `accessSync(path, F_OK)` | `path_exists_sync` (`Path::exists`); used by the sync resolver |
| `path-utils.ts:31-38` | `pathExists` uses async `access` | `path_exists` (`tokio::fs::metadata`); focused and differential async cases |
| `path-utils.ts:40-46` | `expandPath` = `normalizePath` with Unicode spaces and `@` stripping | `expand_path`; focused tests plus differential `~`, `~/`, `~draft.md`, `@~draft.md`, NBSP, and untrimmed inputs |
| `path-utils.ts:48-50` | `resolveToCwd` = `resolvePath` with the same options | `resolve_to_cwd`; focused and differential absolute/relative/tilde/`..` cases |
| `path-utils.ts:52-84` | `resolveReadPath` returns the first existing candidate in order: resolved, AM/PM, NFD, curly, NFD+curly | `resolve_read_path` walks `read_path_variants` in the same order and deduplicates candidates; focused tests for every variant and the no-variant fallback |
| `path-utils.ts:86-118` | `resolveReadPathAsync` is the async form of the same order | `resolve_read_path_async`; focused and differential async cases |

Differential: `oracle/coding-agent-path-utils-oracle.ts` calls the real pinned module against a shared temp directory (request `27002`, 41 cases), comparing resolved strings and error text. The oracle inherits the test process environment, so both sides expand `~` with the same `HOME`.

## Deviations

- `pathExists` uses `metadata` rather than `access`. Both fail for missing, broken-symlink, and unsearchable paths; the difference is only that `metadata` also requires the target to be readable as metadata, which every resolvable path is.
- Rust `PathError` covers only the `file://` URL failure that `normalizePath` can raise; JavaScript's `ENOENT`/`ENOTDIR` variants do not apply because neither side touches the filesystem while resolving.
