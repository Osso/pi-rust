# Q2 component re-pin proof

Authority: [frozen source](source-lock.json). Contract: [components](../specs/tui-components.md).

Initial RED: `/tmp/pi-rust-repin-tui-components/red-inherited.log` reproduces component image iTerm2 bytes, markdown math, settings input, and stale editor oracle failures. `red-padding.log` proves absent Text.setPadding API. `sandbox-fd-red.log` proves missing fd inside the mandatory runner; fd exposure is separate commit `80e3c3ea`, execution remains blocked pending installed fd.

First implementation restores Text padding/JS whitespace, image fallback, iTerm2 bytes and frozen settings search. Coverage rows remain unpromoted pending whole-module proof. Later-only terminal image metadata/layout deletion and Markdown/editor re-pin remain open.
