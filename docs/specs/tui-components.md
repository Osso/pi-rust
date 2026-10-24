# Frozen TUI components (Q2)

Authority: [source lock](../porting/source-lock.json), commit `4f9e6f8a4`.

Text supports changing padding with cache invalidation and JavaScript whitespace emptiness. Image fallback is complete literal text, including a supplied filename, independent of hyperlinks and render width. iTerm2 encoding has no later-only `size` field. Capability detection has no Windows-console special branch. Settings search removes spaces from multi-character input; a single Space always activates the selected item, including during search.

Remaining Q2 component/deletion work is tracked in the [Q2 proof](../porting/q2-tui-components-proof.md). Cell/image dimensions remain u32 (approved numeric-domain gap); typed invalid-JS inputs remain excluded. No claim of complete component parity until the module proofs finish.

Markdown exports only frozen options (ordered-list markers/backslash escapes); LaTeX and source transforms are absent. Streaming render caches top-level tokens by next token type, width and serialized fingerprint. Same text/padding/theme retains caches; invalidate/theme replacement clears token caches. Default style prefixes are lazy and cached, with theme replacement invalidation.

Spacer default is one empty row; explicit zero remains zero rows. SettingsList skips descriptions only when absent or empty, not whitespace-only. Text and Box default padding is 1/1. Input render widths<=2 return the prompt only. Empty normalized SelectList descriptions omit description columns; custom truncators receive signed i64 widths, including internally derived negative values at terminal widths0..3. Output truncation still clamps as frozen.

Editor top and bottom scroll indicators must fit viewport width while retaining literal truncation/color-reset bytes. User2026-10-24 approved a separately pinned TS+Rust correction for the frozen bottom-indicator overwidth bug; [delta provenance and exact-frame proof](../porting/traceability/editor.md#approved-bottom-indicator-delta-2026-10-24). The common pin stays unchanged. Terminal-image numeric-domain restrictions include nonfinite and zero-cell calculation outputs, confirmed by parent against October21 user approval.

All later-only fullscreen/layout/LaTeX/metadata modules are removed. Coding-agent uses frozen TUI/root-compositor; retained legacy settings/extension signatures are a Q13 consumer gap, not an alternate TUI implementation.
