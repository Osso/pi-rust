# Lossless runtime/mailbox content

Frozen source: `4f9e6f8a4`; mailbox migration starts at `c92e00f5`.

- Canonical persisted mailbox JSON has one `JsValue` authority. Enqueue and retry compare UTF-16 identities without scalar reconstruction; original row bytes remain the compare-and-swap token.
- Public mailbox bodies, addresses, thread IDs and message IDs retain UTF-16. Mailbox file references retain raw JSON strings without widening unrelated `AgentFileReference`.
- SQLite lookup/binding keys use Node UTF-8 encoding; bound keys do not replace strings inside canonical JSON or receipts.
- Multi-agent store serialization, restoration and completion notifications retain raw message content and summary units. Store copy projections continue dropping unknown envelope fields as frozen source does.
- Supervisor request payload objects, response content and human-review content retain UTF-16, including arbitrary object keys. Prompt JSON uses the lossless pretty writer.
- Broadcast content reaches the canonical lossless enqueue directly.

Excluded: parent `agent_session.rs` integration, other tools/extensions/modes, unmerged Q16 and host-relay API reconciliation. No full-parity claim; proof and outstanding boundaries: [traceability](../porting/traceability/lossless-runtime-mailbox.md).
