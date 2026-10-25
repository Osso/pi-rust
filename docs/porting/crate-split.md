# Coding-agent crate split

This is the round-1 checkpoint. [Round 2](crate-split-2.md) splits core further under the same compatibility contract.

Contract: [coding-agent crate layers](../specs/coding-agent-crate-layers.md).

## Dependency graph

`pi-coding-agent` (facade) → `pi-coding-agent-interactive` → `pi-coding-agent-extensions` → `pi-coding-agent-core` → `pi-coding-agent-base`.

Direct edges also let the facade use all lower layers and interactive use core. The extensions-to-interactive back-edge is removed by putting the shared footer in core; interactive already uses agents-core session/view adapters.

## Boundary changes

- Base owns `utils`, `config`, the unchanged process console writers, and internal-runner mode constants. Configuration embeds the original facade's `package.json` and theme bytes, not metadata from a new package.
- Core owns `core`, `cli::args`, shared `modes::extension_actions`, Supervisor `client`, `project_resolver`, `approval_reviewer`, `ensure_running`, and `request_wake`. UI leaves in core are `theme`, `diff`, `keybinding_hints`, `visual_truncate`, `footer`, and `footer_format`. Old facade and interactive paths re-export these modules.
- The unchanged `runtime_process_identity` implementation lives in core, with an old-path bundled-extension re-export. Callback error conversions live with their core-owned argument types. The unchanged `ResidentConsoleService::as_str` inherent impl moves from the CLI module to its owning core module. Compiler-proven cross-crate `pub(crate)` uses become `pub`; no other visibility is widened.
- Relative unit-test support symlinks are `crates/pi-coding-agent-core/tests` and `crates/pi-coding-agent-interactive/tests`, each targeting `../pi-coding-agent/tests`. Automatic integration-test/example/bench discovery is disabled in new library packages; all existing targets stay in the facade. Each consuming test crate includes its own oracle helper because dependency crates do not expose `cfg(test)` modules. Nested includes retain their original support files.
- Three facade tests directly include implementation files/assets, rather than importing the facade. Only those include paths change: `repin_control_retention_clock`, `differential_syntax_highlight`, and `differential_syntax_highlight_grammars`. Public test imports are unchanged. Moved unit tests keep references to the original facade fixture and asset directory.

## Inventory and documentation

Rust implementation paths are changed mechanically in coverage and traceability documents. No locked TypeScript counts or statuses change. `scripts/source-inventory.py` contains no moved Rust paths and is unchanged.

## Proof ledger

Baseline revision: `9ac6affc`. Artifacts: `/tmp/pi-crate-split-proof/` on the development host. Warning-mode workspace/all-target/all-feature clippy baseline: **1,686 warning JSON messages, zero errors**. Incremental compilation stays disabled.

| Touched layer | Before check | After check | Before debug build | After debug build |
|---|---:|---:|---:|---:|
| core (`model_resolver.rs`) | 48.61s | 41.49s | 51.91s | 51.13s |
| utils (`ansi.rs`) | 24.39s | 36.19s | 57.82s | 1m 04s |
| interactive | 32.48s | 7.67s | 58.65s | 15.91s |
| extensions (`codex_usage/mod.rs`) | 36.29s | 15.83s | 1m 17s | 21.72s |

Commands: `cargo check -j4 -p pi-coding-agent` and `cargo build -j4 -p pi-coding-agent --bin pi`, after touching each file. These values are Cargo's reported Finished times. The first core check also rebuilt feature-dependent lower packages after the all-feature clippy baseline. Concurrent-machine 1-minute load was 25.38 (core), 29.48 (utils), 25.58 (interactive), and 22.68 (extensions); baseline initialization load was 19.30.

After timings use source snapshot `1f00a821` (parent `95b94269` merged) with no content edits, after warming regular check/build feature sets. The interactive sample is `components/status_indicator.rs`; the before sample was `footer.rs`, which now belongs to core. Each measured layer touch rebuilds exactly that package and its dependents; the Cargo logs list those packages. After-touch 1-minute loads were **84.71 / 45.17 / 31.70 / 30.63** for core/utils/interactive/extensions. The utils check also waited for the shared package-cache lock. These are not controlled-load benchmark comparisons: interactive/extensions improve substantially, while utils does not improve in this run. Standalone `cargo check -j4 -p pi-coding-agent-core` after touching `model_resolver.rs` finished in **26.10s** at load 26.26, including feature-set dependency rebuilds. No second-round core split is indicated by this sample.

