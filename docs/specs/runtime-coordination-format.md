# Runtime coordination format

Match frozen `core/runtime-coordination-format.ts` at `4f9e6f8a4`. [Traceability](../porting/traceability/runtime-coordination-format.md) maps all exported/private branches.

## What it must do

- [ ] Format mailbox prompts with exact sender/host lines, body whitespace, fallback text and file-reference order/labels.
- [ ] Format shared-channel prompts with trimmed bodies, exact sender/host lines, message separators and action-only handling guidance, including an empty message list.
- [ ] Preserve the existing mailbox UTF-16 carriers and explicit errors for invalid reference lists/null references.

## How it works

- [Branch map and parity exclusions](../porting/traceability/runtime-coordination-format.md)

## Implementation inventory

- `crates/pi-coding-agent-storage/src/core/runtime_coordination_format.rs`: mailbox and channel prompt formatting.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/differential_runtime_coordination_format.rs`
- `crates/pi-coding-agent/tests/lossless_coordination_format.rs`

## Known gaps (current cycle)

- [ ] Record fresh common-pin differential results.

## Out of scope

Approved adversarial/runtime and beyond-existing-carrier exclusions; see the branch map.
