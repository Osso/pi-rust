# User message rendering

Match frozen `4f9e6f8a4` `coding-agent/src/modes/interactive/components/user-message.ts`. [Traceability](../porting/traceability/interactive-user-message.md) maps source branches.

## What it must do

- [x] Render the source Markdown box, output padding and OSC133 markers exactly.
- [x] In-transit messages use dim text and a muted sending row; returning to normal removes both effects. Reapplying the same state does not change output.
- [x] Preserve raw UTF-16 text through state changes, padding and invalidation.
- [x] Expose only frozen constructor inputs: text, optional Markdown theme and output padding.

## How it works

- [Source mapping and proof](../porting/traceability/interactive-user-message.md)

## Implementation inventory

- `crates/pi-coding-agent-ui/src/modes/interactive/components/user_message.rs`: Markdown, state, styling and zone markers.

## Tests asserting this spec

- `tests/differential_interactive_components.rs::user_message_in_transit_matches_the_pinned_typescript_source`: transition sequence and five render variants.
- `tests/raw_error_consumer_components.rs::user_message_keeps_raw_units_across_padding_and_invalidation`: raw units across transitions.
- `tests/interactive_components_c.rs`: user-message padding/zone tests.

## Known gaps (current cycle)

- [x] Post-commit 437ed5c9 targeted differential1, raw21 and native2 pass; see traceability.

## Out of scope

Interactive-mode scheduling and coordination visibility integration are separate Q13 modules.
