# Channel post tool

Match `packages/coding-agent/src/core/tools/channel-post.ts` at immutable `4f9e6f8a4`.

- Sequential main-session tool with exact action-required schema, description and seven prompt guidelines.
- A nonempty agent ID or required-agent-ID flag rejects before database/message validation. Empty ID is falsy for this guard but remains an empty string in sender identity, not null.
- Require a nonempty control database path before trimming the message. Trim native UTF-16 with JavaScript whitespace rules; reject an empty result.
- Persist the full trimmed message and sender identity, then advance that sender's cursor to the posted ID. Errors propagate; no automatic retry or self-echo.
- Return posted message ID in text and details. Ignore unused abort/update arguments, matching source.
- Render call title and joined text-only result output. Preserve Text reuse and frozen native TypeError for incompatible retained components.

Node `node:sqlite` rejecting lastInsertRowid above2^53 is an approved runtime-binding gap (main ruling2026-10-24 under2026-10-23 policy), not an added native rowid restriction. Shared DB helpers remain unchanged.

[Traceability](../porting/traceability/channel-post.md) maps proof and exclusions.
