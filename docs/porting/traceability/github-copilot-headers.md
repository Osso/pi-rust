# github-copilot-headers traceability

Source: `packages/ai/src/api/github-copilot-headers.ts`.
Rust: `crates/pi-ai/src/api/github_copilot_headers.rs`.
Tests: `crates/pi-ai/tests/github_copilot_headers.rs` (3 behavioral), `crates/pi-ai/tests/differential_github_copilot_headers.rs` (16 requests vs pinned TS).

## Symbols → proof
| TS | Rust | Test |
|---|---|---|
| `inferCopilotInitiator` (`"user" \| "agent"`) | `infer_copilot_initiator` → `CopilotInitiator` | `infers_the_initiator_from_the_last_message` |
| `hasCopilotVisionInput` | `has_copilot_vision_input` | `detects_images_in_user_and_tool_result_content` |
| `buildCopilotDynamicHeaders` | `build_copilot_dynamic_headers` | `builds_dynamic_headers_with_optional_vision_flag` |

## Branch notes
- Empty message list → `user` (TS `last && last.role !== "user"` is falsy).
- Last message `assistant` or `toolResult` → `agent`.
- Images are detected only in user block content and tool-result content, never in assistant messages or string user content.
- Header map is `BTreeMap`; TS insertion order is not semantic.

## Gaps
- None.

## Discrepancies
- None.
