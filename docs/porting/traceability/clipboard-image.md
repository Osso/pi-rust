# coding-agent clipboard-image traceability

Historical whole-module table below targets `packages/coding-agent/src/utils/clipboard-image.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`; it is not new-pin acceptance. The bounded tmpdir consumer below is tested against common pin `4f9e6f8a4`.

The older whole-module proof used native fake processes rather than a source differential; the new bounded temp-root proof invokes the actual frozen module with mocked child_process commands. The port is proven at the real process boundary instead: `crates/pi-coding-agent/tests/clipboard_image.rs` re-executes the test binary with a PATH containing only fake `wl-paste`/`xclip`/`wslpath`/`powershell.exe` scripts, so the production code runs unmodified against recorded invocations.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `clipboard-image.ts:12` | Supported MIME list (png, jpeg, webp, gif) | `SUPPORTED_IMAGE_MIME_TYPES` |
| `clipboard-image.ts:14-17` | Timeouts 1000/3000/5000ms, 50MB buffer | `DEFAULT_LIST_TIMEOUT`/`DEFAULT_READ_TIMEOUT`/`DEFAULT_POWERSHELL_TIMEOUT`/`DEFAULT_MAX_BUFFER_BYTES` |
| `clipboard-image.ts:19-21` | `isWaylandSession`: `WAYLAND_DISPLAY` or `XDG_SESSION_TYPE=wayland` | `is_wayland_session`; focused `detects_wayland_sessions_from_display_or_session_type` |
| `clipboard-image.ts:23-25` | `baseMimeType` | `base_mime_type`; exercised by every MIME selection test |
| `clipboard-image.ts:27-39` | `extensionForImageMimeType` | `extension_for_image_mime_type`; focused `maps_image_mime_types_to_extensions` |
| `clipboard-image.ts:41-57` | `selectPreferredImageMimeType`: preference order, then any `image/*` | `select_preferred_image_mime_type`; focused `xclip_prefers_the_listed_image_type_then_falls_back_to_supported_types` |
| `clipboard-image.ts:59-62` | `isSupportedImageMimeType` | `is_supported_image_mime_type` |
| `clipboard-image.ts:68-84` | `convertToPng` for unsupported clipboard formats | `convert_image_bytes_to_png`; focused `converts_unsupported_clipboard_formats_to_png` (BMP → PNG) |
| `clipboard-image.ts:86-114` | `runCommand`: timeout, maxBuffer, non-zero exit or spawn error → not ok | `run_command`; every scenario asserts the exact argv of each fake tool |
| `clipboard-image.ts:116-135` | Wayland: `wl-paste --list-types`, then `--type <mime> --no-newline` | `read_clipboard_image_via_wl_paste`; focused `wayland_reads_wl_paste_and_never_touches_the_native_clipboard` |
| `clipboard-image.ts:137-148` | `isWSL`: `WSL_DISTRO_NAME`/`WSLENV`, else `/proc/version` | `is_wsl`; focused `wsl_uses_powershell_with_the_quoted_windows_path` |
| `clipboard-image.ts:155-205` | WSL PowerShell fallback: `wslpath -w`, quoted path, `ok`/`empty`, temp file cleanup | `read_clipboard_image_via_powershell`, `powershell_script`; focused WSL scenario asserts the script contains `$path = 'C:\Users\O''Hare\clip.png'` and no custom env var |
| `clipboard-image.ts:207-236` | X11: `TARGETS` listing, preferred type then supported types | `read_clipboard_image_via_xclip`; focused xclip scenarios |
| `clipboard-image.ts:238-250` | Native clipboard image (PNG) | `read_native_clipboard_image`; focused `non_wayland_reads_the_native_clipboard_without_spawning_tools` |
| `clipboard-image.ts:252-299` | Platform dispatch, Termux skip, WSL ordering, Wayland skip of native, final conversion | `read_clipboard_image`; focused Wayland/WSL/non-Wayland/Termux/no-source scenarios |

## Gaps and discrepancies

- **Gap:** `@mariozechner/clipboard` is a napi module; the port injects a `ClipboardModule` (production: `clipboard_native::clipboard()`), matching how the TypeScript tests mock the module. The native X11/Wayland syscalls themselves are not exercised (no display in CI); see `clipboard-native.md`.
- `readClipboardImage({env, platform})` becomes `read_clipboard_image(&ClipboardEnvironment, native)`; the environment is a value instead of reading `process.env`/`process.platform`, which is what makes the real-boundary tests possible.
- The WSL temp file name uses pid plus nanoseconds instead of `randomUUID()`.
- `spawnSync` timeouts kill with `SIGTERM`; `kill_on_drop` + `tokio::time::timeout` terminates the child with `SIGKILL`. Both are treated as "not ok".
- `readClipboardImageViaNativeClipboard` throws in the source when `hasImage()` throws; the Rust `has_image` returns `Result` and the error propagates the same way.

## Proof inventory

- Focused cases: 12.
- Fake-tool invocations asserted: `wl-paste --list-types`, `wl-paste --type image/png --no-newline`, `xclip -selection clipboard -t TARGETS -o`, `xclip -selection clipboard -t <mime> -o`, `wslpath -w <tmp>`, `powershell.exe -NoProfile -Command <script>`.

## Assigned tmpdir consumer (2026-10-24)

Frozen common-pin `clipboard-image.ts:162` uses `tmpdir()` for PowerShell staging. Native now uses the [shared Node authority](../../specs/node-tmpdir.md), retaining its existing filename behavior. The four-case actual-source/native root matrix and integration gates are recorded once in the [interactive consumer proof](interactive-mode.md#assigned-tmpdir-consumers-2026-10-24). This bounded proof does not revalidate the historical whole-module table or waive its listed discrepancies.
