# Coding-agent crate split

Contract: [coding-agent crate layers](../specs/coding-agent-crate-layers.md).

## Dependency graph

`pi-coding-agent` (facade) → `pi-coding-agent-interactive` → `pi-coding-agent-extensions` → `pi-coding-agent-core` → `pi-coding-agent-base`.

Direct edges also let the facade use all lower layers and interactive use core. The extensions-to-interactive back-edge is removed by putting the shared footer in core; interactive already uses agents-core session/view adapters.

## Boundary changes

- Base owns `utils`, `config`, the unchanged process console writers, and internal-runner mode constants. Configuration embeds the original facade's `package.json` and theme bytes, not metadata from a new package.
- Core owns `core`, `cli::args`, shared `modes::extension_actions`, Supervisor `client`, `project_resolver`, `approval_reviewer`, `ensure_running`, and `request_wake`. UI leaves in core are `theme`, `diff`, `keybinding_hints`, `visual_truncate`, `footer`, and `footer_format`. Old facade and interactive paths re-export these modules.
- The unchanged `runtime_process_identity` implementation lives in core, with an old-path bundled-extension re-export. Callback error conversions live with their core-owned argument types. Compiler-proven cross-crate `pub(crate)` uses become `pub`; no other visibility is widened.
- Relative unit-test support symlinks are `crates/pi-coding-agent-core/tests` and `crates/pi-coding-agent-interactive/tests`, each targeting `../pi-coding-agent/tests`. Automatic integration-test/example/bench discovery is disabled in new library packages; all existing targets stay in the facade. Each consuming test crate includes its own oracle helper because dependency crates do not expose `cfg(test)` modules. Nested includes retain their original support files.
- Three facade tests directly include implementation files/assets, rather than importing the facade. Only those include paths change: `repin_control_retention_clock`, `differential_syntax_highlight`, and `differential_syntax_highlight_grammars`. Public test imports are unchanged. Moved unit tests keep references to the original facade fixture and asset directory.

## Inventory and documentation

Rust implementation paths are changed mechanically in coverage and traceability documents. No locked TypeScript counts or statuses change. `scripts/source-inventory.py` contains no moved Rust paths and is unchanged.

## Proof ledger

Baseline revision: `9ac6affc`. Artifacts: `/tmp/pi-crate-split-proof/` on the development host. Warning-mode workspace/all-target/all-feature clippy baseline: **1,686 warning JSON messages, zero errors**. Incremental compilation stays disabled.

| Touched layer | Before check | Before debug build |
|---|---:|---:|
| core (`model_resolver.rs`) | 48.61s | 51.91s |
| utils (`ansi.rs`) | 24.39s | 57.82s |
| interactive (`footer.rs`) | 32.48s | 58.65s |
| extensions (`codex_usage/mod.rs`) | 36.29s | 1m 17s |

Commands: `cargo check -j4 -p pi-coding-agent` and `cargo build -j4 -p pi-coding-agent --bin pi`, after touching each file. These values are Cargo's reported Finished times. The first core check also rebuilt feature-dependent lower packages after the all-feature clippy baseline. Concurrent-machine 1-minute load was 25.38 (core), 29.48 (utils), 25.58 (interactive), and 22.68 (extensions); baseline initialization load was 19.30.

Checkpoint proof is pending. Pure-move commits `753f8441` and `d302947e` preserve source bytes; later commits contain the listed boundary and path edits. No runtime fallback, callback registry, profile change, push, or deployment is part of this split.
