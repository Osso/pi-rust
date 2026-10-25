# Coding-agent list tools traceability

Pinned source: `packages/coding-agent/src/cli/list-tools.ts` at common pin `4f9e6f8a4`.

Rust: `crates/pi-coding-agent-tools/src/cli/list_tools.rs`; frozen facade path re-exports it. Main-approved2026-10-25 relocation keeps the implementation byte-identical, providing one authority for CLI and interactive consumers. Current common-pin revalidation: `display-list-tools.log` passes3 tests, including four frozen inventories, newline and raw split-surrogate retention.

Differential: `tests/differential_list_tools.rs` compares `formatToolInventory` byte-for-byte across four fixtures (`28500..=28503`): empty, sources/active flags, description truncation, and locale name ordering.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `list-tools.ts:9` | Description width 54 | Truncation fixture `28502` |
| `list-tools.ts:15-26` | Source labels: `extension:` verbatim, `sdk`, `builtin` for `<builtin:>` paths, else the raw source | Fixture `28501` |
| `list-tools.ts:28-33` | Truncate to 51 UTF-16 units, trim end, append `...` | Fixture `28502`, astral unit test |
| `list-tools.ts:35-68` | Header/dash/row widths, two-space separators, trailing-space trimming | All fixtures |
| `list-tools.ts:70-88` | Empty inventory message; sort by name with `localeCompare`; active set | Fixtures `28500..=28503` |
| `list-tools.ts:90-92` | `listTools` prints the inventory plus a newline | `list_tools_appends_the_console_log_newline` |

## Semantics

- Widths and truncation use UTF-16 code units like `String.prototype.length`/`slice`.
- A truncated astral character retains its lone UTF-16 unit in the returned JsString. Replacement occurs only at Node-compatible stdout encoding; the raw-return unit test and byte-level output fixtures cover those separate boundaries.

## Discrepancies

None found.
