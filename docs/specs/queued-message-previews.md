# Queued-message previews

Preserve frozen `4f9e6f8a4` display queue provenance and coordination visibility, without changing delivery. [Traceability](../porting/traceability/coordination-visibility.md) maps the bounded agent-session slice and complete visibility helper.

## What it must do

- [x] Keep exact text, input source and custom type in display queues; text getters and clearQueue retain their existing text outputs.
- [x] Return independent, non-consuming preview snapshots; enqueue/remove/clear keep previews synchronized with source behavior.
- [x] Identify shared-channel previews and extension mailbox envelopes exactly, including optional host/session and joined user text blocks.
- [x] Hide only coordination session previews when configured, not user input or compaction queues; delivery remains untouched.
- [x] Retain unmatched UTF-16 units in text, envelope classification and snapshots.

## How it works

- [Queue and visibility source mapping](../porting/traceability/coordination-visibility.md)

## Implementation inventory

- `crates/pi-coding-agent/src/core/agent_session.rs`: native queue previews and snapshots.
- `crates/pi-coding-agent/src/modes/interactive/coordination_visibility.rs`: source envelope/provenance predicates.
- `crates/pi-coding-agent/src/modes/interactive/interactive_mode.rs`: visible queue projection and pending rows.

## Tests asserting this spec

- `tests/agent_session.rs::queued_previews_keep_input_provenance_and_are_non_consuming_snapshots`: real streaming queue/snapshot boundary.
- `tests/coordination_visibility.rs`: pinned helper differential and raw-envelope case.
- `interactive_mode::ui_tests::hidden_coordination_queue_keeps_user_input_and_delivery_intact`: real UI/session integration.

## Known gaps (current cycle)

- [x] Targeted queue/visibility/UI proofs pass at 15f0eca7; full agent_session52/53 retains the expected mailbox RED (traceability).

## Out of scope

Whole agent-session re-pin and transcript coordination-message containers are separate slices.
