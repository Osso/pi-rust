# coding-agent channel-post traceability

Frozen source: `packages/coding-agent/src/core/tools/channel-post.ts` at `4f9e6f8a4`.

Actual `differential_tools_index` Cargo RED at `23ef1259` (`tool-metadata-incrate-green.log`, despite its historical filename) exposed the old description. Source `channel-post.ts:7–9,23–33` defines the action-required schema description, description/snippet and seven coordination guidelines; native constants/schema now match them exactly. Execution is unchanged. Metadata Cargo GREEN1 at `8bb3bf7d` (`tool-metadata-final-green.log`).

Rust: `crates/pi-coding-agent/src/core/tools/channel_post.rs` (`ChannelPostTool`).

| Source | Rust | Tests |
|---|---|---|
| `channelPostSchema` | `parameters()` | tool metadata test |
| subagent rejection | `execute` multi-agent guard | `rejects_channel_post_from_subagent_contexts` |
| control DB requirement | `ExtensionContext::control_db_path` + local requirement check | `list_sessions_requires_a_control_database_path` pattern; error text matches |
| trimmed non-empty message | `JsString::trim` + error | `channel_post_rejects_an_empty_message`, differential |
| `postSharedChannelMessage` + `advanceSharedChannelCursor` | `post_shared_channel_message`, `advance_shared_channel_cursor` | `channel_post_posts_the_message_and_advances_the_sender_cursor`, differential state |
| `Posted shared channel message {id}.` + `{messageId}` | `text_result` | differential |

Differential: `tests/differential_session_tools.rs::channel_post_tool_differential` compares the
result and the channel rows/cursor on identical control DBs.

## Gaps

- Current-pin execution/result/DB-state differential passes at `1350065f` within `differential_session_tools` GREEN12 (`fixture-session-final-test.log`). This does not replace separate renderer proof.

## Renderers

Renderers ported: `render_call` (title), `render_result` (tool output). Covered by `differential_interactive_components.rs::tool_renderer_cases`.
