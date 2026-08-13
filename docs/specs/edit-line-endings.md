# Agent-core edit line-ending and BOM helpers

Agent-core ports the pure line-ending and leading-BOM helpers used by the edit tools from `packages/agent/src/harness/tools/edit-diff.ts`. This bounded contract excludes diff computation and edit orchestration. See the [edit line-ending traceability](../porting/traceability/edit-line-endings.md) for source mappings.

## What it must do

### Line-ending detection and conversion

- [x] Expose `LineEnding::{CrLf,Lf}` with exact `as_str()` literals through `pi_agent_core::harness::tools::edit_diff` only.
- [x] Detect CRLF only when the first CRLF begins before the first LF; treat missing LF and lone CR as LF.
- [x] Normalize every CRLF to LF, then every remaining CR to LF, preserving sequential quirks such as `\r\r\n`.
- [x] Restore LF unchanged for `LineEnding::Lf`; replace every LF with CRLF for `LineEnding::CrLf`, including doubling an existing CRLF.

### BOM extraction

- [x] Return public mutable `bom` and `text` fields in `StripBomResult`.
- [x] Remove exactly one leading U+FEFF when present; preserve BOM-only, repeated-leading, interior, empty, Unicode, supplementary, combining, NUL, and line-ending content exactly otherwise.

### Boundary and proof

- [x] Keep the helpers pure, standard-library-only, module-only, and independent from file I/O, diff generation, fuzzy matching, and edit orchestration.
- [ ] Compare the four helpers directly with the pinned TypeScript source through eight differential fixtures (`13081..=13088`) after the focused production slice.

## How it works

- [Edit line-ending traceability](../porting/traceability/edit-line-endings.md)

## Implementation inventory

- `crates/pi-agent-core/src/harness/tools/edit_diff.rs` — line-ending and BOM helpers.
- `crates/pi-agent-core/src/harness/tools/mod.rs` — module-only public boundary.
- `crates/pi-agent-core/tests/edit_line_endings.rs` — focused behavior proof.
- `crates/pi-agent-core/tests/differential_truncate/edit_line_endings.rs` — pending pinned differential fixtures.
- `oracle/truncate-oracle.ts` — pending persistent edit-helper oracle operation.
- `run-tests.sh` — focused test registration.

## Tests asserting this spec

- `crates/pi-agent-core/tests/edit_line_endings.rs`
- `crates/pi-agent-core/tests/differential_truncate.rs`
- `crates/pi-agent-core/tests/differential_truncate/edit_line_endings.rs`

## Known gaps (current cycle)

- [ ] Direct pinned differential proof and oracle operation remain pending at request IDs `13081..=13088`.

## Out of scope

- Fuzzy matching, Unicode normalization, replacement application, diff generation, edit validation, edit errors, file I/O, tool integration, and edit orchestration.
- Generic JavaScript runtime values and lone UTF-16 surrogates outside the typed Rust string boundary.
- New dependencies, fallback behavior, serde, constructors, and convenience wrappers.
