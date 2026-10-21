# Q1 frozen TUI core re-pin proof

Verified 2026-10-21 against immutable `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`, selected by [source-lock.json](source-lock.json). Worktree: `/home/osso/.worktrees/pi-rust-repin-tui-core`, branch `repin/tui-core`. Follow-up fixes and proof at `e86a3210`; master `166eac08` merged into this branch at `ba9f7042` (only manual conflict: coverage-count header). No frozen source edits, push, canonical merge or delegation.

## Module coverage

| Frozen module | Function/export audit and proof | Classification |
|---|---|---|
| `src/index.ts` | concrete root TUI constructor/render; old-only root exports removed, coding-agent consumers migrated; root-renderer mutation killed; [trace](traceability/tui-index.md) | done for export mapping, not component implementations |
| `src/keybindings.ts` | all manager methods/defaults/global registry; seven-config differential plus direct shared mutation/reset/replacement identity; Arc-backed state; [trace](traceability/keybindings.md) | done within typed API |
| `src/stdin-buffer.ts` | complete sequence, paste, dedup, flush/clear/destroy and one 10ms default/custom timeout; [trace](traceability/stdin-buffer.md) | done within documented String/event/timer representation |
| `src/utils.ts` | measurement, ANSI, normalization, tracker (including nested CSI SGR carry), wrap/truncate/slice/segments predicates and edge matrices; approximately 11k width samples; [trace](traceability/utils.md) | done within documented host-object/byte-offset representation |
| `src/terminal-colors.ts` | strict framing, one scheme report, finite long-channel arithmetic; [trace](traceability/terminal-colors.md) | done with approved NaN RGB restriction (2026-10-21) |
| `src/terminal.ts` | finite flag parsing; Apple input/error helpers; write-log filename/local date/error side effects; actual PTY/pipe start/drain/stop/restart/resize/UTF8/paste and termios proof; [trace](traceability/terminal.md) | done with approved non-finite flags/non-usize dimensions (2026-10-21) |
| `src/tui.ts` | current frozen renderer byte streams, overlays/focus/fixed/render/flow regions, visible-window shrink, deferred force reset, expired query ordering, exact debug paths/payloads/failure; [trace](traceability/tui.md) | done with approved >u32 cell-report restriction (2026-10-21); F1–F5 regression proof |
| `src/native-modifiers.ts` | direct frozen Linux false-result comparison; Darwin-only loader audited | gap: native Darwin addon, not Linux behavior |
| `native/win32/src/win32-console-mode.c` | exact frozen export is ONLY enableVirtualTerminalInput; inspected symbol resolution, console-mode failure/success and registration | gap: Windows/N-API cannot execute on Linux; no native parity claimed |

Component, editor, markdown, image, settings-list, text and terminal-image implementations were not modified. Later-only internal modules remain by parent authorization for Q2/Q13. Removed root names and every retained old-only implementation/reference are inventoried in `/tmp/pi-rust-repin-tui-core/REPORT.md`.

## Behavioral changes with RED evidence

Frozen defaults remove later history/alternate-screen actions and modified navigation aliases. Color reports reject repeated frames; finite channels no longer overflow integer exponentiation. Width uses frozen base-grapheme behavior, not later spacing/conjunct fixes; normalization preserves tabs, wrapping splits LF only, truncation does not inject an OSC8 close. Stdin has one timeout, default 10ms.

Terminal uses frozen progress-clear semicolon, Apple-only Shift+Enter, finite wide flags, exact local-date write logs and no missing-parent creation. Real process differentials reproduced split-read UTF8 corruption, a stopped worker consuming the next lifetime's response, non-TTY ENOTTY and idle drain ignoring consumed raw protocol input. The reader now preserves partial UTF8, is cancellable/joined before restart, skips raw-mode calls without a TTY, and tracks raw activity.

TUI reproductions fixed upward shrink by repainting the visible window, expired OSC11 reply order, oversized-margin clamp order, digits-only cell framing, forbidden flow-region full redraw, and premature forced state reset before stop. Shared global keybinding identity and frozen debug/error/dump boundaries were closed rather than waived.

## Proof ledger and results

Every Cargo invocation uses the repository's mandatory runner with `CARGO_BUILD_JOBS=4` and `CARGO_TARGET_DIR=/home/osso/.worktrees/pi-rust-repin-tui-core/target`. Tests do not touch a live host terminal; PTYs are created within the private devpts sandbox. Logging tests use sandbox HOME only.

Original evidence: `/tmp/pi-rust-repin-tui-core/`. Verifier follow-up: `/tmp/pi-rust-tui-fixes/`; exact final argv/environment/revision records in `final-commands.json`, RED/GREEN ledger in `ledger.md`, mutation records in `mutations.json`. Tests retain the mandatory sandbox runner.

