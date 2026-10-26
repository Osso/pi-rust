# Pi Atlas native extension

Source: `@mohndoe/pi-atlas` 0.2.1, commit `38d84cb549cce66dd76d45a5020d132f74780393` in the read-only `/home/osso/.worktrees/pi-rust-ext-sources/pi-atlas`. Requirements: [native Pi Atlas](../../specs/pi-atlas-native.md).

## Module audit

| Snapshot module / behavior | Native implementation | Behavioral proof |
| --- | --- | --- |
| `index.ts`: sole `/atlas` registration, description, `hasUI` guard, loading/dashboard phases, cancellation, overlay sizing | `command.rs`, hidden registration in `native_user_extensions/mod.rs`, startup registration in `pi-coding-agent/src/main.rs` | `startup_resolves_atlas_from_user_settings_package_path` runs the actual `pi -p hi` binary under isolated HOME and literal `../../../Repos/pi-atlas` settings; `settings_package_loads_atlas_native_command`; `headless_atlas_does_not_parse_or_write_files`; `loads_disk_logs_then_opens_six_tab_dashboard_and_closes`; `cancellation_closes_loading_but_worker_still_writes_cache` |
| `types.ts`, `helpers/session.helper.ts`, `helpers/usage.helper.ts`: ordered aggregate construction, numeric merges, usage fields | `data.rs` | frozen parser/summary fixtures; exact TS-cache byte round trip, including escaped UTF-16 keys/text |
| `parser.ts`: dispatch, corrupt-line accounting, identity-first merges, explicit/implicit skills, all source cost-relevant entries, sanitized tool names, edit/write language attribution | `parser.rs`, `data.rs` | `aggregates_session_messages_skills_and_language_edits`; `matches_frozen_parser_fixtures_including_utf16`; `corrupt_lines_report_node_error_class_and_retain_valid_entries` |
| `cache.ts`: recursive JSONL traversal, sorted SHA-256 signature, cache validation, version, timestamp, parse progress, debug environment settings, sorted aggregates, diagnostics | `cache.rs`; existing shared SHA-256 helper made public, unchanged algorithm | cache exchange, nested discovery/invalidation, post-parse timestamp test; external TS reverse-reader and Node signature comparison |
| `compute.ts`: all four ranges, project/model/provider filters, totals, unique sessions/dates, stable rankings, skills, daily zero filling and local hourly costs | `compute.rs` | 16 frozen summary/filter cases; chart fixtures and lifecycle fixtures |
| `format.ts`: extension language map, basename, decimal/currency/model-name formatting, local cache timestamps, ANSI/control stripping | `format.rs`, `ui.rs` | frozen format fixtures, local-calendar test, parser and dashboard fixtures |
| `colorPalette.ts`: complete language/provider mappings and unknown-name white | `palette.rs` with existing native Chalk level/standard styles | all entries, unknown and empty names, at all four Chalk levels |
| `components/Dashboard.ts`, `TabBar.ts`, `RangeSelector.ts`: title/footer, range rebuilds, empty states, clamped tab switching, close keys, input dispatch, source render-cache/resize order | `command.rs`, `ui.rs`, `state.rs` | 36 complete plain/themed dashboard frames; keyboard test; 24 source-driven scroll/range/resize frames |
| `components/LoadingView.ts`, `components/shared/Bar.ts`: progress bar, counts, remaining-time label, cancel keys | `loading.rs`, `command.rs` | plain/themed loading fixtures and actual asynchronous factory/worker/done lifecycle tests |
| `components/SortedTable.ts`, `cells.ts`, `MarqueeText.ts`: used fixed/fill columns, sort indicators, source padding/truncation, focus/scroll, UTF-16 marquee slicing, timer render requests | `table.rs`, `state.rs`, `command.rs` | dashboard frames, focused long project names, scroll/range/resize sequence; native TUI handles terminal input parsing and text layout |
| `components/KpiCards.ts`, `StatCard.ts`, `shared/GridRow.ts`: six KPIs, token cards, percentage grids, source UTF-16 `padEnd` semantics | `ui.rs`, `layout.rs`, native `pi_tui::Text` | plain/themed dashboard fixtures at widths 80, 100, 160 |
| `components/BarChart.ts`: hourly/daily chart, aggregation, raw-max scaling, axes, labels, granularity | `chart.rs` | 36 source chart cases: four ranges, three widths, three heights, 60 daily entries / 24 hours |
| `tabs/Overview.ts`, `Languages.ts`, `Models.ts`, `Projects.ts`, `Skills.ts`, `Usage.ts` | `ui.rs` | all six tabs in plain/themed fixtures; live command lifecycle; range and scroll frames |
| Used `pi-tui-extras`: `BorderBox`, left/right alignment, padding and truncation | `layout.rs`, reusing native pi-tui text/wrapping/measurement | full plain/themed rendering fixtures |

