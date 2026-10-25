# Extension UI key release and inherited terminal

Native extension custom components can opt into key releases and temporarily
release the TUI terminal for inherited-stdio commands. Contract follows pinned
`packages/tui/src/tui.ts` and the Space Invaders/interactive-shell examples.
Evidence: [traceability](../porting/traceability/core-ui-input.md).

## What it must do

### Component input

- [x] Default key-release opt-out; forward explicit opt-in through the production interactive-mode component adapter to the real TUI filter.
- [x] Continue delivering ordinary presses regardless of release opt-in.
- [x] Preserve Space Invaders held movement/fire, release, timer, collision, scoring, level, pause/save, restart, rendering and disposal behavior against pinned source.

### Terminal ownership

- [x] Expose synchronous stop/start through the factory's TUI handle; terminal release/reacquisition must occur before the method returns.
- [x] Expose forced request-render without changing existing non-forced request-render behavior.
- [x] Preserve interactive-shell command classification, environment options, force prefix, non-TUI result, lifecycle order and full result objects.
- [x] Run shell commands with inherited stdin/stdout/stderr and clear the screen before execution; preserve null statuses for signal/spawn failure.
- [x] Project lone-surrogate input to U+FFFD at existing String boundaries under [compatibility](compatibility.md#out-of-scope).

## How it works

- [Source mapping, behavioral matrix and proof limits](../porting/traceability/core-ui-input.md)

## Implementation inventory

- `crates/pi-coding-agent-core/src/core/extensions/types.rs` — native component/TUI API.
- `crates/pi-coding-agent-interactive/src/modes/interactive/interactive_mode.rs` — active renderer lifecycle, forced-render queue and input adapter.
- `crates/pi-coding-agent/examples/extensions/space_invaders.rs` — explicitly registered native game.
- `crates/pi-coding-agent/examples/extensions/interactive_shell.rs` — explicitly registered inherited-terminal handler.

## Tests asserting this spec

- `crates/pi-coding-agent-interactive/src/modes/interactive/interactive_mode.rs::ui_tests`
- `crates/pi-coding-agent/tests/example_space_invaders.rs`
- `crates/pi-coding-agent/tests/example_interactive_shell.rs`
- `crates/pi-coding-agent/tests/support/extension_examples_l_games.rs`
- `oracle/example-interactive-shell-oracle.ts`

## Known gaps (current cycle)

- [ ] Three baseline interactive-mode tests hang; complete changed-file verification remains blocked. Exact cases and baseline probes: [proof limits](../porting/traceability/core-ui-input.md#redgreen-mutation-and-compatibility-proof).

## Out of scope

- Doom-overlay implementation (separate owner).
- Dynamic TypeScript extension loading, already an approved Rust-native boundary.
- Lifecycle calls from a non-owning UI thread; inherited terminal execution occurs synchronously in the owning factory.
- Forged/malformed saved game values, raw lone surrogates in String APIs, broad workspace gates and unrelated baseline fixes.
