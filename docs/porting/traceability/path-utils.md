# Tool path utilities traceability

Pinned source: `packages/agent/src/harness/tools/path-utils.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `crates/pi-agent-core/src/harness/tools/path_utils.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `path-utils.ts:4-9` | Replace Unicode spaces with ASCII space and strip one leading `@` | `normalize_tool_path`/`is_unicode_space`; used by read/write/edit differential fixtures |
| `path-utils.ts:11-13` | `resolveToolPath` = `getOrThrow(env.absolutePath(...))` | `resolve_tool_path` returning `Result<String, FileError>` |
| `path-utils.ts:15-27` | Read variants: resolved, AM/PM narrow-NBSP, NFD, curly apostrophe, NFD+curly apostrophe; first existing wins; otherwise resolved | `resolve_read_tool_path`/`replace_am_pm`/`replace_apostrophes`; read tests and differential fixtures |

## Rust boundary and discrepancies

- JavaScript regex replacements are implemented as character scans; outputs match the source for valid Unicode.
- NFD uses `unicode-normalization` (`=0.1.26`); this matches `String.prototype.normalize("NFD")` for representable scalars.
- The source builds a `Set` to deduplicate variants; Rust preserves first occurrence order with a vector.
