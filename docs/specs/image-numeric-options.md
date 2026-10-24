# Image numeric cell options

Match frozen `tui/src/components/image.ts` and its `terminal-image.ts` calculations for numeric cell limits. [Traceability](../porting/traceability/image.md) records proof.

## What it must do

- [x] Preserve fractional maxWidthCells until the component derives default maxHeightCells; do not narrow either option early.
- [x] Preserve JavaScript Math.min/max NaN propagation into cell-size metadata and protocol output.
- [x] Keep frozen Kitty/iTerm2 blank-line layout, explicit image ID retention and no-capability fallback.
- [x] Cache by render width; invalidate before observing capability changes, preserving fallback callback effects.

## How it works

- [Image traceability](../porting/traceability/image.md)
- [Terminal-image traceability](../porting/traceability/terminal-image.md)

## Implementation inventory

- `crates/pi-tui/src/components/image.rs`: f64 options and cached component output.
- `crates/pi-tui/src/terminal_image.rs`: f64 cell metadata and protocol encoding.
- `crates/pi-js-compat/src/number.rs`: single JS min/max authority.
- `crates/pi-coding-agent-interactive/src/modes/interactive/components/tool_execution.rs`: retains tool image-width f64 through Image construction.

## Tests asserting this spec

- `crates/pi-tui/tests/differential_image_component_numbers.rs`:420 frozen component cases.
- `crates/pi-js-compat/tests/differential_number_math.rs`: 121 same-bit min/max pairs including signed zero and NaN GREEN.
- Existing `differential_components`, `differential_terminal_image`, `raw_images`, `terminal_image` regression consumers remain adapted to f64.

## Known gaps (current cycle)

- [ ] Parent final independent/readability gates.

## Out of scope

Cell/image pixel dimensions and explicit image IDs retain the separately approved u32 domain. Component render widths follow the shared usize API. No arbitrary JavaScript input-type coercion is added.
