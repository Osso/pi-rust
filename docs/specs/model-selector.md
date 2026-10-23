# Model selector

Common-pin `components/model-selector.ts` selects a model from the actual refreshed ModelRegistry. [Implementation and authority map](../porting/traceability/interactive-model-selector.md).

## What it must do

- [x] Refresh synchronously on construction, capture load errors, then publish models/request render asynchronously.
- [x] Refresh scoped model metadata by provider/id, retaining missing scoped models; sort current model first and otherwise stably by provider.
- [x] Retain selected index under filtering with source min-clamping; support all/scoped toggles, wrapping arrows, empty-list guards and callback-only selection/cancel.
- [x] Render the exact ten-row window, scroll count, current checkmark, error/no-match/model-name branches and raw UTF-16 display names.
- [x] Preserve actual inherited Container children and live focused Input, including direct submit and clear-before-load lifecycle.
- [x] Catch actual registry getAvailable failures from the Vertex ADC home lookup and show their original message without clearing unrelated configuration.
- [x] `/model` auto-selects only an exact model reference; other text opens this actual selector with the captured search input, not a generic list.
- [x] Host Enter selection and Escape cancellation retain the frozen model/status/editor behavior.

## How it works

- [Traceability](../porting/traceability/interactive-model-selector.md)

## Implementation inventory

- `crates/pi-coding-agent/src/modes/interactive/components/model_selector.rs` — selector, mounted children and deferred registry snapshot.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/differential_model_selector.rs` — nine actual-source/real-registry scenarios.
- `crates/pi-coding-agent/tests/differential_model_selector_nss.rs` — real Vertex NSS throw/catch process differential.
- `interactive_mode::ui_tests::viewed_child_cycle_routes_live_targets_and_preserves_frozen_errors` — six actual frozen resolver/cycle and native-mode scenarios.
- `interactive_mode::ui_tests::model_command_uses_exact_references_and_the_frozen_model_selector` — 11 actual-source controller/widget and native-mode scenarios.

## Known gaps (current cycle)

- [x] Viewed-child cycling resolves a live interactive target; absent/core-only targets fail explicitly without mutating main.
- [ ] Viewed-child model candidates/selection, main binding and footer refresh remain a whole-mode integration slice.
- [ ] `app.model.select` key-action wiring remains in the whole-mode key-handler audit.

## Out of scope

Network catalog refresh, settings persistence and old refresh-status rows are absent from frozen; no alternate implementation is retained. Parent owns final workspace gates.
