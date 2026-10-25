# Resource configuration selector

Match frozen `4f9e6f8a4` `coding-agent/src/modes/interactive/components/config-selector.ts` and its CLI caller. [Traceability](../porting/traceability/interactive-config-selector.md) records representation and proof boundaries.

## What it must do

- [x] Accept one flat resolved resource set; display Resource Configuration, filter hint, binary checkboxes and grouped resource categories.
- [x] Match frozen frames, navigation, search, paging, cancellation and toggle-render callbacks.
- [x] Write relative +/- resource patterns to each resource's metadata scope; package filters preserve unrelated resource arrays.
- [x] Preserve unmatched UTF-16 units in native paths, search, display and persisted patterns.
- [x] CLI resolves resources once using command settings; legacy local flag does not select a separate view or impose a trust gate.

- [ ] `config` ignores help, unknown, positional and local-scope flags; it opens the selector and exits 0 after close. Trust override flags remain last-wins.

## How it works

- [Component traceability](../porting/traceability/interactive-config-selector.md)
- [CLI integration](../porting/traceability/config-selector.md)

## Implementation inventory

- `crates/pi-coding-agent-ui/src/modes/interactive/components/config_selector.rs`: resource grouping, filtering, rendering and settings toggles.
- `crates/pi-coding-agent/src/cli/config_selector.rs`: terminal lifecycle and command adapter.
- `crates/pi-coding-agent/src/package_manager_cli.rs`: resource resolution and selector request.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/differential_interactive_components.rs`: dedicated selector differential with six scenarios.
- `crates/pi-coding-agent/tests/config_selector.rs`: exact native patterns and raw UTF-16.
- `crates/pi-coding-agent/tests/config_selector_cli.rs`: terminal lifecycle probes.
- `crates/pi-coding-agent/tests/config_dispatch_process.rs`: real frozen/native pty dispatch for ignored arguments, selector entry and exit 0.
- `crates/pi-coding-agent/tests/package_command_execution.rs`: command resource-resolution/trust boundary.

## Known gaps (current cycle)

- [x] Native4, CLI4 (+1 ignored probe), package execution31 and package CLI8 pass at `13e744ba` (see traceability).

## Out of scope

Broader package command re-pin and whole interactive-mode differential remain separate slices.

- Home-directory lookup failures during group-label construction reject selector construction and CLI selection; they are not converted to a label or an uncaught in-chain panic.
