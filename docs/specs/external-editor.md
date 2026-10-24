# External editor

Match the inline frozen `interactive-mode.ts:5447–5507` at the [common pin](../porting/source-lock.json). There is no separate frozen `external-editor.ts` module.

## Contract

- Read the editor command; an empty command warns and returns. Read expanded editor text when available, otherwise ordinary text.
- Join `os.tmpdir()` with exactly `pi-editor-${Date.now()}.pi.md`; no private directory or `prompt.md` lifecycle.
- Inside the protected flow, write UTF-8 content before stopping the TUI. Emit the literal launch/resume banner, split the command on spaces preserving empty arguments, and asynchronously spawn with inherited stdio.
- A child error, signal or nonzero status leaves editor content unchanged and adds no failure diagnostic. On status0, read UTF-8 and remove exactly one trailing newline before replacing content.
- Finally, attempt unlink while ignoring cleanup errors, restart the TUI, then request forced rendering. Preserve original operation errors unless restart itself throws.
- Preserve human UTF-16 until Node's filesystem/argv/stdio encoding boundaries; keep original native error fields.

Windows shell execution remains the project-wide recorded gap, not POSIX proof.

## Proof

`tmpdir_external_editor_matches_frozen_inline_flow` uses isolated native and actual frozen method execution, real TUI terminal lifecycle snapshots and a real fake-editor process. It validates the physical temp parent and literal filename pattern/time range before normalizing only the clock nonce for exact trace comparison. The process heartbeat distinguishes asynchronous waiting from blocking execution. No real user editor or clipboard runs.

[Traceability](../porting/traceability/interactive-external-editor.md).
