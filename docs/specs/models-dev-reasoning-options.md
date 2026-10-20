# models.dev reasoning option map

Expose the effort-map contract of pinned `packages/ai/scripts/models-dev-reasoning-options.ts` through `pi_ai::models_dev_reasoning_options`. Source pin, branch mapping and recorded proof: [module traceability](../porting/traceability/models-dev-reasoning-options.md).

## What it must do

- [x] Expose `ModelsDevReasoningOption`, `ModelsDevEffort`, and `get_effort_thinking_level_map`, using existing thinking-level/map types.
- [x] Represent toggle, effort and budget-token options. Effort accepts `none`, `minimal`, `low`, `medium`, `high`, `xhigh`, `max`, `default` and JSON null; budget bounds are optional numbers.
- [x] Combine effort lists regardless of order or duplicates, ignoring toggle/budget controls. Return `None` when no selectable effort exists, including empty, default-only and null-only inputs.
- [x] Emit maps in `off`, `minimal`, `low`, `medium`, `high`, `xhigh`, `max` order. Unsupported values are explicit null; `off` is `"none"` only when supplied. Never include `ultra`.

Independent proof at `222ab0f`: four public tests and 1,110 exact-source differential cases passed; see the linked ledger.

## How it works

- [Export/branch coverage and proof ledger](../porting/traceability/models-dev-reasoning-options.md)

## Implementation inventory

- `crates/pi-ai/src/models_dev_reasoning_options.rs`: typed options and effort mapping.
- `crates/pi-ai/src/lib.rs`: public module export.

## Tests asserting this spec

- `crates/pi-ai/tests/models_dev_reasoning_options.rs`: four public API tests.
- `crates/pi-ai/tests/differential_models_dev_reasoning_options.rs`: 1,110 exact-source cases through `oracle/models-dev-reasoning-options-oracle.ts`.

## Known gaps (current cycle)

None identified within the approved typed API boundary.

## Out of scope

- Invalid JavaScript runtime inputs and non-JSON number fixtures: [approved typed API exclusions](../porting/traceability/models-dev-reasoning-options.md#explicit-exclusions-and-discrepancies).
- Runtime reasoning/model settings, catalog/configuration changes and CLI integration: this slice exposes only the source module's conversion API.
