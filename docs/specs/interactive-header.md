# Interactive header

Match frozen `interactive-mode.ts` startup/header registration and reload expansion, including its optional callable `setExpanded` shape. [Native representation and proof](../porting/traceability/interactive-mode.md#header-registration-checkpoint-2026-10-23).

## What it must do

- [x] Render frozen compact/expanded startup hints from effective keybindings, including quiet/verbose behavior.
- [x] Ctrl+O and extension tool-expansion requests update the active header and expandable loaded-resource/chat children in that order; plain children remain unchanged. Expansion does not persist an output-padding setting. Header callback failures propagate before expanding later children or requesting a render.
- [x] Preserve header-container spacers while replacing custom headers and restoring the original built-in header.
- [x] Invoke registered custom expansion callbacks with the tool-output expansion flag; omit the call for nonexpandable components.
- [x] Propagate before-invalidate hook rejection before dispose; release the lifecycle permit and allow the next transition.
- [x] Preserve original typed header factory/setExpanded errors through synchronous owner and worker response paths.
- [x] Reapply active-header expansion after session_start can alter its state; plain components remain unchanged.
- [x] Preserve raw header-error display framing and editor restoration through real source showError, not helper-argument capture.

## Startup initialization

- [x] Print the configured model scope before mounting the UI when nonempty and not quiet (verbose overrides quiet); preserve thinking suffixes and effective cycle-key text/colors.
- [x] Bind extensions and discover startup resources before listing resources and restoring initial messages; quiet mode keeps source diagnostic behavior.
- [x] Use the complete current-session rebind path to refresh provider count, editor border, title and cwd. Register resource themes after binding.
- [x] Match actual frozen init output for fresh, quiet, quiet/scoped, verbose/scoped and restored sessions, including full header/resource/chat UTF-16/ANSI frames, title, console line and provider count.
- [x] After initialization, theme changes invalidate the UI, update the editor border and request render; footer branch changes request render. The native branch subscription stays alive until stop.

## How it works

- [Interactive traceability](../porting/traceability/interactive-mode.md#header-registration-checkpoint-2026-10-23)
- [Runtime error boundary](../porting/traceability/agent-session-runtime.md)

## Implementation inventory

- `crates/pi-coding-agent-tools/src/core/extensions/types.rs` — header factory result and optional expansion callback.
- `crates/pi-coding-agent-interactive/src/modes/interactive/interactive_mode.rs` — startup rendering, registration and UI-owner delivery.
- `crates/pi-coding-agent-core/src/core/agent_session_runtime.rs` — fallible synchronous before-invalidate boundary.

## Tests asserting this spec

- `ui_tests::header_startup_and_registration_frames_match_frozen_expandability` and `oracle/interactive-header-oracle.mts` — four startup states, expandable/plain registration and restoration, repeated expansion/collapse of active header/resource/chat, unchanged plain child and output padding, exact UTF-16/ANSI frames.
- `tests/agent_session_runtime.rs::before_invalidate_error_skips_dispose_and_releases_gate_for_next_transition` and `oracle/agent-session-runtime-invalidate-error-oracle.mts` — raw failure, retained live session and released lifecycle permit.

## Known gaps (current cycle)

- [ ] Whole interactive-mode audit remains open; these header branch proofs do not imply whole-mode parity.

## Out of scope

Remaining extension component APIs and whole interactive-mode acceptance remain separate audit work; no TypeScript extension loading is added.
