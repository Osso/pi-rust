# theme-controller.ts

Frozen `4f9e6f8a4`, `packages/coding-agent/src/modes/interactive/theme/theme-controller.ts:1–126`.
Rust: `crates/pi-coding-agent-tool-support/src/modes/interactive/theme/theme_controller.rs`.

| Frozen symbol / branch | Rust | Proof |
|---|---|---|
| constructor environment resolution/init/listener, 27–34 | `new`, `bind_terminal_color_scheme_listener` | `differential_theme_controller` compares UI events and theme/terminal/settings snapshots |
| auto setting/query/notifications/pick theme, 37–44 | `apply_from_settings` auto branch | differential: scheme success, background unavailable, settings edited before terminal event |
| explicit setting, 47–51 | explicit branch | differential explicit dark/light/missing |
| unset setting/background/persist high confidence, 54–60 | detection branch | differential white background then second application; `theme_controller::detects_and_persists_terminal_theme` real settings persistence |
| setThemeName, 64–67 | `set_theme_name` | differential success/failure, showError true/false, disables auto-sync once |
| setThemeInstance, 69–76 | `set_theme_instance` | differential active light instance, notifications/invalidations/changed order |
| preview resolved setting/active name, 78–85 | `preview` | differential explicit/auto preview: requestRender without onChanged |
| disableAutoSync/getTerminalTheme, 88–94 | matching methods | differential repeated disable, terminal state after disabled events |
| applyThemeName fallback/error, 97–105 | `apply_theme_name` | exact error event and fallback dark snapshot |
| notifyChanged, 108–111; idempotent auto-sync, 113–117 | matching core methods | full event-order comparison |
| terminal listener disabled/missing auto/current theme, 120–125 | `apply_terminal_theme` | differential: disabled event ignored, same-name no notification, changed setting disables auto |

Removed old-only rebind API and listener-unsubscribe-on-drop behavior. Frozen constructor retains the controller through the registered callback. `registered_ui_listener_retains_controller` drops the Rust handle, emits light, and observes the active theme change. Initial RED observed dark instead.

Theme detection helper is **sequential** at frozen theme.ts:771–782: await optional scheme; return immediately on a value; only then query background on unsupported/empty/rejected scheme. The former traceability statement about frozen concurrency was incorrect. `differential_auto_theme` compares six real pinned traces with delayed completion: scheme success never starts background; empty/error/missing scheme reaches background; background value/error/empty resolves through the environment. Both timed queries consume their own budget. Rust synchronous poll-query adapters preserve this ordering, not a concurrent-query deviation.

Proof ledger `/tmp/pi-rust-repin-lossless-interactive/`: `test-theme-controller-lifetime-red.log`, `test-auto-theme-early-return-red.log`, `test-theme-controller-differential-red.log`; GREEN `test-theme-controller-green-1.log`: controller differential 1/1 (four scenarios), detection differential 1/1 (six scenarios), native probes 4/4 (+4 child probes ignored in parent runner and executed by wrappers).
