# Agent reference identity

Preserve the JavaScript object identity read by frozen AgentSession restriction guards (`agent-session.ts:2810–2824`), without using value equality or restoring mutation-through-returned-reference semantics.

- One Agent-owned locked store holds state, model generation and opaque tokens for stored messages. No second copy of authoritative message/model values is introduced.
- Model replacement increments generation, including equal-valued fresh objects. Snapshots from different Agents never identify the same active model.
- Newly supplied message objects receive fresh tokens, including same-valued replacements. Appends preserve existing tokens. Prefix slices/truncation preserve surviving tokens; clearing then re-appending does not resurrect identity.
- Stored session message entries own stable tokens. Rebuilding context transfers their exact tokens into the Agent; custom/branch/compaction projections and truncated clones receive fresh tokens per rebuild. Message-end and direct bash persistence bind the Agent object's token to the newly stored message entry. No equality inference or serialized token fields.
- Restoring messages with an unequal token count fails explicitly without changing state.
- Identity snapshots apply a caller-supplied message predicate under the same lock and return metadata only. Restriction consumers can exclude supervisor-status messages without copying state into a second authority.
- Message-end dispatch exposes the emitted object's token, not whichever message is last after another listener mutates state. The native getter is valid only in that Agent's current awaited listener task scope, including the synchronous callback invocation. Outside dispatch, in another task, for another Agent, and after dispatch it returns an explicit error; no stale value is retained.
- Existing serialized AgentState/AgentEvent schemas remain unchanged. Native append/truncate APIs represent frozen direct pushes/prefix slices; owned set_messages inputs represent newly supplied objects.

[Traceability](../porting/traceability/agent.md#frozen-reference-identity) records exact frozen branches and proof. Restriction advisory/validation execution integration is a subsequent slice, not enabled by this metadata alone.
