# Prompt image presence

Agent-session prompt images follow frozen `agent-session.ts`: omission and an explicit empty image array have distinct extension-visible values. See the [module trace](../porting/traceability/agent-session.md).

## What it must do

- [x] Preserve omitted, empty and populated images in input-extension events.
- [x] Preserve absence, explicit empty images and an input transform to empty images through `before_agent_start`; preserve populated image content in user messages.
- [x] Preserve optional images through real RPC stdin/stdout commands and registered input extensions.
- [ ] Preserve optional images at initial print/interactive prompt callers.

## How it works

- [Agent-session trace](../porting/traceability/agent-session.md)

## Implementation inventory

- `crates/pi-coding-agent-core/src/core/agent_session.rs`: optional prompt images and extension-event propagation.
- `crates/pi-coding-agent/src/modes/{rpc/rpc_mode.rs,rpc/rpc_mode/input.rs,print_mode.rs}`: RPC and print boundaries.
- `crates/pi-coding-agent-interactive/src/modes/interactive/interactive_mode.rs` and `crates/pi-coding-agent/src/main.rs`: initial-image propagation.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/agent_session_prompt_images.rs`
- `oracle/agent-session-input-images.ts`: actual frozen extension handlers.
- `crates/pi-coding-agent/tests/differential_rpc_mode.rs`: real RPC image-presence differential.

## Known gaps (current cycle)

- [ ] Initial print/interactive callers have mechanical option propagation and workspace compilation proof, not a new mode-level extension-event probe.

## Out of scope

Image decoding, provider validation and wrong-typed JavaScript inputs are not changed.
