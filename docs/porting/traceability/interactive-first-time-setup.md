# components/first-time-setup.ts

Source: `coding-agent/src/modes/interactive/components/first-time-setup.ts`
Rust: `crates/pi-coding-agent-interactive/src/modes/interactive/components/first_time_setup.rs`

Tests: `interactive_components_c.rs::first_time_setup_*`, differential `firstTimeSetup`.

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `FirstTimeSetupComponent` | `FirstTimeSetupComponent` | behavioral + differential |
| `FirstTimeSetupOptions` | `FirstTimeSetupOptions` | differential |
| `FirstTimeSetupResult` | `FirstTimeSetupResult` | differential submitted payloads |
| initial theme index (`Math.max(0, findIndex)`) | `position().unwrap_or(0)` | differential both detected themes |
| theme step -> analytics step on confirm | `SetupStep` | walkthrough test |
| `onThemePreview` on changed selection | `move_selection` | walkthrough + differential |
| analytics selection clamped | `move_selection` | differential `j` |
| cancel -> `onCancel` | `Component::handle_input` | cancel test |
| logo/welcome/hints | `Text` children | differential |

## Gaps

- `APP_NAME` comes from `config::package_config().app_name` (the source's `config.ts` constant).

## Discrepancies

None found.

## Q18 native UTF-16

Render/input, style composition and hint joins retain `JsString`; no internal encoding. Behavioral cases in `raw_error_consumer_components` cover unmatched input then selection; trust also retains unmatched path units in summary. Tests **unexecuted**, coding-agent remains compiler-blocked. Trust construction propagates path failures with `Result`, rather than dropping them.