`components/Header.ts`, `RankedBarList.ts`, and its helper `UsageRow.ts` are not reachable from the package entry point; only the dead ranked-list test imports that component. They provide no command, hook, widget, footer, or dashboard behavior and are not loaded by this native extension. The source entry point registers no event hooks, tools, footer, or widget. Unused general-purpose dependency options are not ported, as requested.

The repository's existing pathological-input, OS/runtime-fault, numeric-extreme, and scheduling-race exclusions apply. No additional user-observable deviation is introduced. JS stack frames remain under the existing repository-wide stack-text exclusion.

## Preserved source discrepancies

- The README describes `~/.pi`; executable code uses `${XDG_CONFIG_HOME ?? ~/.config}/pi`. The native path follows executable code.
- The chart indexes a Monday-first weekday-name array with Sunday-first `getUTCDay()`. The resulting labels are preserved and covered by source chart fixtures.
- Dashboard border caching prevents ordinary timer-only marquee requests from changing an already cached frame. Height-only resize rebuilds tables even while returning cached output; newly rebuilt tables do not receive scroll input until first rendered. These source behaviors are preserved, not silently repaired.
- Empty directories produce an empty signature, which the source cache reader rejects even though the writer stores it. Native behavior matches.

## Persistence and oracle setup

All oracle executions used a scratch copy of the snapshot with its `node_modules` symlinked to `/home/osso/Repos/pi-atlas/node_modules`. `tsx` was executed as `node /home/osso/.worktrees/pi-rust-source-4f9e6f8a4/node_modules/tsx/dist/cli.mjs`. The original snapshot and dependency trees were not modified. No network or live provider was used.

The native cache interchange test reads the TS-written `fixtures/cache.json`, reconstructs native aggregates, and writes byte-identical JSON. Its retained `atlas-cache-interchange` test artifact is then read by the snapshot's real `readCache()`; the snapshot's real `computeSignature()` also matches the native SHA-256 over the same files, sizes and fractional modification milliseconds. This proves both file-format directions, not only a native self-roundtrip.

Fixtures live beside the native module under `fixtures/`. Source generation scripts were run in `/tmp/pi-atlas-oracle-native`; clocks were frozen at `2026-10-25T12:00:00.000Z` for domain/cache fixtures. UTC was explicit for hourly fixtures. Palette fixtures use a fresh process per level with `FORCE_COLOR=0..3`, `COLORTERM=''`, and `TERM=dumb`; changing `chalk.level` after `.hex()` builder creation does not change captured color escape sequences.

## Final integration proof

Merged canonical master `9b9c08990` (the real-startup resource-loader registry wiring). The real-process atlas test was RED with the atlas factory absent from the shared registry (`Failed to load extension ... pi-atlas/src/index.ts`) and GREEN after its hidden registration was added there. Existing regex-replace and disable-broadcast registrations are preserved.

- `cargo test -j3 -p pi-coding-agent-extensions --lib native_user_extensions::pi_atlas`: 22 passed.
- `cargo test -j3 -p pi-coding-agent --test suite_extensions native_user_extensions_process::startup_resolves_atlas_from_user_settings_package_path -- --exact`: passed; actual binary, isolated HOME, literal relative package path, no load failure.
- `cargo check -j3 --workspace --all-targets` and `cargo fmt --check`: passed.
- `cargo clippy -j3 -p pi-coding-agent-extensions -p pi-coding-agent-tools -p pi-coding-agent --all-targets --all-features -- -D warnings`: passed.
- `python3 scripts/source-inventory.py`: 686 rows / 186679 lines, zero mismatches; TS reverse-cache reader and Node signature check also passed on the final native cache artifact.

Proof logs are `/tmp/pi-atlas-*-final.log`, `/tmp/pi-atlas-process-red.log`, and `/tmp/pi-atlas-process-green.log`; the final reverse-cache artifact is `/var/tmp/osso/pi-test-_oa9ixwf/atlas-cache-interchange`. No network, live provider, deployment, push, stash, or subagent was used. Readability tools were unavailable; manual review separated table metadata/rows, domain aggregation, filesystem effects, UI state and layout. No warning suppressions or lossy UTF-16 projections were added.

## UTF-8 file decoding (readability audit 2026-10-26)

`cache.rs::read_cache` and the session-log read in `load_aggregate_with_progress` (source `cache.ts:64`, `parser.ts:391`) use `String::from_utf8_lossy` because the source reads them with Node `readFile*(path, "utf-8")`, i.e. WHATWG UTF-8 decoding with U+FFFD replacement of maximal invalid subparts; Rust's lossy decoder applies the same replacement rule, so output is identical. A fallible decode would turn source "replace and continue" into an error. Readability-audit P1 (lossy conversion) rejected on this basis; invalid UTF-8 input is otherwise an adversarial-input gap under the parity bar.
