# Agent-core edit line-ending and BOM traceability

Pinned source: `packages/agent/src/harness/tools/edit-diff.ts:7-21,243-246` at `532f7d12cd19235081bd4d6e425bde185652d86f`. This bounded slice ports four pure module helpers only; fuzzy matching, edit application, and file operations remain deferred.

| TypeScript source | Observable behavior | Rust proof or gap |
|---|---|---|
| `edit-diff.ts:7-13` | `detectLineEnding` compares the first CRLF index with the first LF index; missing LF, missing CRLF, and lone CR return LF | `harness::tools::edit_diff::detect_line_ending`; focused detection tests in `tests/edit_line_endings.rs`; direct differential fixture groups `13081..=13088` through `edit_text_helpers` |
| `edit-diff.ts:15-17` | `normalizeToLF` replaces CRLF first, then every remaining CR, preserving repeated-sequence ordering | `normalize_to_lf`; focused normalization tests including `\r\r\n`; direct differential groups `13081..=13088` |
| `edit-diff.ts:19-21` | `restoreLineEndings` returns LF text unchanged and blindly replaces every LF for CRLF, including existing CRLF doubling | `restore_line_endings`; focused restoration tests; direct differential groups `13081..=13088` |
| `edit-diff.ts:243-246` | `stripBom` removes exactly one leading U+FEFF and returns the anonymous structural object `{ bom: string; text: string }`; absent, repeated, interior, and BOM-only values remain source-compatible | Named Rust boundary `StripBomResult` preserves public mutable `bom`/`text` fields; focused BOM and string-boundary tests; direct differential groups `13081..=13088` |

## Rust boundary and source quirks

- Public functions are exposed only through `pi_agent_core::harness::tools::edit_diff`; no `harness` or `harness::tools` root re-exports are added.
- `LineEnding` preserves exact source literals `"\r\n"` and `"\n"` and derives only `Debug`, `Clone`, `Copy`, `PartialEq`, and `Eq`.
- `StripBomResult` has public mutable `bom` and `text` fields and derives only `Debug`; no constructor, serde support, or extra traits are added.
- Restoration intentionally does not normalize existing CRLF before replacing LF, so `"a\r\nb"` with CRLF restoration becomes `"a\r\r\nb"`.
- BOM extraction removes one Unicode scalar U+FEFF only; it does not remove interior or subsequent BOMs.
- Rust strings preserve valid Unicode scalars, supplementary characters, combining marks, NUL, and line-ending bytes. Lone UTF-16 surrogates remain outside the typed boundary.

## Proof status

- Focused RED: `/tmp/pi-rust-edit-line-endings-red.out` records exit 101 with only the missing `harness::tools::edit_diff` import.
- Focused GREEN: `/tmp/pi-rust-edit-line-endings-green.out` records exit 0 with 6 passed and 0 failed; production commit `5f0ee4e` implements the four module-only helpers.
- Differential RED: `/tmp/pi-rust-edit-line-endings-differential-red.out` records exit 101 after existing requests through `13080`; request `13081` reaches the unchanged oracle's unsupported operation and fails only because its non-JSON response cannot be parsed.
- Differential GREEN: `/tmp/pi-rust-edit-line-endings-differential-green.out` records exit 0 with one persistent test passing; exactly eight request groups (`13081..=13088`) compare detection, normalization, restoration, and BOM extraction.
- Oracle typecheck: `/tmp/pi-rust-edit-line-endings-oracle.out` records `./run-tests.sh oracle` exit 0 with silent stdout/stderr.
