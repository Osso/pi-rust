# Tool capabilities

**User decision, 2026-10-26:** remove `broadcast` from native tool capabilities with the tool. This is a deliberate gap from the frozen source, not parity. The comparison, counts, and checkpoint below describe the former state and do not verify the removal.

Frozen `packages/coding-agent/src/core/tool-capabilities.ts`, all 24 lines, at unmodified `4f9e6f8a4`. Native: `crates/pi-coding-agent-data/src/core/tool_capabilities.rs`. Values/order already match; this slice makes no production edit.

| Frozen export / construction | Native / observable proof |
|---|---|
| `CHILD_DISABLED_AGENT_TOOL_NAMES` (1–11) | Ordered nine-element array; full actual-source export comparison and retained `core_constants::exposes_tool_capability_constants` |
| `SUPERVISOR_ONLY_TOOL_NAMES` (13) | Ordered two-element array, `ask_supervisor` then `manage_goal`; same full export comparison |
| `STANDALONE_DISABLED_TOOL_NAMES` (15–24) | `LazyLock<IndexSet<&str>>`, exact 17-element insertion order and membership; full actual-source comparison plus all 17 positive/nine negative lookups |

`differential_core_leaves::all_tool_capability_exports_and_membership_match_frozen` materializes the actual frozen readonly set rather than serializing it as `{}`. Inputs are concrete names, with case, trailing space, embedded NUL, Unicode, empty and prototype-property controls. Assertions cover exported data, not native implementation shape. Initial focused run is GREEN (`/tmp/pi-rust-repin-core-tools/tool-capabilities-first.log`); no manufactured RED or native behavior correction.

## Read-only built-in consumer audit

- `extensions/agents-core/src/runtime.ts:185–187` spreads both arrays into child exclusions.
- `src/architect/main.ts:30` spreads supervisor-only names into architect exclusions.
- `src/core/agent-session.ts:354,1118,2366` reads standalone membership for availability/execution checks.

No production consumer mutates these exports or relies on shared-reference identity. The approved Rust-native/owned-value boundary therefore needs no special mutable alias behavior. Existing `differential_standalone_worker::standalone_partial_config_validation_and_capability_order_match_frozen` also exercises the imported standalone set at its session option boundary; no session implementation change belongs to this slice.

## Discrepancies / gaps

The frozen standalone-disabled set contains `broadcast`; the native target removes it by user decision. [Contract](../../specs/tool-capabilities.md).

## Checkpoint

Native/test candidate `6cc86baa`; approved oracle type-only correction `1cdf85b6`. Artifacts are under `/tmp/pi-rust-repin-core-tools/`.

| Proof | Result / artifact |
|---|---|
| Actual frozen export/order/membership comparison | 1/0, all three exports plus 26 lookups; `tool-capabilities-first.log` |
| Retained constant and session option-boundary tests | 2/0; `tool-capabilities-retained.log` |
| Workspace all-target keep-going compile | 0 errors/warnings; `tool-capabilities-workspace-summary.json` |
| Target warning Clippy | 0 changed test findings, 504 existing selected-target/dependency diagnostics; `tool-capabilities-clippy-summary.json` |
| Protected strict TypeScript / style / inventory | Compiler 0 (`tool-capabilities-types-green.log`); edited blocks formatter-clean and Biome lint 0 edited findings with six existing unrelated warnings (`tool-capabilities-biome-summary.json`); inventory PASS 686/186679,455done/19gap/46partial/166todo |

Protected compilation first found three pre-existing TS2352 dynamic-data casts in unrelated handlers. Compiling the unchanged `afec7263` fixture reproduces all three (`tool-capabilities-types-baseline.log`). Main approved only explicit `unknown` boundaries, without any/suppression/runtime edits. Whitespace-normalized emitted JavaScript before/after those type-only corrections is byte-identical (SHA256 `ab6dd76db42e35871bb5ed22d4fb018f0d6405aea6695d474d7090f9e98c7ad4`; `tool-capabilities-oracle-type-equivalence.json`). The pinned source bytes remain unmodified.

Manual Rust/TS changed-line readability audit is clean; metrics binary is unavailable. The native module itself is unchanged. No broad checks were repeated solely for the documentation checkpoint.
