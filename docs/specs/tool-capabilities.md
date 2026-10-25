# Tool capability names

## What it must do

- [x] Export the frozen nine child-disabled agent tool names in source order, including `answer_question` and `set_agent_model`.
- [x] Export supervisor-only names in order: `ask_supervisor`, `manage_goal`.
- [x] Export the standalone-disabled set in insertion order: child-disabled names, supervisor-only names, then `send_agent_message`, `contact_parent`, `list_sessions`, `broadcast`, `channel_post`, `resume_session`.
- [x] Preserve membership without case folding, trimming or treating prototype-property names as members.

## How it works

The module has three data exports and no input functions. Rust arrays and an immutable `LazyLock<IndexSet<&str>>` retain the exact ASCII literals and iteration order. Built-in consumers read/copy these exports; none mutates them or depends on JavaScript object identity. [Full map](../porting/traceability/tool-capabilities.md).

## Implementation inventory

- `crates/pi-coding-agent-data/src/core/tool_capabilities.rs`: the two arrays and ordered standalone set; unchanged by this revalidation.
- `oracle/coding-agent-core-leaves-oracle.ts`: actual pinned module export/order/membership snapshot.
- `crates/pi-coding-agent/tests/differential_core_leaves.rs`: dedicated whole-module comparison, separated from unrelated constant-module tests.

## Tests asserting this spec

`all_tool_capability_exports_and_membership_match_frozen` compares all three exports and 26 concrete lookups: all 17 members plus nine nonmembers, including empty, case/space/NUL differences, Unicode and `__proto__`. The native data already matches; no production fix or behavioral RED is claimed.

## Known gaps (current cycle)

No module-specific semantic gap. General Rust-native/owned-value rules apply; the read-only built-in consumer audit is recorded in traceability. Final compiler/lint/fixture gates remain pending until the slice checkpoint.

## Out of scope

Changing session, architect or agents-core runtime filtering; shared runtime APIs, new dependencies and operational state.
