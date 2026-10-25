# Coding-agent package-manager CLI

Frozen: `packages/coding-agent/src/package-manager-cli.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d` (779 lines).
Native: `crates/pi-coding-agent/src/package_manager_cli.rs`.

**Done at the user-observable parity bar (2026-10-25).** Whole-module audit plus existing tests. Native regressions cover each found discrepancy; no new frozen differential or matrix. [Contract](../../specs/package-command-errors.md).

## Exports and branches

- `PackageCommand` maps to the native enum; parsing/alias/help/validation priority and update-target selection are tested in `tests/package_manager_cli.rs` and the existing `differential_package_manager_cli.rs` frozen output tests. `handlePackageCommand` maps to validation plus `execute_package_command`, preserving UTF-16 argv and diagnostic/output text.
- `PackageCommandRuntimeOptions` maps to the existing host effects' extension factories. Settings creation, saved trust for update, command trust override/default/extension decisions, warnings and local-write rejection use the real SettingsManager/ProjectTrustStore and production loader.
- `handleConfigCommand` maps to validation plus `execute_config_command`: non-config argv is unhandled; only trust flags affect config; old local/extra arguments are ignored. One package resolution result goes to the concrete TUI selector. Existing real-terminal proof is retained.
- Install/remove/list/update execution, progress filtering, error catch boundaries, user/project list order, extension-before-self ordering, current/newer/force/renamed release plans, unavailable installation instructions, notes, child steps, nonzero exit/signal/spawn errors and fallback instructions are covered by `tests/package_command_execution.rs`. Effects injection does not replace the real command settings/trust setup in those tests.
- Self-update reads global npmCommand, not merged project settings. It uses the existing release/config helpers and propagates the complete frozen release-lookup error. Windows-only quarantine/restriction paths remain the repository platform gap.

## Successor corrections

- List failures previously escaped the frozen package catch. Native raw-UTF-16 error test now observes `Error: <message>` and exit 1.
- Self-update previously read the project's npm override. A native fixture with two real executable probes now proves global-only selection.
- Spawn failures used OS descriptions and termination used numeric signals. Native missing-executable and SIGTERM regressions now require Node text.
- Empty install/extension values follow frozen truthiness, including a later nonempty positional source. Native argument test covers all three cases.
- DefaultPackageCommandEffects now reads live stdout winsize for notes rather than always using 80. A 37-column stdout PTY with non-TTY stdin proves the actual host adapter. The safe winsize operation is exposed through `pi_tui::terminal::stdout_columns`; no dependency was added.
- Interactive project-trust used no startup UI and silently became untrusted. The process-terminal adapter now displays the existing startup selector. Real package-list and main-startup PTYs assert the saved Trusted decision; errors propagate through Result. [Main wiring](main.md).

## Proof

- Initial native RED: `/tmp/settings-models-package-red.log`; final execution GREEN: `/tmp/settings-models-package-merged-native.log` (33 tests), including the empty-argument regression.
- Existing parser/help/validation proof: `/tmp/settings-models-package-parser-green.log` (11 tests, including two frozen differentials).
- Width RED: `/tmp/settings-models-package-width-red.log`; trust PTY RED/GREEN: `/tmp/settings-models-trust-pty-{red,green}.log`.
- Existing stale lookup assertion now requires `Could not determine latest pi version: offline`; no assertion was loosened.
- Final checkpoint/revision ledger: [successor proof](../settings-models-successor-proof.md).

## Gaps and source quirks

Frozen `config` ignores non-trust arguments, including help/local tokens; this is preserved, not replaced with extra validation. V8 stack text is omitted under the existing approval. Adversarial/wrong-typed JSON, numeric extremes, unsupported OS/runtime faults and scheduling races follow AGENTS.md. There is no remaining loader, startup UI, sorted-provider-error or model-refresh seam in this module: the latter old-only update target was already removed before the successor audit.
