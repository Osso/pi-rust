# CLI Mode

This specification covers the direct-module `Mode` union from `packages/coding-agent/src/cli/args.ts:13` at pinned TypeScript commit `532f7d12cd19235081bd4d6e425bde185652d86f`. The coding-agent package index exports only `Args` and `parseArgs` from this module at `index.ts:3`; it omits named `Mode`. Rust therefore exposes only `pi_coding_agent::cli::args::Mode`. Focused production is committed as `f9a20d1`; differential/typeproof proof is committed as `95fbc1f`.

## What it must do

- [x] Expose exactly `Text`, `Json`, and `Rpc` variants.
- [x] Derive exactly `Debug`, `Clone`, `Copy`, `PartialEq`, and `Eq`.
- [x] Return exact literals `"text"`, `"json"`, and `"rpc"` from `as_str`.
- [x] Expose the direct `cli::args` module without a `cli` or crate-root `Mode` re-export.
- [x] Cover variants, literals, equality, copying, traits, and distinctness in the focused test.
- [x] Compare source-shaped literals through exactly one differential request `13176`, containing `text`, `json`, and `rpc` in source order.
- [x] Prove the exact direct-module TypeScript union with a standalone typeproof; no package-index import or export claim.

## How it works

- [CLI Mode traceability](../porting/traceability/cli-mode.md)

## Implementation inventory

- `crates/pi-coding-agent/src/cli/args.rs` — closed enum and literal mapping.
- `crates/pi-coding-agent/src/cli/mod.rs` — public direct `args` module.
- `crates/pi-coding-agent/tests/cli_mode.rs` — focused behavioral/API proof.
- `crates/pi-coding-agent/tests/differential_cli_mode.rs` and `.../differential_cli_mode/mod.rs` — one-request direct-module differential proof.
- `oracle/cli-mode-oracle.ts` — direct-module JSONL oracle.
- `oracle/cli-mode-typecheck.mjs` — source-lock-verified strict direct-module union proof.
- `run-tests.sh` — focused, differential, and oracle registrations.

## Bounded differential/typeproof proof

- [x] Focused RED exits 101 because `pi_coding_agent::cli::args` is absent at base revision `2ba1d17`; no other diagnostic is reported.
- [x] Focused GREEN records 1 passed and 0 failed after the direct module was added and redundant assertions were removed.
- [x] Differential RED `/tmp/pi-rust-cli-mode-differential-red.out` exits 101 at `f9a20d1` because the skeleton returns structured `Error: unsupported operation: cli_mode`, not because of startup or JSONL failure.
- [x] Differential GREEN `/tmp/pi-rust-cli-mode-differential-green.out` records exactly 1 passed and 0 failed for request `13176`.
- [x] Targeted direct oracle source typecheck, standalone direct-module typeproof, aggregate `./run-tests.sh oracle`, Node syntax, denied-warning Clippy, formatting, diff, and changed Rust/TypeScript/JavaScript readability artifacts pass.
- [x] The standalone proof uses the pinned source's own `utils/highlight-js-lib-index.d.ts` declaration for the transitive module graph; it adds no declaration copy, suppression, dependency, or package-index claim.
- [x] Because the TypeScript alias is erased at runtime, the oracle proves direct-module loadability and compares only source-shaped typed literals; the standalone typeproof proves the exact union.
- [x] Final integration at code/proof HEAD `95fbc1f`: workspace `273 passed, 0 failed, 2 ignored`; workspace check, all-target/all-feature Clippy with `-D warnings`, aggregate oracle/typeproof, Node syntax, formatting, slice-range diff, changed-file readability, documentation, dependency/lock, and clean-pin gates pass.
- [x] The Rust worktree is clean at `95fbc1f`; the source worktree is clean at `532f7d12cd19235081bd4d6e425bde185652d86f`.
- [ ] Inherited independent verification remains pending.

No dependency change or deployment applies to this slice.

## Out of scope

`Args`, `parseArgs`, `isValidThinkingLevel`, `VALID_THINKING_LEVELS`, parsing, help, dispatch, runtime mode consumers, TUI/RPC/text behavior, constructors, `FromStr`, `Display`, serde, validation, invalid runtime types, `cli`/crate-root re-exports, dependencies, protocol/agent-core behavior, and unrelated APIs remain excluded.
