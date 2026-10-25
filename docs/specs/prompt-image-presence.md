# Prompt image presence

Agent-session prompt images follow frozen `agent-session.ts`: omission and an explicit empty image array have distinct extension-visible values. See the [module trace](../porting/traceability/agent-session.md).

## What it must do

- [ ] Preserve omitted, empty and populated images in input-extension events.
- [ ] Preserve absence, explicit empty images and an input transform to empty images through `before_agent_start`.
- [ ] Preserve optional images at RPC and initial print/interactive prompt callers; user-message image content remains unchanged.

## How it works

- [Agent-session trace](../porting/traceability/agent-session.md)

## Implementation inventory

- `crates/pi-coding-agent-core/src/core/agent_session.rs`: optional prompt images and extension-event propagation.
- `crates/pi-coding-agent/src/modes/{rpc/rpc_mode.rs,rpc/rpc_mode/input.rs,print_mode.rs}`: RPC and print boundaries.
- `crates/pi-coding-agent-interactive/src/modes/interactive/interactive_mode.rs` and `crates/pi-coding-agent/src/main.rs`: initial-image propagation.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/agent_session_prompt_images.rs`
- `oracle/agent-session-input-images.ts`: actual frozen extension handlers.

## Known gaps (current cycle)

- [ ] Runtime proof for the new core image-presence cases is pending; workspace compilation passes.

## Out of scope

Image decoding, provider validation and wrong-typed JavaScript inputs are not changed.
