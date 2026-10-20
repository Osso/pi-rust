# Valid content-stream scratch preservation

Preserve valid runtime output properties from pinned `packages/ai/src/api/anthropic-messages.ts` and `packages/coding-agent/examples/extensions/custom-provider-anthropic/index.ts`. [Traceability](../porting/traceability/ai-types.md) records the bounded representation and producer proof.

## What it must do

- [x] Preserve optional numeric `index` on text, thinking, and tool-call content in complete event serde round trips, including the source example's `text_start` block with index 3.
- [x] Preserve optional tool `partialJson` verbatim; constructors leave both fields absent. Use the existing JS-number serialization convention for indices, without restricting them to integers.
- [x] Anthropic Messages start/delta snapshots retain indices and accumulated tool JSON; stop removes scratch before end snapshots.
- [x] Anthropic Messages errors before block stop remove scratch before error/result snapshots, retaining parsed arguments, text, thinking, usage, and source error ownership.
- [x] Compare full pinned-provider event content without filtering `index` or `partialJson`; retain documented timestamp and SDK identity proof boundaries.

## How it works

- [Shared content representation](../porting/traceability/ai-types.md)
- [Anthropic lifecycle and remaining proof boundaries](../porting/traceability/anthropic-messages.md)

## Implementation inventory

- `crates/pi-ai/src/types.rs`: optional content fields and constructor absence.
- `crates/pi-ai/src/api/anthropic_messages.rs`: source-matching scratch lifecycle.
- Existing workspace struct literals: mechanical `None` initialization only.

## Tests asserting this spec

- `crates/pi-ai/tests/types.rs`: complete valid-event serde round trips and absent-field defaults.
- `crates/pi-ai/tests/anthropic_messages.rs`: existing 27 adapter behavioral tests.
- `crates/pi-ai/tests/differential_anthropic_messages.rs`: 44 controlled-fetch/SSE source scenarios; three new open-block errors.
- `oracle/anthropic-messages-oracle.ts`, `crates/pi-ai/tests/support/anthropic.rs`: timestamp normalization only for event content; no scratch filtering.

## Known gaps (current cycle)

- [ ] Other providers' `customInput`, `streamIndex`, and `partialArgs` remain unrepresented; this slice does not prove their lifecycle.
- [ ] Full native custom Anthropic example lifecycle is independently owned by its example slice; shared serde preservation does not establish producer parity.

## Out of scope

Other backend scratch unions, live provider traffic, SDK/reader visibility, and changes to timestamp/error ownership. Prior scratch-normalized comparisons did not prove strict valid-event parity; valid output fields are not typed-input exclusions.
