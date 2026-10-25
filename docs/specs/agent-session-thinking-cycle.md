# Agent-session thinking cycle

Coding-agent thinking cycling follows frozen `agent-session.ts:4256–4268`. See the [module trace](../porting/traceability/agent-session.md).

## What it must do

- [x] If the current valid thinking level is not in the model's available list, cycling selects its first level, not its second.

## How it works

- [Agent-session trace](../porting/traceability/agent-session.md)

## Implementation inventory

- `crates/pi-coding-agent-core/src/core/agent_session.rs`: model-aware thinking cycle.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/agent_session_thinking_cycle.rs`
- `oracle/agent-session-scenarios.ts`: actual frozen operation.

## Known gaps (current cycle)

The valid unlisted `xhigh` case passes against frozen. Contradictory model capability configurations remain outside this slice.

## Out of scope

Model-capability policy and contradictory custom models with no available thinking levels are not changed.
