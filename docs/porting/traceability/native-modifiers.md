# native-modifiers.ts frozen traceability

Reference: immutable `4f9e6f8a4`, `packages/tui/src/native-modifiers.ts` (59 lines).
Rust: `crates/pi-tui/src/native_modifiers.rs`.

| Export/branch | Rust/proof |
|---|---|
| `ModifierKey` shift/command/control/option | `ModifierKey` enum; `native_modifiers` test |
| `isNativeModifierPressed`, non-darwin platform | false; Linux differential `differential_native_modifiers` |
| Darwin helper cache, x64/arm64 guard, three candidate locations, valid function check | approved macOS addon gap, no native implementation |
| Missing/throwing helper, strict `=== true` result | gap outside Linux; no false claim of native parity |

## Native addon implementation rows

Existing approval: [AGENTS.md](../../../AGENTS.md), macOS/Windows addons remain gaps. Re-inspected frozen files, no execution claim on Linux.

| Frozen native file | Inspected exports/branches | Classification |
|---|---|---|
| `native/darwin/src/darwin-modifiers.c` | `isModifierPressed`, CoreGraphics modifier masks, `CGEventSourceFlagsState`, invalid/missing symbols | gap: needs macOS native platform proof |
| `native/win32/src/win32-console-mode.c` (53 lines) | `node_symbol` searches main module then node.dll; N-API registration exports ONLY `enableVirtualTerminalInput`; invalid stdin/GetConsoleMode/SetConsoleMode failure returns false, success ORs `0x0200` | gap: Windows API + N-API unavailable on this Linux target; no `isModifierPressed` export in frozen Windows addon |

Frozen modifier helper loads Darwin ONLY; Windows VT input loading belongs to `terminal.ts`. The prior traceability incorrectly described later Windows modifier exports. Loading native addons is not ported; do not promote native implementation rows to done.
