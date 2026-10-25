# docs-tree-context traceability

Source: `packages/coding-agent/extensions/docs-tree-context/src/index.ts` (101 lines).
Rust: `crates/pi-coding-agent-extensions/src/bundled_extensions/docs_tree_context.rs`.
Tests: `crates/pi-coding-agent/tests/bundled_extensions_context.rs`.

| Source symbol / branch | Rust item | Proof |
| --- | --- | --- |
| `pathExists` (ENOENT false, others throw) | `path_exists` | differential docs/missing-docs steps |
| `toPortablePath` | `to_portable_path` | differential tree output |
| `collectDocsFiles` (excluded dirs, files only) | `collect_docs_files` | differential fixture tree |
| `buildDocsContext` (dedupe, sort, join) | `build_docs_context` | differential context string |
| `EXPLICIT_DOC_FILES` additions | same | differential fixture includes both |
| `insertAfterUserRules` (existing section, missing marker) | `insert_after_user_rules` | differential steps 2-4 |
| `session_start` / `before_agent_start` | factory handlers | differential |

Notes:
- Sort uses UTF-16 code-unit order, matching JavaScript's default `Array.sort`.
- Symlinks are skipped because `DirEntry::file_type` reports them as neither file nor
  directory, matching `Dirent.isFile()`.
- Relative paths use `utils::paths::relative_path` + portable separator.
