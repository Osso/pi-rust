# windows-self-update traceability

Pinned source: `packages/coding-agent/src/utils/windows-self-update.ts` at
`532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-coding-agent/src/utils/windows_self_update.rs`.

Differential test skipped: the TypeScript module reads `process.report.getReport().sharedObjects`
and performs real renames/copies. The Rust port injects the shared-object list and is proven
behaviorally on real files (`crates/pi-coding-agent/tests/windows_self_update.rs`, 6 cases).

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `windows-self-update.ts:6` | Quarantine directory name | `QUARANTINE_DIR_NAME` |
| `windows-self-update.ts:8-10` | Resolve paths before comparison | `resolve` |
| `windows-self-update.ts:12-24` | Walk up to the nearest `node_modules` (case-insensitive), return `<node_modules>/.pi-native-quarantine` | `get_quarantine_root`; `quarantine_root_is_the_nearest_node_modules_directory` |
| `windows-self-update.ts:26-48` | Keep loaded shared objects inside the package dir, deduplicated, case-insensitive | `loaded_shared_objects_in_package_dir`; `loaded_shared_objects_are_filtered_and_deduplicated` |
| `windows-self-update.ts:50-60` | Best-effort recursive removal of the quarantine root | `cleanup_windows_self_update_quarantine`; `cleanup_removes_the_quarantine_root` |
| `windows-self-update.ts:62-84` | Create a unique run dir, move each loaded file preserving its package-relative path, then copy it back so the running process keeps the original path | `quarantine_windows_native_dependencies`; `quarantine_moves_native_files_and_restores_a_copy` asserts the run layout, the restored copy, and that outside files are ignored; `quarantine_without_loaded_files_is_a_noop` |

## Gaps

- Shared-object enumeration: TypeScript reads the Node report. The Rust binary loads no Node addons,
  so `process_shared_objects` reads `/proc/self/maps` on Linux and returns nothing elsewhere; callers
  can inject the list. `process_shared_objects_are_absolute_paths` checks the Linux default.
- Quarantine run names use a v7 UUID instead of `randomUUID`'s v4. The name is only used for
  uniqueness, never parsed.
- Path containment uses Rust's component-wise `Path::starts_with` (case-sensitive) instead of the
  source's lowercased string comparison. On Windows a case-different path under the package dir
  would be treated as outside; the port targets the Rust runtime, which does not use this module.

## Discrepancies

- None observed in the exercised behavior: relative quarantine paths, copy-back, and cleanup match
  the source.

## Proof inventory

- Behavioral cases: 6.
