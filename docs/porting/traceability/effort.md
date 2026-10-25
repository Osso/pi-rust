# `effort` extension traceability

Source: `packages/coding-agent/extensions/effort/src/index.ts`. Rust:
`crates/pi-coding-agent-extensions/src/bundled_extensions/effort/mod.rs`.

| TypeScript symbol / branch | Rust item | Proof |
|---|---|---|
| `default` factory registering `effort` and `multi-agent` | `effort_extension`, `register_delegation_control`, `register` | `bundled_commands.rs::effort_registers_command_and_opens_selector`; differential `commandDescriptions` |
| `getEffortLevels` (`getSupportedThinkingLevels`) | `get_effort_levels` reusing `agent_session::thinking_level_from_model` | differential `select-high`, `unsupported`; behavioral tests |
| `formatEffortLevels` | `format_effort_levels` | differential `unsupported` message |
| `findSelectedEffort` (`level === effort.toLowerCase()`) | `find_selected_effort` | differential `set-high`, `unsupported` |
| `parseMultiAgentMode` | `parse_multi_agent_mode` | differential `explicit-policy`, `invalid-mode` |
| `restoreDelegationMode` (last custom entry wins, default proactive) | `restore_delegation_mode` | differential `restore-explicit`; behavioral `effort_restores_mode_from_branch` |
| `persistDelegationMode` (`multi-agent-mode`, `{mode}`) | `persist_delegation_mode` | differential `explicit-policy`, `ultra` |
| `updateDelegationStatus` (dim prefix + mode) | `update_delegation_status` | differential `status`; behavioral `effort_status_dims_only_the_prefix` |
| `policyText` / `removeDelegationPolicy` / `injectDelegationPolicy` | `policy_text`, `remove_delegation_policy`, `inject_delegation_policy` | differential `explicit-policy`, `restore-explicit`, `ultra` |
| `applyDelegationMode` / `setDelegationMode` (ultra→max when explicit) | `apply_delegation_mode`, `set_delegation_mode` | differential `explicit-from-ultra`; behavioral test |
| `handleDelegationCommand` (child guard, selector, invalid mode) | `handle_delegation_command` | differential `child`, `selector-mode`, `invalid-mode` |
| `handleEffortCommand` (no model, selector, invalid effort) | `handle_effort_command` | differential `select-high`, `no-model`, `cancel-select`, `unsupported` |
| `setEffort` (ultra activates proactive; label) | `set_effort` | differential `ultra`; behavioral test |
| `session_start` / `session_tree` restore + status | handlers in `register_delegation_control` | differential `restore-explicit`, `status` |
| `thinking_level_select` ultra reactivation | `on_thinking_level_select` | differential `thinking-ultra` |
| `before_agent_start` policy injection / child no-op | `on_before_agent_start` | differential `explicit-policy`, `restore-explicit`, `ultra`, `child` |

## Discrepancies / gaps

- `ctx.setThinkingLevel` is provided by the `ExtensionContext` callback added in
  `261b879`; the value is the agent-core `ThinkingLevel` enum.
- `session_tree` is registered with the same restore handler; the differential
  oracle drives `session_start` because both call the same Rust function.
- Status coloring uses `try_current_theme()` and returns the source's
  `Theme not initialized. Call initTheme() first.` as a handler error, matching
  the runner's caught throw before `initTheme` (for example `pi extensions`).

## Current-pin disabled mode (2026-10-25)

Frozen modes are `proactive`/`disabled`, not `explicit`; proactive displays as
`active`. Disabled removes the policy block rather than injecting an explicit
permission policy. It removes the frozen ten subagent tool names, remembers
removed names in first-seen order, and restores them once (deduplicated) when
proactive is selected. Mode persistence precedes tool updates and status.
Selecting ultra reactivates delegation; disabling ultra retains max reasoning.
The differential adds real tool-state callbacks and exact final active tools /
setActiveTools traces, including duplicate input tool names and restore order.
Existing explicit cases remain and now prove the frozen invalid-mode warning.
Direct disabled-mode policy/notification expectations use frozen-probed values.
RED: the old implementation kept tools, accepted explicit and injected policy.
