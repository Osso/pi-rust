# Edit tool traceability

Pinned source: `packages/agent/src/harness/tools/edit.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `crates/pi-agent-core/src/harness/tools/edit.rs`. Behavioral proof: `tests/harness_tools.rs` edit tests. Differential: 11 edit fixtures in `tests/differential_harness_tools/`. The replacement algorithm is documented in [edit-diff](edit-diff.md).

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `edit.ts:16-35` | Nested TypeBox schema and exact descriptions | `edit_schema`; schema test and differential comparison |
| `edit.ts:37-38` | `EditToolInput` plus legacy `oldText`/`newText` fields | `prepare_edit_arguments`; legacy/JSON-string test |
| `edit.ts:40-44` | `EditToolDetails { diff, patch, firstChangedLine? }` | details JSON map; disjoint-edit test and differential fixture |
| `edit.ts:46-66` | Parse a JSON-string `edits`, append legacy fields, drop legacy keys | `prepare_edit_arguments`; legacy/JSON-string test |
| `edit.ts:68-73` | Empty/missing `edits` error | `parse_edit_input`; differential fixtures use prepared input |
| `edit.ts:75-77` | Access error text with `FileError.code` | `edit_access_error`; edit missing-path behavior |
| `edit.ts:79-83` | Description text | `DESCRIPTION`; schema test |
| `edit.ts:84-93` | Resolve path and enter the mutation queue | `execute_edit`/`FileMutationQueue::run`; symlink serialization test |
| `edit.ts:95-101` | Pre-abort, file info, non-file rejection | `execute_edit`; pre-abort test |
| `edit.ts:103-108` | Read, pre-abort, BOM/line-ending normalization | `execute_edit`/`strip_bom`/`detect_line_ending`/`normalize_to_lf`; BOM+CRLF test and differential fixture |
| `edit.ts:109-113` | Apply edits, pre-abort, restore BOM/endings, write | `execute_edit`; fuzzy, overlap, missing, duplicate, no-change tests and differential fixtures |
| `edit.ts:114-124` | Diff string, unified patch, first changed line, success text | `generate_diff_string`/`generate_unified_patch`; disjoint-edit test and differential fixture |

## Rust boundary

- `firstChangedLine` is omitted from details when absent, matching `undefined`.
- `edits` are typed records; malformed non-string fields are rejected with a `ToolError` instead of JavaScript coercion.
