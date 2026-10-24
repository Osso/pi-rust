# clipboard.ts

Frozen `packages/coding-agent/src/utils/clipboard.ts`,4f9e6f8a4,127lines. Native `utils/clipboard.rs`. [Contract](../../specs/clipboard.md).

| Source symbols / branches | Native mapping / proof scope |
|---|---|
| NativeClipboardExecOptions:6–10; copyToX11Clipboard:12–18 | shared spawn_sync with input, explicit stdio and5000ms whole-call timeout; xclip then xsel on caught failure; private actual-source tool argv/input/status matrix |
| MAX_OSC52_ENCODED_LENGTH:20; emitOsc52:26–33 |100000encoded cap, shared Node UTF8/Base64; exact cutoff, oversized rejection, raw UTF16 and stdout EPIPE private-process cases |
| isRemoteSession:22–24 | three truthy env fields; empty strings remain false; native/tool successes still emit OSC52 remotely |
| copyToClipboard:35–66 | Linux skips native; nonLinux native success/error and local/remote ordering; injected addon boundary and OS-platform seam, not copy-method replacement |
| execSync dispatch:68–91,104–113 | pbcopy/clip/Termux and Wayland/X11 environment truthiness; shared sync input/stdio timeout authority; no separate process runner |
| which/spawn/stdin/unref:92–103 | preflight no explicit timeout; shared spawn authority, independent stdin write/end task, no wait/status/timeout; stdin errors deliberately ignored; original asynchronous ENOENT/EACCES deferred to shared uncaught authority |
| final OSC52/failure:118–126 | exact payload/terminator, copied-or-OSC result, original failure message; stdout Error fields remain uncaught |

## Genuine REDs before production edits

Artifacts under `/tmp/pi-rust-repin-lossless-interactive/`:

| Observation | Artifact |
|---|---|
| wl-copy exits27: frozen only invokes which/wl-copy; native additionally invokes xclip | clipboard-text-wl-copy-red.log |
|2MiB input blocked6s: frozen whole-call5000ms timeout fails copy; native starts its timer after stdin delivery and succeeds | clipboard-text-timeout-behavior-red.log |
| which reports a missing wl-copy available: frozen emits uncaught spawn ENOENT/exit1; native silently falls back/exit0 | clipboard-text-async-spawn-red.log |
| closed stdout at the actual copy boundary: source uncaught EPIPE/exit1; native ignores write failure/exit0 | clipboard-text-stdout-error-behavior-red.log |
| no image, but text clipboard available: frozen image-only handler leaves editor unchanged; native inserts old-only text | clipboard-text-image-only-behavior-red.log |

Initial image fixture had a set_text type error; corrected without production edits. Initial closed-stdout fixture failed in libtest before copy; corrected with an explicit ready/release handshake and private-process exit, not treated as a behavioral RED. The first timeout filter was stripped by test-sandbox's environment whitelist and re-ran the wl-copy case; the separate timeout test above is the valid proof.

## Old-only deletions and discrepancies

Frozen module has no readClipboardText/readWaylandClipboardText exports or helpers. Delete native read_clipboard_text/read_wayland and their50MiB/timeout capture runner. Remove interactive right-click callback/helper and image-paste text fallback. Parent granted removal-only tui.rs API work; current tui.rs already has no right-click API, so no tui.rs edit is needed. Retain image-only consumer behavior and tmpdir/image/share proof.

Delete five old-only read tests (native text, empty/unavailable text, Wayland preference, empty Wayland no fallback, missing Wayland native fallback). Replace the legacy test asserting nonzero wl-copy falls back to X11 with the opposite frozen-derived assertion, backed by the actual-source differential; no other assertion is loosened. Legacy Wayland fixture keeps its runtime alive until its unref helper starts, without making copy await the process.

**DISCREPANCY:** frozen marks copied immediately even if wl-copy later exits nonzero, or emits an unhandled asynchronous spawn failure after successful which. Reproduce literally; never convert those failures to synchronous X11 fallback. Frozen stdin.on(error) ignores all stdin errors, despite its comment naming EPIPE specifically.

## Explicit boundaries

- Windows native process behavior is the existing project gap; the private Linux fixture proves win32 branch command selection only.
- Native clipboard loading/packaging is separately tracked in clipboard-native.ts; no live addon/display access is used here. Its old get_text trait method is not a clipboard.ts consumer after deletion.
- Independent producer/task/timer teardown and Node next-tick vs promise interleaving are approved scheduling gaps. Private copy fixtures preserve a live runtime while the unref stdin task drains. For an unhandled async Error, compare exact original fields/exit/tool effects, not whether the promise-result report was written before termination. No native waits or recovery fallback are added to copy itself.

## Whole-module proof (2026-10-24)

`clipboard-text-module-green.log`:4/4 actual-source process tests GREEN at a1f9166b,26 concrete cases. Two added normal-success/remote wl-copy source cases are included in the final matrix proof. `clipboard-text-image-only-green.log`: actual source/native UI consumer1/1 GREEN, four operations (powershell/image/image-text/share) ×four mutable tmpdir environments. `clipboard-text-regressions-green.log`:11/11 retained copy/helper regressions GREEN. Assertions preserve complete outcome/errors, actual argv/order and stdin bytes, native-sink bytes and exact OSC52 payloads; no console/text/provider normalization. Real display/native-addon operations are replaced only at external OS/addon boundaries.

Removed source-absent callback/options direct mechanical test consumer `interactive_crash.rs`; all-target workspace type proof covers that API deletion. No tui.rs change, no new dependency, no new process authority. Scoped checkpoint gate logs and exact revision coverage are maintained in clipboard-text-proof-ledger.md under the artifact directory above.
