# native-modifiers.ts traceability

Source: `packages/tui/src/native-modifiers.ts` (66 lines). Rust: `crates/pi-tui/src/native_modifiers.rs`.

## Exported symbols

| TypeScript export | Rust item | Tests |
|---|---|---|
| `type ModifierKey` | `enum ModifierKey { Shift, Command, Control, Option }` | `tests/native_modifiers.rs` |
| `isNativeModifierPressed(key)` | `is_native_modifier_pressed(key: ModifierKey) -> bool` | `tests/native_modifiers.rs` |

## Branches

| TypeScript branch | Rust branch | Coverage |
|---|---|---|
| cached helper already resolved | none | gap: no native helper exists |
| `arch !== "x64" && arch !== "arm64"` → no helper | none | gap |
| platform `darwin` / `win32` candidate paths | none | gap |
| require succeeds and exports `isModifierPressed` | none | gap |
| require throws / invalid export → try next | none | gap |
| helper absent → `false` | `false` | `no_native_helper_is_available_in_the_rust_port` |
| helper throws → `false` | `false` | same |
| helper returns `true` | not reachable | gap |

## Gaps

- The TypeScript module's only non-`false` path is a Node N-API addon shipped as a platform prebuild (`native/darwin/prebuilds/...`, `native/win32/prebuilds/...`). The Rust port does not load Node addons, so `is_native_modifier_pressed` always returns `false`.
- This matches TypeScript behavior on Linux and on any platform where the addon fails to load. On macOS/Windows with a working addon the Rust result would differ. A future Rust-native platform layer must supply this state; no consumer exists in the ported slices yet, so no injection API was added (minimal scope).

## Differential testing

Not applicable: the function depends on Node's `process.platform`, `process.arch`, `process.execPath`, and the presence of platform-specific `.node` files. The observable Linux behavior (`false`) is covered by the unit test.

## Discrepancies

None in the ported behavior. The unported addon path is classified as an explicit gap, not a source bug.

## Native addon implementation rows

The existing [AGENTS.md approval](../../../AGENTS.md) says: “macOS/Windows addons are a gap (2026-10-19).” These are the actual implementations of those addons, not new exclusions inferred from their filenames. Both inventory rows change from `todo` to that existing approved `gap`; addon build scripts remain outside this reconciliation.

| Original pinned source | Exports and implementation branches inspected | Rust/proof boundary |
|---|---|---|
| `packages/tui/native/darwin/src/darwin-modifiers.c` | N-API registration exports `isModifierPressed`; shift/command/control/option map to CoreGraphics masks; recognized masks query `CGEventSourceFlagsState`; unknown names/missing symbols/invalid arguments return false. | No native implementation; false-only function cannot query macOS modifier state. |
| `packages/tui/native/win32/src/win32-console-mode.c` | N-API registration exports `isModifierPressed` and `enableVirtualTerminalInput`; modifier aliases query `GetAsyncKeyState` via user32; missing symbol/unknown name returns false. VT branch obtains stdin, reads console mode, sets `ENABLE_VIRTUAL_TERMINAL_INPUT`, returns success/failure. | Neither actual Windows modifier queries nor console-mode mutation is proven ported. VT-input export is part of this same approved Windows addon, not evidence of parity from Linux terminal behavior. |

The existing `no_native_helper_is_available_in_the_rust_port` pass proves only false-only behavior. Exact current log location/revision: [proof ledger](../proven-assets-proof.md). No macOS/Windows execution, N-API parity or new approval claimed.
