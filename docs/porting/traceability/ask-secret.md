# ask-secret traceability

Source: `packages/coding-agent/extensions/ask-secret/src/index.ts` (169 lines).
Rust: `crates/pi-coding-agent/src/bundled_extensions/ask_secret.rs`.
Tests: `crates/pi-coding-agent/tests/bundled_extensions_tools.rs` (`ask_secret_matches_typescript`).

| Source symbol / branch | Rust item | Proof |
| --- | --- | --- |
| default `askSecretExtension(pi)` | `ask_secret_extension` | registration differential |
| `createAskSecretToolDefinition(options)` | `create_ask_secret_tool_definition` | registration differential; injected provisioner |
| schema/description/label/promptSnippet/approvalRequired/sequential | `ask_secret_schema`, `ToolDefinition` literal | registration differential (byte-equal JSON) |
| `validateRequest` / `validateArgument` | `validate_request`, `validate_argument` | control-character and empty checks; differential error text |
| relative-path rule (`startsWith("/")`, `..` segment) | `validate_request` | unit branch |
| non-TUI/unavailable result | `execute_ask_secret` mode/hasUI branch | differential step 2 |
| username/password prompts (`secret: true` on password) | `ExtensionUiDialogOptions` | differential `inputs` calls |
| cancellation results | `cancelled_result` | differential step 4 |
| `provisionBrowserCredential` argv/stdin/exit-code/abort | `provision_browser_credential` | injected provisioner differential; abort/exit branches implemented |
| `clearStringReference` | absent (no-op in source) | source comment: JS strings immutable |

Gaps / discrepancies:
- The default provisioner spawns `authsudo`; tests never run it. Spawn failures map
  `NotFound`/`PermissionDenied` to Node `spawn <cmd> ENOENT`/`EACCES`; other kinds use
  the OS message.
- Abort kills the child immediately; the source kills on abort and then awaits the
  exit code before throwing the same message.
- Invalid JS runtime input types (non-string fields) are rejected with a Rust error;
  typed API exclusion per the 2026-08-12 decision.
