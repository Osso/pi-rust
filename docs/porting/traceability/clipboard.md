# coding-agent clipboard traceability

Pinned source: `packages/coding-agent/src/utils/clipboard.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential test skipped: the module spawns platform clipboard tools, reads `process.platform`, and writes OSC 52 to stdout; the TypeScript tests mock all of them. The port is proven at the real boundary: `crates/pi-coding-agent/tests/clipboard.rs` re-executes the test binary with a PATH of fake `pbcopy`/`wl-copy`/`which`/`xclip`/`xsel`/`termux-clipboard-set` scripts, and the parent asserts both the fake-tool invocations and the OSC 52 sequences the child wrote to stdout.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `clipboard.ts:11-18` | `copyToX11Clipboard`: `xclip -selection clipboard`, then `xsel --clipboard --input` | `copy_to_x11_clipboard`; focused `linux_x11_copy_falls_back_from_xclip_to_xsel` |
| `clipboard.ts:20` | OSC 52 payload cap 100,000 encoded characters | `MAX_OSC52_ENCODED_LENGTH`; focused `oversized_osc52_payloads_fail_instead_of_writing` |
| `clipboard.ts:22-24` | `isRemoteSession`: `SSH_CONNECTION`/`SSH_CLIENT`/`MOSH_CONNECTION` | `is_remote_session`; focused `detects_remote_sessions` |
| `clipboard.ts:26-33` | `emitOsc52`: base64, size guard, `\x1b]52;c;<data>\x07` | `emit_osc52`; focused remote/fallback scenarios compare the exact sequence |
| `clipboard.ts:35-40` | Read options: utf8, 50MB, 5000ms | `READ_TIMEOUT`/`READ_MAX_BUFFER_BYTES` |
| `clipboard.ts:42-49` | `readWaylandClipboardText`: `wl-paste --no-newline --type text` | `read_wayland_clipboard_text`; focused Wayland text scenarios assert the argv |
| `clipboard.ts:51-71` | `readClipboardText`: Wayland first (empty result is final), else native, errors → `null` | `read_clipboard_text`; focused 5 cases incl. the #7248 regression |
| `clipboard.ts:73-83` | Prefer native writes; never on Linux; remember `copied` | `copy_to_clipboard`; focused `local_native_copy_skips_osc52_and_shell_tools` |
| `clipboard.ts:85-89` | Remote sessions always continue to OSC 52 | focused `remote_native_copy_emits_osc52_after_the_native_write` |
| `clipboard.ts:91-137` | Platform shell fallbacks (`pbcopy`, `clip`, Termux, Wayland `which wl-copy` + `wl-copy`, X11) | `copy_on_linux`, `run_shell_with_input`, `run_program_with_input`; focused `local_shell_fallback_success_skips_osc52`, `linux_wayland_copy_uses_wl_copy`, `failed_wl_copy_falls_back_to_xclip_when_x11_is_available`, `linux_termux_copy_uses_termux_clipboard_set` |
| `clipboard.ts:139-144` | OSC 52 when remote or nothing copied | `emit_osc52` in `copy_to_clipboard`; focused `osc52_fallback_is_used_when_native_and_shell_tools_fail` |
| `clipboard.ts:146-148` | Nothing copied → throw `Failed to copy to clipboard` | `ClipboardError::new("Failed to copy to clipboard")`; focused oversized case |

## Discrepancies

- The source reads `process.env`/`process.platform` inside the module and mocks them in tests. The port takes a `ClipboardEnvironment` value and an optional `ClipboardModule`; production callers pass `ClipboardEnvironment::from_process()` and `clipboard_native::clipboard()`.
- `execSync("...")` uses `/bin/sh -c` on Unix (`cmd.exe /C` on Windows); the port does the same explicitly instead of relying on Node's shell selection.
- `wl-copy` is spawned and awaited through the same 5000ms timeout as `execSync`; Node's `spawn` path has no timeout.
- OSC 52 is written with `std::io::stdout()`; the TypeScript tests intercept `process.stdout.write`.

## Proof inventory

- Focused cases: 15.
- Fake-tool invocations asserted: `pbcopy`, `which wl-copy`, `wl-copy`, `xclip -selection clipboard`, `xsel --clipboard --input`, `termux-clipboard-set`, `wl-paste --no-newline --type text`.