## Checkpoint results

Source/configuration snapshot: `1f00a821`, including parent `95b94269`. Final proof documentation adds no code changes. The check/clippy commands ran at `b7d3bf1d` with the generated lockfile subsequently committed unchanged as `1f00a821`.

| Proof | Result | Artifact in `/tmp/pi-crate-split-proof/` |
|---|---|---|
| `cargo check -j4 --workspace --all-targets --keep-going --message-format=json` | exit 0; 0 errors, 0 warnings; Finished in 1m 04s | `checkpoint-check.log` |
| `cargo clippy -j4 --workspace --all-targets --all-features --keep-going --message-format=json` | exit 0; 1,676 warnings, 0 errors; Finished in 1m 31s | `checkpoint-clippy.log` |
| `cargo test -j4 -p pi-coding-agent --no-run` | exit 0; all 699 executable test targets built; Finished in 5m 45s | `checkpoint-test-no-run.log` |
| `python3 scripts/source-inventory.py` | PASS; 686 rows / 186,679 locked TS lines | `checkpoint-inventory.log` |
| `differential_print_mode` / `output_accumulator` | 23/23 and 10/10 GREEN | `final-runtime-print-output.log` |
| `differential_openai_codex_responses -p pi-ai` | 38/38 GREEN | `checkpoint-runtime-ai.log` |
| `agent_session -- --test-threads=1` | 68 passed / 3 failed, identical to unsplit parent; explicit accepted checkpoint exception | `final-serial.log`, `parent-agent-session-serial.log` |
| subagent frozen tool-description differential | 1/1 GREEN | `checkpoint-subagent-description.log` |
| `target/debug/pi --version` / `--help` | exit 0; stdout byte-identical (7 / 10,166 bytes); stderr byte-identical and empty | `before-{version,help}.stdout`, `after-{version,help}.stdout` |
| formatting and changed-line readability | `cargo fmt --all --check` passes; manual Rust/TS changed-line audit finds no new structural complexity or suppressions | git diff plus this ledger |

The clippy decrease is **not** a lost check or a split effect. All ten removed warning signatures belong to the merged parent ExtensionStatus changes: four `clippy::useless_conversion` diagnostics in `bundled_extensions/pyrun/eval_tool.rs`, one `clippy::map_identity` in `tests/footer_data_provider.rs`, and five `clippy::map_clone` in the repeatedly included `tests/support/bundled_extensions.rs`. Normalizing moved paths yields zero added warning signatures. No lint is disabled.

The three pre-existing serial failures are `navigate_tree_moves_leaf_and_returns_editor_text`, `navigate_tree_summarizes_the_abandoned_branch_and_attaches_the_summary` (both editor text None instead of first), and `prompt_persists_user_and_assistant_messages` (four entries instead of three). The unsplit parent serial run at `6828bb7e` gives exactly the same assertions and 68/3 count; the final split run after `95b94269` retains them. Default-parallel parent/split runs also fail; the parent run was terminated after two tests remained active beyond eight minutes. Partial evidence is `parent-agent-session-parallel.log`; no parallel-pass claim is made. Main accepted this pre-existing exception, without test exclusions or behavior changes.

The separately authorized `6019805b` fixes a pre-existing codegen failure in the opt-in subagent example: the brace-free `js_format!` template interpolates the frozen literal `{previous}` as an argument. The real frozen-extension description differential passes. The older full subagent dispatch test exposes an independent inherited model/thinking mismatch and is not claimed GREEN; it remains outside this structural refactor.

## Content audit

Pure-move commits `753f8441` and `d302947e` contain **356 + 1 R100 renames**, respectively. Against merged parent `95b94269`, **330 of 357 moved files are still byte-identical**; the other 27 contain only the [enumerated boundary/path edits](crate-split-audit.md). Full evidence: `pure-move-*.log`, `diff-audit-name-status.log`, and `diff-content-audit.patch` in the artifact directory.

The lockfile adds exactly four local packages, removes no packages, and changes only the existing facade's dependency list. External package records are identical to the parent. `.cargo/config.toml`, workspace lints, and profile settings are unchanged. No runtime fallback, callback registry, push, or deployment is part of this split.
