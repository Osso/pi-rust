# Model selector

Common-pin `components/model-selector.ts` selects a model from the actual refreshed ModelRegistry. [Implementation and authority map](../porting/traceability/interactive-model-selector.md).

## What it must do

- [x] Refresh synchronously on construction, capture load errors, then publish models/request render asynchronously.
- [x] Refresh scoped model metadata by provider/id, retaining missing scoped models; sort current model first and otherwise stably by provider.
- [x] Retain selected index under filtering with source min-clamping; support all/scoped toggles, wrapping arrows, empty-list guards and callback-only selection/cancel.
- [x] Render the exact ten-row window, scroll count, current checkmark, error/no-match/model-name branches and raw UTF-16 display names.
- [x] Preserve actual inherited Container children and live focused Input, including direct submit and clear-before-load lifecycle.
- [ ] Catch actual registry getAvailable failures from the Vertex ADC home lookup and show their original message without clearing unrelated configuration.

## How it works

- [Traceability](../porting/traceability/interactive-model-selector.md)

## Implementation inventory

- `crates/pi-coding-agent/src/modes/interactive/components/model_selector.rs` — selector, mounted children and deferred registry snapshot.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/differential_model_selector.rs` — nine actual-source/real-registry scenarios.

## Known gaps (current cycle)

- [ ] Shared Vertex ADC→auth→registry Result propagation, separately authorized and committed.

## Out of scope

Network catalog refresh, settings persistence and old refresh-status rows are absent from frozen; no alternate implementation is retained. Parent owns final workspace gates.