| Proof | Result/artifact |
|---|---|
| Requested core targets plus verifier regressions | 305 passed, zero failed; follow-up `core-stream.log` |
| Real-stream harness-free target | all 3 source/Rust PTY/pipe scenarios passed; follow-up `core-stream.log` |
| 10 touched coding-agent targets | 137 passed, 1 known settings-selector fixture failure, 2 ignored PTY entry points; follow-up `consumers.log` |
| Workspace `cargo check --workspace --all-targets` | exit 0, zero warnings; follow-up `workspace-check.log` |
| Production mutation probes | all 10 follow-up mutants killed by behavioral assertions; restored byte-identically; follow-up `mutations.json` (original 30-mutant evidence retained separately) |
| Formatting | cargo fmt --all --check exit 0 and before each commit; follow-up `fmt.log` |
| Clippy | cargo clippy -p pi-tui -j4 --all-targets -- -D warnings: exit 0, zero warnings; follow-up `clippy.log` |
| Readability | changed-line Rust/JS manual audit; analysis CLI unavailable; oracle biome lint and node syntax check pass; no delegated gate (prohibited) |

The first short-delay stream fixture could inject after frozen restored raw mode under concurrent compilation. It now synchronizes on DRAINING before late input and uses a wider bounded idle window. Production behavior was not changed to hide that fixture failure.

## User-approved numeric-domain deviations (2026-10-21)

| Frozen source/evidence | Frozen observation | Current Rust boundary |
|---|---|---|
| terminal-colors.ts:21/25; `nonfinite-rgb.log` | 256-digit f/8/0 channels produce RGB NaN/0/0 (JSON null/0/0) | byte RGB cannot represent NaN; None |
| terminal.ts:28; `numeric-domains.log` | 400 decimal flag digits parse as Infinity (JSON null) | non-finite flag rejected |
| terminal.ts:501-506; `numeric-domains.log` | -1, 1.5, Infinity and -Infinity pass through env dimension getters | usize cannot represent them |
| tui.ts:1154-1170; `wide-cell-frozen.log`, `wide-cell-rust.log` | ESC[6;4294967296;20t consumed, no component input | u32 CellDimensions rejects it, so it is forwarded; retained by user approval |

User explicitly approved all four listed numeric-domain gaps on 2026-10-21. No API widening required; affected rows are done within those approved restrictions. Native platform gaps are unchanged.

## Unrelated consumer fixture failure

`settings_selector::renders_the_settings_rows_in_source_order` assumes Show images/Image width without enabling image capability. Frozen `coding-agent/src/modes/interactive/components/settings-selector.ts:481,634-645` explicitly inserts those rows only when images are supported. The frozen selector construction test at `test/interactive-mode-status.test.ts:2052-2131` tests Escape, not row ordering, and does not enable images. Parent's conditional authorization therefore did not apply; the fixture was left unchanged except for required import migration. Q1 did not modify terminal-image or that capability condition. This report is not a claim that all consumer targets pass.

## F1–F6 follow-up coverage

| Finding | Exact behavioral proof | Mutation proof |
|---|---|---|
| F1 overflow cleanup | `verifier_regressions::overwidth_stops_terminal_before_descriptive_panic`: frozen error first line and complete end-move/show-cursor/stop bytes; subsequent rendering stopped | held-borrow mutant killed |
| F2 non-scheduling tryRender | `render_region::try_render_*`: cleared/disposed rejection, valid partial write, unchanged unrelated component render count versus scheduling request_render; pinned oracle for failure behavior | scheduling-try mutant killed |
| F3 first duplicate reference only | `verifier_regressions::removing_duplicate_child_keeps_second_reference`: middle/x ordering and Rc identity for Container and TUI; pinned oracle | independent Container/TuiBase retain-all mutants killed |
| F4 callback Set semantics | input/color duplicate registrations, unsubscribe/re-add and live removal/addition order compared with pinned source | duplicate-identity and both snapshot-dispatch mutants killed |
| F5 dynamic Termux truthiness | empty env gives 2 full redraws, late nonempty env gives 1; pinned oracle plus exact OS-environment verifier probe | present-empty and construction-snapshot mutants killed |
| F6 nested CSI SGR | exact `ESC[x ESC[31mabcdef` (without space), width 3, preserves red across wrapping; pinned oracle | anchored-SGR mutant killed |

Original verifier probes were reproduced before production edits: missing try_render yielded E0599, other probes showed the reported wrong outputs. Follow-up exact probes now match frozen; F1's original virtual terminal suppresses helper-method writes, so the committed cursor-capturing differential is the complete restoration proof. The width diagnostic's first line and complete terminal stream are compared directly; trailing static guidance and sandbox-dependent crash-log paths are not part of that differential assertion.

Manual readability audit: added callback helpers avoid held borrows during callbacks, registration identity is explicitly Rc pointer equality, and render commit owns/releases its frame guard before reentrant stop. Existing long commit/overlay/keybinding methods remain; unrelated structural refactors were not included. No warning suppressions added.
