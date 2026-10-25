# Interactive user-visible parity — paused WIP

Paused at the user's request for build-performance restructuring. No interactive implementation changes or new tests have started. Resume from this commit after the restructuring; do not restore obsolete crate paths.

## Authorized scope

Apply the user-observable policy in parent7b6425a1: interactive-mode.ts7938lines in large startup/render/events and command/session-transition slices, plus agent-session-runtime.ts, self-restart.ts and keybindings.ts. Use representative actual-source TUI render/behavior proofs. Visible output and persistence/IPC remain exact; do not resume adversarial/fault/extreme proof work. Mechanical cross-lane adapters are cleared; semantic deviations/dependencies/TS deltas still need approval.

## Completed checkpoint

Footer/readonly checkpoint895df96318b63d11911b5cbcfda5b0f8d0502d0f is landed by parent09f10e9c. Local policy synchronization e960f6b321f3a9e1e4568d464b7ee965703282fb includes7b6425a1. Combined inventory:686/186679;467done/19gap/47partial/153todo. Footer proof is in [its checkpoint](footer-data-provider-proof.md); do not rerun its broad gates without an intersecting change.

## Concrete next startup slice

Frozen interactive init985–1112 is not yet matched by native init (at this checkout, pi-coding-agent-interactive/src/modes/interactive/interactive_mode.rs:6787):

- Source emits the configured model-scope console line; native init currently has no corresponding step.
- Source invokes rebindCurrentSession, which refreshes provider count, editor border, title and cwd. Native init calls bind_extensions directly and does not update provider count there.
- Source bindCurrentSessionExtensions2050–2140 registers current resource themes and shows loaded resources/startup notices after extension binding. Native init shows those before bind_extensions; its bind helper omits the matching tail.
- Source init subscribes the footer branch-change render callback. Native registration/refresh integration still needs a direct user-visible audit.

These are observed control-flow differences, not yet reproduced behavioral failures. First add representative quiet/normal/scoped startup and restored-session frames using the actual pinned init/rebind methods and the existing capture-terminal/native session fixture; prove the visible differences before editing production code. Existing header, reload and session-option oracles are available; preserve ANSI, padding and ordering.

## Exploration artifacts

`/tmp/pi-rust-repin-lossless-interactive/interactive-source-methods.json` contains321 source methods with line ranges; `interactive-native-outline.txt` is the native structural outline. Name-only unmatched methods are not established gaps: several live in extracted native helpers. Existing interactive trace is historical/composite, not whole-module acceptance. Runtime and restart traces already contain substantial completed behavior proofs; audit realistic remaining coverage instead of rebuilding fault matrices.
