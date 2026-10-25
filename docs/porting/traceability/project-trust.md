# project-trust traceability

Sources (pinned revision in `docs/porting/source-lock.json`):

- `packages/coding-agent/src/core/project-trust.ts` (96 lines)
- `packages/coding-agent/src/cli/project-trust.ts` (62 lines)

Rust:

- `crates/pi-coding-agent-tools/src/core/project_trust.rs`
- `crates/pi-coding-agent/src/cli/project_trust.rs`

No TypeScript test file covers either module (`test/project-trust.test.ts` does not exist), so
coverage is behavioral Rust tests over the real trust store and the real extension runner:
`crates/pi-coding-agent/tests/project_trust.rs` (19 tests).

## Exported symbols

| TS symbol | Rust | Tests |
| --- | --- | --- |
| `AppMode` (core) | `core::project_trust::AppMode` | `cli_context_maps_app_modes_to_extension_modes` |
| `ResolveProjectTrustedOptions` | `core::project_trust::ResolveProjectTrustedOptions` | every core test |
| `resolveProjectTrusted` | `core::project_trust::resolve_project_trusted` | all core tests below |
| `createProjectTrustContext` (cli) | `cli::project_trust::create_project_trust_context` | all `cli_context_*` tests |
| private `formatProjectTrustPrompt` | `format_project_trust_prompt` (public) | `prompt_text_matches_the_typescript_template` |

## Branches (`resolveProjectTrusted`)

Ordered exactly as in the TypeScript source; each row names the test that reaches it.

| # | Branch | Test |
| --- | --- | --- |
| 1 | `trustOverride !== undefined` returns it | `trust_override_short_circuits_every_other_check` |
| 2 | no trust-requiring project resources → `true` | `projects_without_trust_requiring_resources_are_trusted` |
| 3 | extension result present, `trusted: "yes"` → `true` | `extension_decisions_win_and_remember_persists_them` |
| 4 | extension result present, `trusted: "no"` → `false` | `extension_decision_without_remember_does_not_write` |
| 5 | extension result `remember: true` → `trustStore.set` | `extension_decisions_win_and_remember_persists_them` |
| 6 | extension handler error → `onExtensionError` text | `extension_errors_are_reported_and_resolution_continues` |
| 7 | stored decision trusted / untrusted | `stored_decisions_win_over_the_default_setting` |
| 8 | `defaultProjectTrust: "always"` / `"never"` | `default_setting_applies_when_the_store_has_no_decision` |
| 9 | `ask` + `!hasUI` → `false` | `ask_without_ui_is_untrusted` |
| 10 | prompt selection persisted (`updates.length > 0`) | `ask_prompts_with_every_option_and_persists_the_choice` |
| 11 | session-only selection writes nothing | `session_only_choices_do_not_touch_the_store` |
| 12 | prompt dismissed / unknown label → `false` | `dismissing_the_prompt_is_untrusted`, `an_unknown_selection_is_untrusted_without_writing` |

Option labels and their order come from the already-ported `trust_manager::get_project_trust_options`;
the prompt test asserts the full five-label list including the parent-folder label.

## Branches (`createProjectTrustContext`)

| # | Branch | Test |
| --- | --- | --- |
| 1 | `mode === "interactive" ? "tui" : mode` | `cli_context_maps_app_modes_to_extension_modes` |
| 2 | `select` requires `hasUI` and interactive mode | `cli_context_selects_through_the_startup_ui_in_interactive_mode`, `cli_context_without_ui_never_prompts`, `cli_context_outside_interactive_mode_never_prompts` |
| 3 | `confirm` uses `Yes`/`No` labels and `?? false` | `cli_context_confirm_uses_yes_no_labels` |
| 4 | `input` forwards title and placeholder | `cli_context_selects_through_the_startup_ui_in_interactive_mode` |
| 5 | `notify` prints only outside interactive mode | not asserted (writes to stderr; no capture seam) |

## Documented deviations

- **Startup UI seam.** TypeScript calls `showStartupSelector`/`showStartupInput` from
  `cli/startup-ui.ts`. That module is still blocked on `ExtensionInputComponent` and
  `FirstTimeSetupComponent`, so `create_project_trust_context` takes a `ProjectTrustStartupUi`
  trait object instead of a `SettingsManager`. The two methods mirror
  `showStartupSelector(settingsManager, title, [{label, value}])` and
  `showStartupInput(settingsManager, title, placeholder)`; `confirm` maps the selected label back
  to the `true`/`false` values TypeScript passes. When `startup-ui.ts` lands, the trait
  implementation becomes the concrete adapter and this seam disappears.
- **Error propagation.** TypeScript rejects the promise when the trust store cannot be read or
  written. `resolve_project_trusted` returns `Result<bool, TrustError>` instead of swallowing the
  error (`unreadable_trust_store_errors_instead_of_silently_untrusting`).
- **`notify` output** is written with `eprintln!` after the same chalk color selection as the
  TypeScript `console.error`; there is no stderr capture in the test harness, so the color path is
  covered only by inspection.

## Q18 native trust continuation (2026-10-22)

Trust persistence already uses native JsObject path keys with lossless JSON.
ProjectTrustContext/Event cwd, resolution callbacks, prompts/options/selections,
and CLI startup UI now use JsString. CLI notification coloring stays native until
stderr write. Project-trust UI selection errors propagate like the frozen source;
no `.ok().flatten()` suppression. Trust-directory error context retains original
units instead of reusing its encoded filesystem argument.

`lossless_project_trust` covers raw trust-key persistence/reopen, raw cwd prompt,
and raw UI failure propagation. Written but unexecuted while coding compilation
is blocked. replacement-cli-2 has zero trust/resource/package module primaries;
no runtime PASS or renewed frozen differential parity claimed.

## Process-terminal trust wiring (2026-10-25)

Both production callers previously passed no startup UI, so interactive trust silently became untrusted. Real package-list and main-startup PTY regressions reproduce the missing prompt (`/tmp/settings-models-trust-pty-red.log`) and now pass after the host is wired to the existing startup selector (`/tmp/settings-models-trust-pty-green.log`). Both assert the persisted Trusted decision, not only rendered text. The adapter runs the Rc-based startup TUI on one blocking worker/current-thread runtime and uses a merged-settings snapshot for its read-only theme/terminal settings. The native startup UI trait now returns Result, preserving UI failures; test implementations changed mechanically. The main factory receives startup UI as captured host state. No source algorithm or provider was changed.
