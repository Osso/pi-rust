# Tool capabilities

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

None specific to this data-only module. Common-pin compiler/lint/protected fixture/style/inventory checkpoint is pending. [Contract](../../specs/tool-capabilities.md).
