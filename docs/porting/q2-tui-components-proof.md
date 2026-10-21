# Q2 component re-pin proof

Authority: [frozen source](source-lock.json). Contract: [components](../specs/tui-components.md).

Initial RED: `/tmp/pi-rust-repin-tui-components/red-inherited.log` reproduces component image iTerm2 bytes, markdown math, settings input, and stale editor oracle failures. `red-padding.log` proves absent Text.setPadding API. `sandbox-fd-red.log` proves missing fd inside the mandatory runner; fd exposure is separate commit `80e3c3ea`, execution remains blocked pending installed fd.

First implementation restores Text padding/JS whitespace, image fallback, iTerm2 bytes and frozen settings search. Coverage rows remain unpromoted pending whole-module proof. Later-only terminal image metadata/layout deletion and Markdown/editor re-pin remain open.

Markdown restores frozen token-render caching, same-text/padding/theme identity guards and theme invalidation; removes later-only math/tokenizer/transform options and public Marked/Tokens facade. Literal math remains in frozen differential corpus. User/assistant Markdown consumers no longer install transforms (frozen counterparts do not); constructor compatibility is left to Q13. Editor oracle now loads frozen tui.ts/TUI rather than absent tui-main-screen.ts. RED cache API: `red-markdown-cache-api.log`; GREEN execution currently environment-blocked by missing required fd (`green-components-1.log`).

## Old-only removal and Q13 consumer boundary

Deleted alt-screen-search, alt-screen-flash, HStack/VStack/Stack/ScrollView, layout/layout-node, TuiMainScreen/TuiAltScreen, ViewportTui and renderer writer branches, LaTeX renderer/tokenizers, terminal-image metadata/placement/crop registry, and their dedicated tests/oracles. The later-only physical-row overlay test in tab_width.rs is removed; frozen tab-width.test.ts contains only utility-width tests already covered in utils.rs. Retained frozen tests now instantiate TUI directly. Removed image-path helper moved to its frozen coding-agent render-utils consumer (Node URL adaptation), not a terminal-image public API.

Interactive consumer uses frozen TUI plus existing root compositor, not a fullscreen layout. Legacy renderer-selection/settings/extension signatures remain consumer data scaffolding for Q13; fullscreen switch is rejected, no alternate renderer is retained. The local ScrollViewScrollbar enum preserves settings serialization only; Q13 must remove non-frozen settings/extension surfaces, transformer constructor arguments, and finish interactive-mode re-pin. No claim of full Q13 parity.

Editor oracle targets frozen tui.ts/TUI. HistoryPrevious/Next actions and their later-only test are removed. Frozen top narrow scroll indicators contain truncateToWidth reset bytes; bottom indicators are untruncated and can exceed width (source bug, classified/pending parent decision). A direct frozen scenario and native exact-byte regression cover both branches. Existing native later-only expectations now use frozen semantics.

Evidence: `components-green-2.log` (all focused differentials except first editor mismatch), `editor-green-3.log` (editor/history/cache GREEN), `native-red-stale.log` (later-only native expectations), `autocomplete-fd-green.log` (22+3 pass), `sandbox-fd-green.log` (11 pass). All test execution uses mandatory guard. Coverage promotions remain pending final proofs.
