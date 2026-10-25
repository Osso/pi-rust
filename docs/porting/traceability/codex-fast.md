# `codex-fast` extension traceability

Common pin: `4f9e6f8a4`. Source: `packages/coding-agent/extensions/codex-fast/src/index.ts`. Rust:
`crates/pi-coding-agent-extensions/src/bundled_extensions/codex_fast/mod.rs`.

| TypeScript symbol / branch | Rust item | Proof |
|---|---|---|
| `default` factory with `CodexFastOptions.authority` | `codex_fast_extension`, `CodexFastOptions`, `register` | `bundled_commands.rs::fast_toggles_priority_and_updates_status`; differential `toggle` |
| `FastModeAuthority` shared across runtimes | `FastModeAuthority` (`Arc<Mutex<Option<FastServiceTier>>>`) | `fast_shares_authority_and_blocks_child_commands`; differential `child` |
| `supportsFastMode` (`openai-codex`, `openai-codex-gc`, `openai-codex-team`) | `supports_fast_mode` | differential `team-provider`, `unsupported`, `openai-provider`, `missing-model` |
| `requestedFastMode` (`""`, `on`, `ultra`, `off`, invalid) | `requested_fast_mode` | differential `toggle`, `explicit`, `ultra`; behavioral command tests |
| `fastModeLabel` | `fast_mode_label` | differential notifications/statuses |
| `parsePersistedServiceTier` (`null`, `priority`, `ultrafast`, other) | `parse_persisted_service_tier` | differential `restore-ultrafast`, `persisted-null-over-config` |
| `readPersistedFastMode` (last custom entry wins) | `read_persisted_fast_mode` | differential `restore-ultrafast`, `persisted-null-over-config` |
| `readConfiguredFastMode` (`defaultCodexFastMode`) | `read_configured_fast_mode` | differential `restore-configured` |
| `persistFastMode` (`codex-fast-mode`, `{serviceTier}`) | `persist_fast_mode` | differential `explicit`, `ultra` |
| `updateFastStatus` (`fast`, `fast ultra`, unset) | `update_fast_status` | differential statuses; `codex_fast_status_reads_current_authority_and_display_override` checks live authority and supported/unsupported/missing display model |
| `handleFastCommand` (child guard, usage, provider guard, toggle, notify, editor clear) | `handle_fast_command` | differential command cases; behavioral tests |
| `session_start` restore + status | handler in `codex_fast_extension` | differential `restore-*` |
| `model_select` status refresh | `on_model_select` handler | differential `model-switch` |
| `before_provider_request` injection and non-object warning | handler in `codex_fast_extension` | differential `ultra`, `non-object-payload` |

## Discrepancies / gaps

- `ExtensionApi::append_entry` receives the same JSON object; ordering and
  `serviceTier: null` match.
- Settings are read through `SettingsManager::get_merged_settings` (the Rust
  equivalent of `getMergedSettings`).

## Baseline display re-pin (2026-10-25)

Unsupported-provider warning now uses exact frozen "Fast mode requires a Codex provider"; existing differential RED was only that message.

## Re-pin proof ledger

- RED: `/tmp/bundled-fast-red.log` shows frozen handler mismatch (team-provider and warning text); `/tmp/bundled-fast-status-red.log` shows missing off-state callback.
- Whole-module audit preserves command metadata, validation/editor ordering, branch restore, configured defaults, request object injection, and callback display selection. Native owned values apply; pathological inputs/runtime races are outside the parity bar.
- GREEN at `9c81479b8`: both focused tests pass (`/tmp/bundled-fast-green.log`); 14 frozen cases plus live-status callback matrix. Targeted extension clippy is blocked by 7 existing errors in `pi-coding-agent-base` (`/tmp/bundled-ext-clippy.log`); no lint suppression or adjacent fix added. Changed Rust lines reviewed against the readability checklist.
