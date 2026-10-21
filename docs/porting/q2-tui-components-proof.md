# Q2 component re-pin proof

Authority: [frozen source](source-lock.json). Contract: [components](../specs/tui-components.md).

Initial RED: `/tmp/pi-rust-repin-tui-components/red-inherited.log` reproduces component image iTerm2 bytes, markdown math, settings input, and stale editor oracle failures. `red-padding.log` proves absent Text.setPadding API. `sandbox-fd-red.log` proves missing fd inside the mandatory runner; fd exposure is separate commit `80e3c3ea`, execution remains blocked pending installed fd.

First implementation restores Text padding/JS whitespace, image fallback, iTerm2 bytes and frozen settings search. Coverage rows remain unpromoted pending whole-module proof. Later-only terminal image metadata/layout deletion and Markdown/editor re-pin remain open.

Markdown restores frozen token-render caching, same-text/padding/theme identity guards and theme invalidation; removes later-only math/tokenizer/transform options and public Marked/Tokens facade. Literal math remains in frozen differential corpus. User/assistant Markdown consumers no longer install transforms (frozen counterparts do not); constructor compatibility is left to Q13. Editor oracle now loads frozen tui.ts/TUI rather than absent tui-main-screen.ts. RED cache API: `red-markdown-cache-api.log`; GREEN execution currently environment-blocked by missing required fd (`green-components-1.log`).
