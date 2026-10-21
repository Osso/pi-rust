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

Whole-component audit found zero-argument Text/Box defaults (padding1/1) and lazy Markdown default style context differed; RED in `red-defaults-heading.log` and `red-box-default.log`. Added frozen lifecycle differential covering token reuse, same-theme identity, content-width invalidation, theme switch, invalidation, no foreground callbacks in headings, and constructor defaults.

First all-targets proof (`tui-all-targets-1.log`) leaves only the later-only physical-tab test and four migrated tui_render cases. Three shrink assertions were stale main-screen expectations; frozen tests require unchanged fullRedraws. Forced redraw exposed inherited reset bug: Rust cleared previous Kitty IDs, frozen renderPendingState retains them. Existing frozen test was RED; remove that clear so the next full redraw deletes previous image IDs. This is a migration-exposed core parity fix, not an approved deviation.

Input prompt-width and SelectList JS trim/empty description RED: `red-input-select.log`. Terminal-image Node ASCII high-bit masking and JS exponent/negative-zero formatting RED: `red-image-ascii-numbers.log`, `red-webp-ascii.log`; new cases added to frozen differential corpus.

Consumer RED in `consumers-2.log`: five transformer expectations have no frozen counterpart and were removed; existing literal text/OSC tests retained. Fullscreen-only worker crash assertion moved to frozen regular TUI while retaining Tokio worker/process restoration coverage. RPC example oracle injected removed TuiMainScreen instead of TUI; mock binding corrected. Settings search fixture strips spaces instead of activating each word boundary; image row-order fixture explicitly enables Kitty capability.

SelectList signed callback widths are now i64, with exact subtraction before callback and clamping only at terminal truncation. Widths 0..4 yield callback widths -4..0. RED `red-select-signed.log`, direct frozen observation `select-signed-frozen-node.log`, native+frozen differential in repin_components. Parent classified numeric nonfinite/zero-cell and >u32 terminal-image outputs as approved 2026-10-21 restrictions.
