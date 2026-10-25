# AI context estimation

Port frozen `packages/ai/src/utils/estimate.ts` at `4f9e6f8a4`. [Traceability](../porting/traceability/estimate.md) maps its exports and branches.

## What it must do

- [ ] Count UTF-16 units, round up at four units per token and assign each image 4800 units.
- [ ] Count assistant text, thinking and tool-call names plus JSON arguments; estimate user and tool-result content alike.
- [ ] Use the latest positive assistant usage in array order, skipping aborted/error responses; add estimated trailing messages. Use reported total tokens or the sum of input, output and cache components.
- [ ] Without usable usage, estimate all messages; context inputs also include the system prompt and complete enumerable tool definitions. Use JavaScript JSON number formatting when counting tool definitions.

## How it works

- [Module traceability](../porting/traceability/estimate.md).

## Implementation inventory

- `crates/pi-ai/src/utils/estimate.rs` — estimators.

## Tests asserting this spec

- `crates/pi-ai/tests/estimate.rs` — direct behavior.
- `crates/pi-ai/tests/differential_estimate.rs` — pinned-source comparisons, including ordinary numeric JSON Schema bounds/defaults.
- `crates/pi-ai/tests/tool_metadata.rs` — complete enumerable tool metadata.

## Known gaps (current cycle)

Focused current-revision proof is pending.

## Out of scope

The 2026-10-25 parity bar excludes exotic runtime values, numeric extremes and lone UTF-16 units beyond existing carriers. Existing lossless message, description, metadata and argument carriers remain in use.
