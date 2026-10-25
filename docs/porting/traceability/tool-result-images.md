# Removed old tool-result image normalization

Frozen `4f9e6f8a4` has no `tool-result-images.ts` or
`normalizeToolResultImages` anywhere in `packages/`. Its AgentSession after-tool
hook passes extension results through and performs no image resizing.
The obsolete Rust helper/tests and oracle operation were removed on 2026-10-25.
`agent_session::passes_through_oversized_tool_result_images_unchanged` asserts
exact original image bytes in the persisted conversation after a real tool
call, with default resizing settings. RED reproduced changed bytes.
No read-tool or user-attachment image processing was removed.
