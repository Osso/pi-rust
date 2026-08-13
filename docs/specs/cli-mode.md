# CLI Mode

This specification covers the direct-module `Mode` union from `packages/coding-agent/src/cli/args.ts:13` at pinned TypeScript commit `532f7d12cd19235081bd4d6e425bde185652d86f`. The coding-agent package index exports only `Args` and `parseArgs` from this module at `index.ts:4`; it omits named `Mode`. Rust therefore exposes only `pi_coding_agent::cli::args::Mode`.

## What it must do

- [x] Expose exactly `Text`, `Json`, and `Rpc` variants.
- [x] Derive exactly `Debug`, `Clone`, `Copy`, `PartialEq`, and `Eq`.
- [x] Return exact literals `"text"`, `"json"`, and `"rpc"` from `as_str`.
- [x] Expose the direct `cli::args` module without a `cli` or crate-root `Mode` re-export.
- [x] Cover variants, literals, equality, copying, traits, and distinctness in the focused test.
- [ ] Compare source-shaped literals through differential request `13176`.
- [ ] Prove the direct-module TypeScript union with a standalone typeproof.

## How it works

- [CLI Mode traceability](../porting/traceability/cli-mode.md)

## Implementation inventory

- `crates/pi-coding-agent/src/cli/args.rs` — closed enum and literal mapping.
- `crates/pi-coding-agent/src/cli/mod.rs` — public direct `args` module.
- `crates/pi-coding-agent/tests/cli_mode.rs` — focused behavioral/API proof.
- `run-tests.sh` — focused registration only; differential and oracle registrations remain pending.

## Focused production proof

- [x] Focused RED exits 101 because `pi_coding_agent::cli::args` is absent at base revision `2ba1d17`; no other diagnostic is reported.
- [x] Focused GREEN records 1 passed and 0 failed after the direct module was added and redundant assertions were removed.
- [x] Targeted denied-warning Clippy, formatting, diff, and changed-Rust readability artifacts pass.
- [ ] Differential, direct-module typeproof, broad workspace/static, final, and inherited verification remain pending.

## Out of scope

`Args`, `parseArgs`, `isValidThinkingLevel`, `VALID_THINKING_LEVELS`, parsing, help, dispatch, runtime mode consumers, TUI/RPC/text behavior, constructors, `FromStr`, `Display`, serde, validation, invalid runtime types, `cli`/crate-root re-exports, dependencies, protocol/agent-core behavior, and unrelated APIs remain excluded.
