# Plain-text clipboard copy

Authority: frozen `utils/clipboard.ts`,4f9e6f8a4,127lines.

- Prefer native clipboard writes outside Linux; catch rejection and use platform tools. Linux deliberately skips the native addon. A successful local write returns without OSC52. SSH_CONNECTION, SSH_CLIENT or MOSH_CONNECTION truthiness forces the OSC52 attempt even after native/tool success.
- Use pbcopy, clip, Termux then Wayland/X11 exactly as frozen. X11 tries xclip then xsel only on synchronous command failure. Copy execSync input uses Node UTF-8, ignored output and a5000ms whole-call deadline; the `which wl-copy` preflight has no configured timeout.
- Wayland spawns wl-copy with piped stdin and ignored output; write/end stdin and unref without waiting for exit or adding a timeout. Ignore stdin errors as frozen. A later nonzero child exit does not trigger X11/OSC52. Asynchronous spawn errors remain unobserved process errors through the shared uncaught authority, not synchronous fallbacks; original Node Error fields remain exact.
- OSC52 encodes Node UTF-8 as Base64 and writes ESC]52;c;payload BEL. Exactly100000 encoded characters is allowed; larger payloads are not written. Failure of every local/OSC52 mechanism rejects with `Failed to copy to clipboard`. Unhandled stdout errors retain native Node fields and reach the uncaught authority.
- No text-reading API, right-click clipboard paste or image-paste text fallback exists at this pin. Image paste retains only the frozen image-path behavior.

Windows process execution is the standing gap; a Linux fixture can prove win32 command selection only. Native clipboard packaging is a separate module. Independent task/timer teardown and next-tick ordering are scheduling gaps: private proofs keep the runtime alive to observe stdin/error completion, and compare original uncaught fields rather than promise-result file timing. No live display or real clipboard access is required for proof.
