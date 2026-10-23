# Interactive header

Match frozen `interactive-mode.ts` startup/header registration and reload expansion, including its optional callable `setExpanded` shape. [Native representation and proof](../porting/traceability/interactive-mode.md#header-registration-checkpoint-2026-10-23).

## What it must do

- [x] Render frozen compact/expanded startup hints from effective keybindings, including quiet/verbose behavior.
- [x] Preserve header-container spacers while replacing custom headers and restoring the original built-in header.
- [x] Invoke registered custom expansion callbacks with the tool-output expansion flag; omit the call for nonexpandable components.
- [x] Propagate before-invalidate hook rejection before dispose; release the lifecycle permit and allow the next transition.
- [ ] Prove header factory/setExpanded errors and owner/worker synchronous response paths.
- [ ] Prove active-header expansion during reload, including extension-provided replacements.

## How it works

- [Interactive traceability](../porting/traceability/interactive-mode.md#header-registration-checkpoint-2026-10-23)
- [Runtime error boundary](../porting/traceability/agent-session-runtime.md)

## Implementation inventory

- `crates/pi-coding-agent/src/core/extensions/types.rs` — header factory result and optional expansion callback.
- `crates/pi-coding-agent/src/modes/interactive/interactive_mode.rs` — startup rendering, registration and UI-owner delivery.
- `crates/pi-coding-agent/src/core/agent_session_runtime.rs` — fallible synchronous before-invalidate boundary.

## Tests asserting this spec

- `ui_tests::header_startup_and_registration_frames_match_frozen_expandability` and `oracle/interactive-header-oracle.mts` — four startup states, expandable/plain registration and restoration, exact UTF-16/ANSI frames.
- `tests/agent_session_runtime.rs::before_invalidate_error_skips_dispose_and_releases_gate_for_next_transition` and `oracle/agent-session-runtime-invalidate-error-oracle.mts` — raw failure, retained live session and released lifecycle permit.

## Known gaps (current cycle)

- [ ] Pending header error/worker/reload branches above; whole interactive-mode audit remains open.

## Out of scope

Full startup initialization and remaining extension component APIs are separate whole-module audit work; no TypeScript extension loading is added.
