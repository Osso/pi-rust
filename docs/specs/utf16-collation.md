# Lossless UTF-16 collation

Shared `pi_js_compat::collator::locale_compare_utf16` reproduces host-locale Node `String.prototype.localeCompare` for raw JSON strings. [Traceability](../porting/traceability/utf16-collation.md).

## What it must do

- [x] Compare raw UTF-16 code units without replacing unpaired surrogates, including mixed valid/unpaired strings and valid surrogate pairs.
- [x] Match direct Node pair-order signs across high/low surrogates, replacement character, ASCII case/numeric IDs, emoji, and canonically equivalent accented strings.
- [x] Leave the existing UTF-8 collation backend unchanged; never select a lossy replacement backend on failure.

## How it works

- [API, prerequisites and proof](../porting/traceability/utf16-collation.md)
- [Pinned dependencies](../dependencies.md)

## Implementation inventory

- `crates/pi-js-compat/src/collator.rs` — safe ICU4C-backed UTF-16 entrypoint alongside the existing ICU4X UTF-8 entrypoints.

## Tests asserting this spec

- `crates/pi-js-compat/tests/image_generator_collation.rs` — 16 × 16 exact pair-order comparisons against Node.
- `oracle/image-generator-coercion-oracle.ts` — direct engine localeCompare signs.

## Known gaps (current cycle)

- [ ] Parent owns integrated gates and deployment-environment prerequisite validation.

## Out of scope

- Replacing existing UTF-8 collation callers/backend; installing native packages or changing host services.
