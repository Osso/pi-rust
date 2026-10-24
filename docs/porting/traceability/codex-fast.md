# `codex-fast` extension traceability

Source: `packages/coding-agent/extensions/codex-fast/src/index.ts`. Rust:
`crates/pi-coding-agent-extensions/src/bundled_extensions/codex_fast/mod.rs`.

| TypeScript symbol / branch | Rust item | Proof |
|---|---|---|
| `default` factory with `CodexFastOptions.authority` | `codex_fast_extension`, `CodexFastOptions`, `register` | `bundled_commands.rs::fast_toggles_priority_and_updates_status`; differential `toggle` |
| `FastModeAuthority` shared across runtimes | `FastModeAuthority` (`Arc<Mutex<Option<FastServiceTier>>>`) | `fast_shares_authority_and_blocks_child_commands`; differential `child` |
| `supportsFastMode` (`openai-codex`, `openai-codex-gc`) | `supports_fast_mode` | differential `unsupported`, `openai-provider` |
| `requestedFastMode` (`""`, `on`, `ultra`, `off`, invalid) | `requested_fast_mode` | differential `toggle`, `explicit`, `ultra`; behavioral command tests |
| `fastModeLabel` | `fast_mode_label` | differential notifications/statuses |
| `parsePersistedServiceTier` (`null`, `priority`, `ultrafast`, other) | `parse_persisted_service_tier` | differential `restore-ultrafast` |
| `readPersistedFastMode` (last custom entry wins) | `read_persisted_fast_mode` | differential `restore-ultrafast` |
| `readConfiguredFastMode` (`defaultCodexFastMode`) | `read_configured_fast_mode` | differential `restore-configured` |
| `persistFastMode` (`codex-fast-mode`, `{serviceTier}`) | `persist_fast_mode` | differential `explicit`, `ultra` |
| `updateFastStatus` (`fast`, `fast ultra`, unset) | `update_fast_status` | differential statuses |
| `handleFastCommand` (child guard, usage, provider guard, toggle, notify, editor clear) | `handle_fast_command` | differential command cases; behavioral tests |
| `session_start` restore + status | handler in `codex_fast_extension` | differential `restore-*` |
| `model_select` status refresh | `on_model_select` handler | differential `model-switch` |
| `before_provider_request` injection and non-object warning | handler in `codex_fast_extension` | differential `ultra`, `non-object-payload` |

## Discrepancies / gaps

- `ExtensionApi::append_entry` receives the same JSON object; ordering and
  `serviceTier: null` match.
- Settings are read through `SettingsManager::get_merged_settings` (the Rust
  equivalent of `getMergedSettings`).
