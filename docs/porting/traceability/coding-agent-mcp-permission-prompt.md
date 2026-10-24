# Traceability: `packages/coding-agent/src/core/permissions/mcp-permission-prompt.ts`

Rust: `crates/pi-coding-agent/src/core/permissions/mcp_permission_prompt.rs`.
Tests: `tests/permissions_prompt.rs`.

Pinned source: commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

## Exported surface

| TypeScript | Rust | Tests |
|---|---|---|
| `PermissionPromptInput` | `PermissionPromptInput` (serde, source field names) | `allows_tool_calls_without_further_prompting` |
| `PermissionPromptCaller` | `PermissionPromptCaller` (`Result<Value, String>` models a rejected promise) | every handler test |
| `PermissionPromptDecision` | `PermissionPromptDecision` (`Allow` / `Deny`) | 6 parsing tests |
| `PermissionPromptHandlerOptions` | `PermissionPromptHandlerOptions` | every handler test |
| `parsePermissionPromptDecision` | `parse_permission_prompt_decision` | 6 parsing tests |
| `createPermissionPromptHandler` | `create_permission_prompt_handler` → `PermissionPromptHandler` | 9 handler tests |

## Branches

| TypeScript | Rust | Tests |
|---|---|---|
| `MCP_TOOL_NAME_PATTERN` | `mcp_tool_name_pattern` | invalid-name fallback test |
| existing allow rule skips prompt | `has_allow_rule` under the shared store mutex | `session_allow_rules_skip_future_prompt_calls` |
| desktop notification before prompt | `notify_permission_prompt` | `sends_a_desktop_notification_before_the_prompt` |
| notification closed in `finally` | `handle.close()` after the call | same test |
| prompt call rejection → `undefined` | `Err` → `None` | `falls_back_when_the_prompt_call_fails` |
| `parseResponseObject` string / record / text content | `parse_response_object` | `parses_decisions_from_text_content_records`, string cases |
| `parseUpdatedPermissions` validation | `parse_updated_permissions` | permissions parsing test |
| deny → `{block:true, reason}` | `Deny` arm | `blocks_denied_tool_calls` |
| allow → apply rules then replace input in place | `Allow` arm | `allows_tool_calls_with_updated_input_in_place`, session-rule tests |
| non-session rules persist but do not skip prompts | store semantics | `non_session_allow_rules_do_not_skip_follow_up_prompts` |

## Gaps / deviations

- Common-pin prerequisite (2026-10-24): notification send/close return `Result<_, CallbackError>`; catches at frozen139-142/149-153 log and continue. `fallible_notifications::permission_notifier_send_and_close_failures_preserve_the_prompt_decision` compares real frozen/native denied decisions and call counts with throwing send/close callbacks, including raw UTF-16 throw text. This does not revalidate the rest of this old-pin map.
- `structuredClone(event.input)` maps to a deep `serde_json` clone.
- `PermissionPromptDecision` deliberately has no serde derive: the source type is
  internal, and `PermissionRuleUpdate` (rule-store port) has no `type` field to
  serialize. Parsing builds the typed decision directly.

## Q18 native permission cwd (2026-10-22)

PermissionPromptInput/HandlerOptions cwd and notification body are native. MCP
callback receives original units; notification text encodes only at the notifier
implementation's argv boundary. Raw cwd callback fixture written but unexecuted
while coding compilation is blocked. Tool names/call IDs stay approved scalar
metadata.
