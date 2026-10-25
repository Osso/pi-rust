# list-extensions traceability

Source: `packages/coding-agent/src/cli/list-extensions.ts` (113 lines, pinned revision in
`docs/porting/source-lock.json`).

Rust: `crates/pi-coding-agent-tools/src/cli/list_extensions.rs`; frozen facade path re-exports it. Main-approved2026-10-25 relocation keeps the implementation byte-identical, providing one authority for CLI and interactive consumers. Current common-pin revalidation: `display-list-extensions.log` passes4 tests, including four frozen inventories, empty/newline cases and raw lone-surrogate basenames.

No TypeScript test file covers this module. Coverage is a byte-level differential against the
pinned source: `crates/pi-coding-agent/tests/differential_list_extensions.rs` (4 inventory cases +
newline/no-extension checks).

## Exported symbols

| TS symbol | Rust | Tests |
| --- | --- | --- |
| `formatExtensionInventory` | `format_extension_inventory` | differential cases 0–3, `no_extensions_message` |
| `listExtensions` | `list_extensions` | `list_extensions_appends_the_console_log_newline` |

## Branches

| # | Branch | Test |
| --- | --- | --- |
| 1 | empty list → `Loaded extensions: none` | `no_extensions_message` |
| 2 | `isFirstPartyPath` (`<first-party:` … `>`) | differential case 1/3 (`<first-party:status>`, long first-party name) |
| 3 | first-party label strips the prefix/suffix; otherwise `basename(path)` | differential cases 1/3 |
| 4 | first-party scope `runtime`, source `built-in` | differential case 1 (`scope: project` + source `ignored` in the fixture are overridden) |
| 5 | non-first-party scope from `sourceInfo.scope`, source from `sourceInfo.source` | differential cases 1/2 |
| 6 | counts from `commands.size`, `tools.size`, `handlers.size` | differential cases 1/3 (0, 1, 4, 9, 12, 23, 100) |
| 7 | sort by `path.localeCompare` | differential case 2 (`z`/`Alpha`/`alpha`/`éclair`) |
| 8 | column widths from UTF-16 `length`, `padEnd` in UTF-16 code units | differential case 3 (CJK path/source) |
| 9 | every line is `trimEnd`-ed, including header/separator | differential cases 1–3 (last column padding removed) |

`Extension.handlers.size` is `ExtensionHandlers::len()` (count of event lists with at least one
handler), added in master commit `3e0b20e`. The differential fixture registers one handler per
event list, so the Rust and TypeScript counts are produced from the same data shape.

## Deviations

None. Basenames use the shared lossless Node POSIX path authority, including root/trailing-separator and lone-unit cases.
