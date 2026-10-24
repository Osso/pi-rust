# Clipboard image

Frozen `utils/clipboard-image.ts` (301 lines), common pin4f9e6f8a4, governs the whole module.

- Termux short-circuits. Linux Wayland/WSL tries wl-paste then xclip. Other headless Linux tries wl-paste only. WSL then tries PowerShell. If still absent and not Wayland, try native clipboard; never add a native→xclip fallback. Non-Linux uses native clipboard.
- MIME preference png/jpeg/webp/gif, then first image/*, retaining trimmed raw MIME in command argv. JS trim/lowercase/truthiness and UTF-16 remain exact. Unsupported bytes use the frozen Photon conversion behavior; unavailable/failed conversion yields null.
- Tools use actual process.env, not the dispatch environment option. List/read/PowerShell timeouts are1000/3000/5000ms; maxBuffer is50MiB. Use the [single sync authority](child-process-sync.md). Command result failures produce null; validation throws reject, except inside the PowerShell try/catch.
- PowerShell stages `pi-wsl-clip-<UUIDv4>.png` in the current Node tmpdir. Double apostrophes in the Windows path; require trimmed output exactly `ok`, nonempty bytes, and unconditional best-effort unlink. Random values are compared by v4 format, not equality.
- Native no-image/empty-image returns null; native failure rejects. Do not access a real clipboard during tests.

Proof: `tests/differential_clipboard_image.rs`, actual-source oracle and private fake tool process fixture. Historical old-pin notes are not new-pin evidence.
