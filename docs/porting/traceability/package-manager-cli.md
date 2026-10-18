# coding-agent package-manager-cli.ts traceability

Source: `packages/coding-agent/src/package-manager-cli.ts` (889 lines, pinned
revision in `docs/porting/source-lock.json`). Rust:
`crates/pi-coding-agent/src/package_manager_cli.rs`.

Status: partial. The argv parser, help/usage text, validation output, exit codes,
project-trust warning formatting, and package-list formatting are ported and
differentially tested. Command execution is returned as a typed `PackageAction`
for the runtime; the runtime side is blocked on modules owned by other slices.

## Exported symbols

| TS symbol | Rust item | Tests |
| --- | --- | --- |
| `PackageCommand` | `PackageCommand` | parser tests |
| `PackageCommandRuntimeOptions` | not ported (execution seam) | gap |
| `handleConfigCommand` | `handle_config_command` (validation/help half) | differential `handle_config` cases, `valid_commands_return_actions` |
| `handlePackageCommand` | `handle_package_command` (validation/help half + action) | differential `handle_package_command` cases |
| private `parsePackageCommand` | `parse_package_command` + `PackageCommandOptions` | parser matrix tests, differential |
| private `getPackageCommandUsage` | `package_command_usage` | validation tests, differential |
| private `printPackageCommandHelp` | `package_command_help` | help differential |
| private `printConfigCommandHelp` | `config_command_help` | config help differential |
| private `updateTargetIncludesSelf` / `...Extensions` | `update_target_includes_self` / `update_target_includes_extensions` | predicate test |
| private `reportProjectTrustWarnings` | `report_project_trust_warnings` | compile-time path |
| private `reportSettingsErrors` | not ported (needs `SettingsManager::drainErrors`) | gap |
| private `formatPackage` / list block | `format_package_list` | list formatting test |

## Branches

| Branch | Test |
| --- | --- |
| `uninstall` alias, `-l/--local` only for install/remove | parser test |
| `--self/--extensions/--models/--all/--force` only for update | differential invalid-option cases |
| `--extension` missing value, duplicate, non-update command | differential + conflict test |
| positional source/extra argument | differential |
| update target matrix (`self`, `pi`, positional, `--extension`, `--all`, `--models`, default note) | target matrix test |
| conflict precedence (`--all` vs flags/source, `--models` vs flags/source, `--extension` vs flags/source, positional vs flags) | conflict test + differential |
| help wins over validation | help test |
| missing install/remove source | differential + validation test |
| config command unknown option/unexpected argument | differential |
| package list: empty, user/project sections, filtered marker, installed path | list test |

## Gaps

Execution half of `handlePackageCommand`/`handleConfigCommand` is not wired:
- `createCommandSettingsManager` needs `SettingsManager`, `ProjectTrustStore`
  (ported), `DefaultResourceLoader::loadProjectTrustExtensions`,
  `resolveProjectTrusted`, and `createProjectTrustContext` (ca-settings-resources).
- `refreshModelCatalogs` needs `ModelRuntime::create/refresh` (ca-models).
- `getSelfUpdatePlan`/`runSelfUpdate` need `getLatestPiRelease` (ported in
  `utils::version_check`), `detectInstallMethod`/`getSelfUpdateCommand` (ported in
  `config`), and the note renderer (`Markdown`/TUI).
- `selectConfig` TUI (config-selector) is not ported.
- `prepareWindowsNpmSelfUpdate` (Windows-only quarantine) is ported in
  `utils::windows_self_update` but not wired here.

The action model (`PackageAction`) is the seam those modules will consume.

## Proof

- `cargo test -p pi-coding-agent --test package_manager_cli` (7 tests)
- `cargo test -p pi-coding-agent --test differential_package_manager_cli` (2 tests, 36 argument cases)
- `cargo clippy -p pi-coding-agent --all-targets -- -D warnings`
