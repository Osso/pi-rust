# coding-agent channel-post traceability

Pinned source: `packages/coding-agent/src/core/tools/channel-post.ts` at `532f7d12`.

Rust: `crates/pi-coding-agent/src/core/tools/channel_post.rs` (`ChannelPostTool`).

| Source | Rust | Tests |
|---|---|---|
| `channelPostSchema` | `parameters()` | tool metadata test |
| subagent rejection | `execute` multi-agent guard | `rejects_channel_post_from_subagent_contexts` |
| control DB requirement | `ToolContext::require_control_db_path` | `list_sessions_requires_a_control_database_path` pattern; error text matches |
| trimmed non-empty message | `js_trim` + error | `channel_post_rejects_an_empty_message`, differential |
| `postSharedChannelMessage` + `advanceSharedChannelCursor` | `post_shared_channel_message`, `advance_shared_channel_cursor` | `channel_post_posts_the_message_and_advances_the_sender_cursor`, differential state |
| `Posted shared channel message {id}.` + `{messageId}` | `text_result` | differential |

Differential: `tests/differential_session_tools.rs::channel_post_tool_differential` compares the
result and the channel rows/cursor on identical control DBs.

## Gaps

- `renderCall`/`renderResult` need the interactive `Theme`, which is not ported.
