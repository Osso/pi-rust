# external-editor.ts

Source: `coding-agent/src/modes/interactive/external-editor.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/external_editor.rs`

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `ExternalEditorOptions` | `ExternalEditorOptions` | `interactive_workflows::external_editor_*` |
| `ExternalEditorResult` union | `ExternalEditorResult` enum | same tests |
| `mkdtempSync(tmpdir, "pi-editor-")` | `tempfile::Builder::prefix("pi-editor-").tempdir()` | private-directory test (parent, prefix) |
| mkdtemp 0700 permissions | `set_private_directory_permissions` | test asserts `mode & 0o077 == 0` (Rust `tempdir` creates 0755, so the mode is set explicitly) |
| `prompt.md` write/read | `std::fs::write` / `read` returning `Err(String)` with Node fs text | content capture assertions; error path reaches the process-level uncaught handler ([throw-audit](throw-audit.md)) |
| launch banner on stdout | `stdout().write_all` (crate denies `print_stdout`) | observed in test output |
| `command.split(" ")` + args | `spawn_editor` | fixture command with flags |
| spawn error → `failed` | `spawn_editor` returns `None` | code-mapped (fixture always spawns) |
| exit code != 0 → `failed` | `status.success()` | `--fail` test |
| trailing `\n` removed once | `strip_suffix('\n')` | `edited\n` → `edited`; empty test |
| `finally` cleanup | `TempDir` drop | directory-removed assertions |

Gaps: Windows `shell: true` uses `cmd /d /s /c`; not exercised on Linux. A missing editor binary (spawn error) has no test because the fixture path always exists.

## Re-pin gap: temporary file lifecycle (record only)

Frozen `interactive-mode.ts:5453–5507` writes directly to a `pi-editor-${Date.now()}.pi.md` file in `tmpdir()`, removes it in `finally`, then restarts the TUI. The extracted Rust helper still creates a private temporary directory and `prompt.md` before invoking the editor. This older lifecycle is not new-pin parity evidence. Q18 changes only UTF-16 command/content/error propagation and filesystem/argv encoding; replacing the temp lifecycle is deferred, not approved as parity.
