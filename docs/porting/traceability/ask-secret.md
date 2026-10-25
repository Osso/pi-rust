# ask-secret traceability

Source: `packages/coding-agent/extensions/ask-secret/src/index.ts` (169 lines).
Rust: `crates/pi-coding-agent-extensions/src/bundled_extensions/ask_secret.rs`.
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

## Current-pin browser fields and file secrets (2026-10-25)

Native request schema now matches frozen `path/label` or `record/fields`, with
ordered text/email/password prompts and masked passwords. Provisioning uses
`--fields-json` plus the UTF-8 JSON values array on stdin; no values are returned.
The obsolete username/password selector interface is replaced, not retained.
File requests validate destinations before prompting and before rename, create
owner-only parents, write/sync a new owner-only UUID-named temporary file, then
rename and remove the temporary path. Payload bytes are zeroed after use.
`renameFile` remains the same source injection seam for failure tests.

RED: old browser registration metadata differs from frozen; a real source file
write succeeds while native rejects its missing record. New differential checks
exact returned/UI data, file bytes including newline, modes 0600/0700 and no
remaining temporary files. Other realistic persistence/failure cases are pending
focused proof, not claimed here.

gap: std File drop close-fault reporting and adversarial JSON type errors are
not equivalent to Node FileHandle faults; ordinary paths/errors remain explicit.

Current-pin focused matrices now include `ask_secret_validation_and_masked_cancellation_match_frozen`
(14 concrete validation requests plus cancellation at either browser field),
`ask_secret_file_refusal_cancellation_abort_and_rename_failure_match_frozen`
(directory/symlink refusal before UI, cancelled/pre-aborted writes with no new
parent, failed rename retaining original bytes and removing temporary files),
and `ask_secret_atomic_overwrite_matches_frozen` (existing file changes from
0644 to 0600 with exact replacement bytes). Runtime source is unchanged by
these fixture additions; `renameError` and aborted signals use frozen's own
renameFile and signal seams.

The first expanded matrix exposed a fixture omission, not runtime drift:
source serialized the supplied AbortSignal as `{}` in dialog options while
the native calls recorder discarded signal/timeout entirely. The recorder now
keeps all present dialog option fields (signal as its exact JSON `{}`), without
changing runtime options or weakening source comparisons.
