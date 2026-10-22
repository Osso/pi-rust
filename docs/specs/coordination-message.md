# Coordination message rendering

Match frozen `4f9e6f8a4` `coordination-message.ts`; [traceability](../porting/traceability/coordination-message.md) maps the full module and native chat consumer.

## What it must do

- [ ] Hide only rendered lines; retain ordered child identities and raw text through hide/reveal/invalidation.
- [ ] Forward expansion to immediate custom-message children and in-transit state to immediate user-message children; leave other component types untouched.
- [ ] Wrap exactly new chat components for source-classified coordination, reading the current hide setting on render.

## How it works

- [Source mapping](../porting/traceability/coordination-message.md)

## Implementation inventory

- `src/modes/interactive/components/coordination_message.rs`: conditional Container and typed child state forwarding.
- `src/modes/interactive/interactive_mode.rs`: classify/wrap new chat components.

## Tests asserting this spec

- `tests/coordination_message.rs`: pinned plain/rich state differential and raw identity test.
- `interactive_mode::ui_tests::coordination_transcript_hides_and_restores_existing_components`: actual chat consumer.

## Known gaps (current cycle)

- [ ] Targeted post-commit execution pending.

## Out of scope

Full interactive-mode provisional-message lifecycle and global expansion handling remain separate Q13 work.
