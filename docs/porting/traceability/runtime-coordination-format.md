# coding-agent runtime-coordination-format traceability

Pinned source: `packages/coding-agent/src/core/runtime-coordination-format.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `crates/pi-coding-agent-storage/src/core/runtime_coordination_format.rs`.

Differential proof: `oracle/runtime-coordination-format-oracle.ts` runs the pinned module through `tsx`; `crates/pi-coding-agent/tests/differential_runtime_coordination_format.rs` compares 10 mailbox/shared-channel cases byte-for-byte in source order (same-session, cross-session, unknown sender, empty/trimmed bodies, labeled/unlabeled/empty-label file refs, empty refs, Unicode, empty channel, multi-message channel).

| Source symbol / branch | Rust item | Proof |
| --- | --- | --- |
| `formatRuntimeMailboxPrompt` | `format_runtime_mailbox_prompt` | differential cases |
| `formatSharedChannelPrompt` | `format_shared_channel_prompt` | differential cases |
| `formatSharedChannelMessage` | `format_shared_channel_message` | differential cases |
| `formatRuntimeMailboxFileReferences` / `formatRuntimeMailboxFileReference` | `format_runtime_mailbox_file_references`, `format_runtime_mailbox_file_reference` | file-ref cases |
| `senderSession \|\| "unknown-session"`, `senderAgent \|\| "main"` | `sender_lines` | unknown-sender case |
| `body.trim() \|\| "No message body."` | `message_body` + `pi_js_compat::string::js_trim` | empty/trimmed cases |

Gaps: none; exact string parity.
