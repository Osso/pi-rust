# Extension examples I: session/process

Pinned source: `532f7d12`, `packages/coding-agent/examples/extensions/`.
Native sources are embeddable `#[path]` modules, explicitly factory-registered;
none loads by default. Only JavaScript stack frames may be omitted.

| Source / entrypoint | Native implementation | Proof / status |
| --- | --- | --- |
| `git-merge-and-resolve.ts` default / `agent_end` | `examples/extensions/git_merge_and_resolve.rs::git_merge_and_resolve_extension` | `example_git_merge_and_resolve`: complete metadata, notifications, follow-up delivery options/messages, errors, file contents and git status against real pinned loader/runner |
| `findConflicts`, `formatRange`, `formatConflicts` | private native helpers | New/existing merge, malformed markers, single/multiline/empty sections; conflict instruction mutation rejected |
| `interactive-shell.ts` default / `user_bash` | Not shipped | Missing native terminal lifecycle API; see below |
| `handoff.ts` | Pending | — |
| `summarize.ts` default / `/summarize` | `examples/extensions/summarize.rs::summarize_extension` | `example_summarize`: ten cases, complete command/tool metadata, notifications, model requests, outputs/errors, custom renders at 32/80 columns, Enter/Esc/irrelevant input |
| `ssh.ts` default / flag, four tools, three events | Not shipped | Native custom-operation errors cannot propagate arbitrary Error messages; see below |

## Git proof

`tests/example_git_merge_and_resolve.rs` reuses shared A runner/oracle and E's
isolated git child. Native/TS sides get independent private repositories.
`GIT_CONFIG_GLOBAL=/dev/null`, `GIT_CONFIG_SYSTEM=/dev/null`, no real remotes.
Scenarios: non-repo, dirty tree, no upstream, clean merge, failed fetch, new
conflict, empty conflict section, multiline section, existing merge, malformed
conflict markers. Every scenario emits twice, covering unresolved re-delivery.
Full error strings are compared after only temp-root normalization.
Pinned per-file conflict-read errors are intentionally ignored, as in source.

Development logs and proof ledger: `/tmp/pi-rust-ext-examples-i/`.
RED: missing native example (01). GREEN: ten scenarios (02). Mutation:
`Resolve these conflicts.` → `Ignore these conflicts.` fails (03), reverted.

## Summarize proof

`extractTextParts`, `extractToolCallLines`, `buildConversationText`, and
`buildSummaryPrompt` map to `text_parts`, `conversation_text`, and command prompt
construction. Cases exercise empty/blank text; string/block content; ignored
images, thinking and tool results; assistant tool-only entries; numeric object
key order; missing model/auth; no UI/non-TUI; empty/model-error responses.
`showSummaryUi` maps to `Summary`: real native Markdown/Text/DynamicBorder renders
match pinned TS components at 32 and 80 columns. Enter/Esc close; unrelated input
does not. Rc TUI components are ephemeral per render at the Send+Sync bridge,
matching the existing preset example pattern.

Model calls run through real registries with native/pinned faux providers.
Capture exact model provider/id, context and example-supplied completion options;
validate UUIDv7 and normalize only UUID and message clock. No credentials/network
or file-backed sessions. No-auth uses the unrefreshed in-memory auth snapshot;
configured scenarios refresh native provider auth before invoking the command.
RED: 12; GREEN: 16; notification mutation rejected: 17, reverted.

## Missing native API: SSH arbitrary-operation errors

The prototype passed five flag variants and every entrypoint's successful path
(log 08), then failed expanded error tests (log 11): native read returned
`EIO: SSH failed (7): scripted failure\\n, access '<tmp>/fail.txt'`; pinned TS
returned `SSH failed (7): scripted failure\\n`. Both used the same fake `ssh`
executable, which recorded argv/stdin and exited 7 with scripted stderr.

`ReadOperations` (`core/tools/read.rs:45–68`), `WriteOperations`
(`core/tools/write.rs:43–46`) and `EditOperations` (`core/tools/edit.rs:50–53`)
accept only `io::Result`. Callers unconditionally apply Node filesystem wrapping
(e.g. `read.rs:230–232,344–346`, `write.rs:161–168`), including arbitrary remote
errors. Missing API: custom-operation error pass-through, distinct from errno
filesystem failures. Stop this example; do not add core APIs or brittle string
unwrapping in the example. Prototype and its tests/oracle removed; failure logs
retained in the ledger. No SSH port is claimed.

## Missing native API: interactive shell

Pinned `interactive-shell.ts:157–174` calls `tui.stop()`, inherited stdio
`spawnSync`, `tui.start()`, and `tui.requestRender(true)` inside `ui.custom`.
`crates/pi-coding-agent/src/core/extensions/types.rs:277–319` exposes only
`request_render`, terminal dimensions and mode on `ExtensionUiTuiBackend` /
`ExtensionUiTui`; no terminal release/reacquisition or forced-render argument.
PTY availability does not supply these extension-facing methods. Stop this
example rather than execute while TUI owns the terminal or add a core API.
