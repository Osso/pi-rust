# Interactive settings selector

Port the frozen `coding-agent/src/modes/interactive/components/settings-selector.ts`. [Traceability](../porting/traceability/interactive-settings-selector.md) records source evidence.

## What it must do

- [x] Preserve frozen row ordering, terminal-image capability gating, search/navigation, toggle callbacks and missing hide-tool/hide-coordination settings.
- [x] Preserve fractional/nonfinite numeric configuration display and parsed numeric callbacks through the shared JavaScript number authority.
- [x] Preserve unknown warning properties, lone UTF-16 units, warning submenu state across reopen and callback failure ordering.
- [x] Retain themed Text/SelectList/SettingsList instances for each submenu lifetime; preview, rollback, automatic/single selection and invalidation match frozen sequences.
- [x] Expose the same live settings list used by inherited Container rendering.

## How it works

- [Module traceability](../porting/traceability/interactive-settings-selector.md)

## Implementation inventory

- `crates/pi-coding-agent-interactive/src/modes/interactive/components/settings_selector.rs`: settings, select, warning and theme components.
- `crates/pi-coding-agent-interactive/src/modes/interactive/interactive_mode.rs`: settings construction and persistence callbacks.
- `crates/pi-coding-agent-data/src/core/http_dispatcher.rs`: single f64 timeout-display helper.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/differential_settings_selector.rs`
- `crates/pi-coding-agent/tests/settings_selector.rs`

## Known gaps (current cycle)

- [ ] Parent final independent/readability gates; coupled [tool slice](interactive-tool-execution.md) has focused numeric/lifecycle/clock proofs.

## Out of scope

Invalid JavaScript runtime types are excluded by the approved typed-API boundary. HTTP timeout parsing/runtime is separate from the selector's numeric display. No unknown settings JSON is scrubbed.
