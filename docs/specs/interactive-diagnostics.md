# Interactive diagnostics

Match frozen `interactive-mode.ts:5511–5521` error/warning rendering. [Proof and caller audit](../porting/traceability/interactive-mode.md#diagnostic-helper-correction-2026-10-23).

## What it must do

- [x] Insert one Spacer before every error/warning Text, including the first message.
- [x] Prefix errors with `Error: ` and warnings with `Warning: `, preserving raw UTF-16 and already-prefixed input literally.
- [x] Preserve frozen width-dependent Text wrapping and full ANSI/UTF-16 output.
- [x] Use real frozen diagnostic/status helpers in every affected non-login frame oracle; retain all native assertions against their literal frames.
- [ ] Reconcile login frame fixtures/oracles (lane226).

## How it works

- [Interactive traceability](../porting/traceability/interactive-mode.md#diagnostic-helper-correction-2026-10-23)

## Implementation inventory

- `crates/pi-coding-agent/src/modes/interactive/interactive_mode.rs` — shared error/warning rendering authority.

## Tests asserting this spec

- `ui_tests::diagnostic_helpers_match_frozen_prefix_spacing_and_exact_utf16_frames` and `oracle/interactive-diagnostics-oracle.mts` — actual frozen helpers versus native frames, five widths and three cumulative messages.

## Known gaps (current cycle)

- [ ] Lane226 owns login frame fixture/oracle reconciliation; whole-mode branch coverage remains separate.

## Out of scope

Caller diagnostic arguments, exception classification and other helper behavior are unchanged.
