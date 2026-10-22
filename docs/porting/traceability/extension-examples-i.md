# Extension examples I: session/process
## Common-pin request/model reconciliation (2026-10-22)

Frozen handoff.ts:136–139 passes only resolved auth and loader signal, no cache-retention override or generated session ID. Removed the native extras; fixture captures actual optional values rather than demanding a UUID absent from source. RED `test-handoff-native-options.log` retained.

Frozen summarize.ts:163 selects the static catalog openai/gpt-5.2; native now uses compat.get_model, not registry.find. `missingModel` removes the custom runtime catalog model only; the frozen built-in still exists, so that fixture must exercise static-catalog behavior rather than retain the obsolete no-model expectation. Source :181–189 also omits generated session ID and cache retention; native extras removed. Assertions remain full-result comparisons.

Direct completion resolves auth then dispatches the registered provider, matching source compat.complete rather than ModelRuntime.prepare_request's pre-dispatch cancellation. The native fixture now models stored-key auth separately from the faux provider's intentionally keyless auth; its one-shot UI thread waits for explicit dispatch notification before closing after Escape, matching Node's still-live host. Cancellation remains immediate; no timer delay or cancelled-result assertion is removed.

Proof: `test-example_summarize-stored-auth-public.log` GREEN 1/1 (ten scenarios); handoff reaches final rich-compaction scenario after cancellation passes. Remaining rich image marker RED belongs to shared coding-agent convertToLlm and is main-owned, not patched into this example.

Historical pinned source: `532f7d12`, `packages/coding-agent/examples/extensions/`.
Native sources are embeddable `#[path]` modules, explicitly factory-registered;
none loads by default. Only JavaScript stack frames may be omitted.

| Source / entrypoint | Native implementation | Proof / status |
| --- | --- | --- |
| `git-merge-and-resolve.ts` default / `agent_end` | `examples/extensions/git_merge_and_resolve.rs::git_merge_and_resolve_extension` | `example_git_merge_and_resolve`: complete metadata, notifications, follow-up delivery options/messages, errors, file contents and git status against real pinned loader/runner |
| `findConflicts`, `formatRange`, `formatConflicts` | private native helpers | New/existing merge, malformed markers, single/multiline/empty sections; conflict instruction mutation rejected |
| `interactive-shell.ts` default / `user_bash` | `examples/extensions/interactive_shell.rs::interactive_shell_extension` | Done: `example_interactive_shell`; [core UI API/proof](core-ui-input.md#interactive-shell--done) |
| `handoff.ts` default / `/handoff` | `examples/extensions/handoff.rs::handoff_extension` | `example_handoff`: fourteen complete-result scenarios including original-context invalidation, replacement UI, parent tracking and compaction; metadata mutation rejected |
| `summarize.ts` default / `/summarize` | `examples/extensions/summarize.rs::summarize_extension` | `example_summarize`: ten cases, complete command/tool metadata, notifications, model requests, outputs/errors, custom renders at 32/80 columns, Enter/Esc/irrelevant input |
| `ssh.ts` default / flag, four tools, three events | `examples/extensions/ssh.rs::ssh_extension` | `example_ssh`: nine complete pinned-TS scenarios, fake-only remote, real tool-error TUI and 11 rejected SSH mutations; [core proof](core-ssh-errors.md) |

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

## Handoff proof

`entryToMessage` / `getHandoffMessages` map to `entry_message` /
`handoff_messages`: message entries, ignored entries, latest compaction summary,
first-kept retained range, missing first-kept ID, post-compaction messages.
Normalize numeric JSON object keys to JavaScript enumeration order before
native conversation serialization; model input preserves source order. Use the
shared agent serializer's argument hook with JS stringify and the coding-agent
1000-unit limit. This preserves large-number formatting and nested numeric-key
order without changing shared core files. Regression 25 caught the default
coding-agent serializer rendering `1e20` instead of JS `100000000000000000000`;
classify that existing shared-helper mismatch as a discrepancy outside this
slice. The example uses existing public serializer configuration, not a fallback.

Command cases: non-TUI, no model, blank goal, no conversation, cancelled draft,
nonempty/empty edited draft, cancelled session replacement, aborted model
response, Escape loader cancellation, empty response, and compaction kept ranges
at 0/2/missing. Original runtime is invalidated before `withSession`; native and
TS replacement runners have separate UI traces. Compare complete metadata,
notifications, loader renders at 32/80 columns, completion request, editor draft,
new-session parent/options and replacement-editor/notification effects.
Both managers are in-memory; explicit parent file paths stay under temp roots.

BorderedLoader has no animation frames in pinned TS. Its native Rc components
are ephemeral per render, while the cancellation token/done callback persist at
the Send+Sync extension bridge. Completion uses the real registry/faux provider.
RED: 18; GREEN: 22/23; metadata mutation rejected: 24, reverted.

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

## SSH error boundary: resolved

The historical prototype exposed unconditional Node wrapping of remote operation
errors. The [core SSH slice](core-ssh-errors.md) moves filesystem formatting into
local operations, proves custom error propagation and retained local messages,
and ships the complete opt-in example. No new core API or error-string unwrapping
was needed. That traceability doc owns current proof and remaining interface limits.

## Missing native API: interactive shell

Historical blocker resolved by the [core UI input slice](core-ui-input.md).
Native stop/start and forced request-render now support the complete example;
that traceability page owns the API mapping, acceptance matrix and proof limits.
