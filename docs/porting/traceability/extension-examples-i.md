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
| `summarize.ts` | Pending | — |
| `ssh.ts` | Pending | — |

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

## Missing native API: interactive shell

Pinned `interactive-shell.ts:157–174` calls `tui.stop()`, inherited stdio
`spawnSync`, `tui.start()`, and `tui.requestRender(true)` inside `ui.custom`.
`crates/pi-coding-agent/src/core/extensions/types.rs:277–319` exposes only
`request_render`, terminal dimensions and mode on `ExtensionUiTuiBackend` /
`ExtensionUiTui`; no terminal release/reacquisition or forced-render argument.
PTY availability does not supply these extension-facing methods. Stop this
example rather than execute while TUI owns the terminal or add a core API.
