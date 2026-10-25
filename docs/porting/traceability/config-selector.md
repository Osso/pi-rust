# config-selector traceability

Source: `packages/coding-agent/src/cli/config-selector.ts` (56 lines, pinned revision in
`docs/porting/source-lock.json`). Rust: `crates/pi-coding-agent/src/cli/config_selector.rs`.

Status: done. The TUI is driven through a scripted `Terminal` in a child probe because
`init_theme` resolves built-in themes from the package directory and edition 2024 forbids a safe
`std::env::set_var` in-process.

## Exported symbols

| TS symbol | Rust | Tests |
| --- | --- | --- |
| `ConfigSelectorOptions` (flat resolved paths/settings/cwd/agentDir) | `ConfigSelectorOptions` | adapter test |
| `selectConfig` | `select_config` | `escape_closes_the_selector`, `ctrl_c_closes_through_the_default_cancel_binding`, `ctrl_c_exits_when_the_cancel_binding_excludes_it` |
| — (command-path adapter) | `TuiConfigSelector` (`package_manager_cli::ConfigSelector`) | `package_command_adapter_returns_ok_after_the_tui_closes` |

## Branches

| # | Branch | Test |
| --- | --- | --- |
| 1 | `initTheme(getTheme(), true)` before the TUI | exercised by every probe (theme watcher started/stopped) |
| 2 | `new TUI(new ProcessTerminal())`; native terminal injection | probe (fake terminal + agent dir) |
| 3 | `onClose` → stop, `stopThemeWatcher`, resolve | `escape_closes_the_selector` |
| 4 | `onExit` → stop, `stopThemeWatcher`, `process.exit(0)` | `ctrl_c_exits_when_the_cancel_binding_excludes_it` |
| 5 | ctrl+c with the default `tui.select.cancel` binding (`escape`, `ctrl+c`) takes the cancel branch first | `ctrl_c_closes_through_the_default_cancel_binding` |
| 6 | selector focused, terminal rows passed as `terminalHeight` | probe (renders without panic; focus drives the key handling) |
| 7 | request-render callback wired to `ui.requestRender()` | probe (`flush_pending` renders) |

## Deviations

- **Event loop** — the TypeScript TUI owns its loop; Rust's `TUI` is driven by the
  caller. `select_config` polls `process_events`/`flush_pending` every 5 ms until a close/exit
  callback fires.
- **Focus target** — TypeScript focuses `selector.getResourceList()`; the Rust
  `ConfigSelectorComponent` forwards `handle_input` to its resource list and is focusable, so the
  component itself is focused.
- **Settings manager identity** — `TuiConfigSelector` reloads an equivalent `SettingsManager` for
  the TUI because the command-path store owns its manager behind a mutex. Toggles write through
  to the same settings files, and the command exits right after the TUI closes.
  Trust comes from the command's settings manager, not an old-only selector availability field.
- **`process.exit(0)`** — returned as `SelectConfigOutcome::Exit`; the adapter treats both
  outcomes as normal completion because the command's exit status is 0 either way.

## Proof

`cargo test -p pi-coding-agent --test config_selector_cli` (4 tests + 1 ignored probe entry).

## Homedir failure propagation

Frozen formatBaseDir → getGroupLabel → buildGroups → constructor throws are carried by NativeNodeError Results through ResourceList and ConfigSelectorComponent construction, then CallbackError from CLI select_config to the existing command error adapter. No panic carrier remains. `differential_config_selector_home` forces getpwuid_r EIO for user/project auto resources; exact source/native class and message match. RED/GREEN: `/tmp/pi-rust-repin-lossless-interactive/config-selector-home-{red,green}.log`.

## Config dispatch re-pin (2026-10-25)

Frozen `handleConfigCommand` ignores help, unknown, positional and local flags;
only project-trust overrides are scanned. It opens `selectConfig` and exits 0.
The native extra help/validation/local action field is removed. DISCREPANCY:
frozen `config --help` opens the UI rather than help; preserved literally.
`config_dispatch_process` drives the real frozen handler and native binary
through private ptys for --help, --bogus, positional extra, -l and a mixed case;
it compares selector entry, filter hint, absence of help/validation output and
exit 0, and requires terminal-mode restoration. Whole component frame proof
remains in the existing selector differential. RED: frozen reached/closed UI,
native printed its extra config help and never entered the selector. The three
validation-only calls are removed from the package-command JSON oracle test.

Process proof: the five-case pty matrix passed after restoring dispatch. One
initial frozen launch produced no bytes by the 10-second startup bound; the
diagnostic run completed all ten processes in 43.5 seconds. Startup alone now
has a 30-second bound; close/exit remains bounded to 10 seconds. Process origin,
arguments and PID are logged so future startup failures are attributable.
