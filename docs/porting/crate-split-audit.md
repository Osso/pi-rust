# Crate split content audit

Compared with merged parent `95b94269`; implementation snapshot `1f00a821`. [Graph, proof ledger, and timings](crate-split.md).

The pure-move commits contain only R100 renames: 356 in `753f8441`, one in `d302947e`. Of the 357 moved files, 330 remain byte-identical to the parent. The complete list of 27 moved files with content edits follows. Paths below are relative to each new crate's `src/` directory.

| Crate | File | Only content changes |
|---|---|---|
| base | `config.rs` | Point embedded package/theme includes at the original facade assets; move three CallbackError conversions to core; expose the two CLI-used environment suffix constants. |
| base | `utils/git.rs` | Expose `decode_uri_component` for the facade CLI. |
| base | `utils/mod.rs` | Expose `private_paths` for other layers. |
| base | `utils/paths.rs` | Expose `process_cwd` and `home_directory`. |
| base | `utils/private_paths.rs` | Expose the three cross-layer directory/socket helpers. |
| base | `utils/uncaught.rs` | Expose `PendingNativeError` and `defer_native_error`. |
| core | `cli/args.rs` | Expose `pad_end_utf16` for facade model-list output. |
| core | `core/agent_session.rs` | Expose `thinking_level_from_model`; use relocated process identity; point test include at the facade support directory. |
| core | `core/auth_storage.rs` | Expose `StorageError::into_callback` for interactive mode. |
| core | `core/compaction.rs` | Expose `now_millis` for extensions. |
| core | `core/desktop_notification.rs` | Expose the parent's notification error reporter for interactive/extensions. |
| core | `core/extensions/types.rs` | Expose `ExtensionApi::runtime` for the bundled parent-agent journal. |
| core | `core/mod.rs` | Declare the extracted process-identity module. |
| core | `core/model_catalog_cache.rs` | Point test include at the facade support directory. |
| core | `core/package_manager_process_tests.rs` | Point executable-script support path at the facade fixture. |
| core | `core/package_manager_semver.rs` | Point JSON fixture include at the facade fixture; formatter wraps the include. |
| core | `core/resident_console_transport.rs` | Receive the unchanged `ResidentConsoleService::as_str` impl from the facade CLI; inherent impl must live with its type. |
| core | `core/self_restart.rs` | Point test oracle include at the facade support directory. |
| core | `core/session_manager.rs` | Expose `SessionManagerError::raise_uncaught` for the facade; point test oracle include at the facade support directory. |
| core | `core/tools/change_working_directory.rs` | Point test oracle include at the facade support directory. |
| core | `modes/extension_actions.rs` | Expose the two interactive-mode session callback adapters. |
| extensions | `bundled_extensions/agents_core/lifecycle_runtime.rs` | Re-export the unchanged extracted process-identity function; remove its now-unused imports. |
| extensions | `bundled_extensions/bwrap/worker_matching_tests.rs` | Preserve the original test oracle include base. |
| extensions | `bundled_extensions/pyrun/index.rs` | Point unit-test theme fixture at the facade theme. |
| interactive | `modes/interactive/components/mod.rs` | Re-export core-owned UI leaves under their old component paths. |
| interactive | `modes/interactive/interactive_mode.rs` | Point unit-test includes, fixtures, and themes at the facade directory; formatter wraps long expressions. |
| interactive | `modes/interactive/mod.rs` | Re-export core-owned theme under its old path. |

## New files and facade edits

New Cargo manifests and library roots declare the layers and their re-exports. The new base console/mode-constant modules, core CallbackError conversions, and core process-identity module contain implementations relocated unchanged from their previous owners. Existing facade module declarations become re-exports. All prior facade root exports, bin/test/example declarations, public import paths, and asset bytes remain unchanged.

Three facade test implementation/asset includes change to the physical moved locations; their imports and assertions do not change. Two committed relative symlinks preserve nested unit-test support includes: `crates/pi-coding-agent-core/tests` and `crates/pi-coding-agent-interactive/tests`, both targeting `../pi-coding-agent/tests`. New packages disable automatic integration-test discovery so those targets remain facade-owned.

The only separately approved non-structural correction is commit `6019805b`: the pre-existing subagent description macro failed during codegen because `js_format!` has no brace escapes. Passing `{previous}` as an ordinary argument preserves the exact frozen description. Its added test/oracle metadata branch compares the registered description with the frozen extension.

## Checks

The full diff was read after path normalization. No changed function body introduces new control flow, fallback behavior, warning suppression, or new abstraction. The new TS oracle branch only returns metadata for its new test request. Existing unrelated source complexity remains unchanged.

`Cargo.lock` adds only the four internal package records and the facade's four path dependencies; every external package record is unchanged. Workspace profiles/lints and `.cargo/config.toml` are unchanged. `scripts/source-inventory.py` has no moved Rust references and needs no edit. Rust-path documentation edits do not change locked TS counts or coverage status values.

Artifacts on the development host: `/tmp/pi-crate-split-proof/pure-move-753f8441.log`, `pure-move-d302947e.log`, `diff-audit-name-status.log`, and `diff-content-audit.patch`.
