# CLI Mode traceability

Pinned source: `packages/coding-agent/src/cli/args.ts:13` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and focused proof |
|---|---|---|
| `args.ts:13` | Closed union with exact distinct values `"text"`, `"json"`, and `"rpc"`. | `pi_coding_agent::cli::args::Mode::{Text, Json, Rpc}` with exact `as_str`; the Rust value representation derives `Debug`, `Clone`, `Copy`, `PartialEq`, and `Eq`, and `cli_mode.rs` proves its literals, trait bounds, copying, equality, and distinctness. |
| `index.ts:4` | The package index exports `Args` and `parseArgs`, but omits named `Mode`. | `cli::args` is public; `cli` and crate root add no `Mode` re-export. No package-index export claim is made. |
| `args.ts:13` | Differential proof must compare only final source-shaped literals. | Request `13176` contains `text`, `json`, and `rpc` in source order; the Rust response is a plain `{ id, ok, output: { values } }` record. |
| `args.ts:13` | Type proof must check the exact direct-module union without parser behavior. | `oracle/cli-mode-typecheck.mjs` imports `Mode` only from the pinned direct `cli/args.ts`, proves the exact union and rejects `""`, `"tui"`, and `"RPC"`; it does not import the package index. |

## Rust boundary

- `Mode` is a closed enum with exactly `Text`, `Json`, and `Rpc`.
- `as_str` is the only conversion API and returns the exact source literals.
- The module path is `pi_coding_agent::cli::args::Mode`; no `cli` or crate-root re-export is added.
- No parser, validation, dispatch, consumer, serialization, or runtime invalid-input behavior is included.

## Focused proof status

- Focused RED: `/tmp/pi-rust-cli-mode-red.out` records `cargo test -p pi-coding-agent --test cli_mode` at revision `2ba1d17`, exit 101, with only unresolved `pi_coding_agent::cli::args`.
- Focused GREEN: `/tmp/pi-rust-cli-mode-green.out` records 1 passed and 0 failed after duplicate test assertions were removed.
- Targeted Clippy: `/tmp/pi-rust-cli-mode-clippy.out` records `cargo clippy -p pi-coding-agent --test cli_mode -- -D warnings`, exit 0.
- Formatting and diff: `/tmp/pi-rust-cli-mode-fmt.out` and `/tmp/pi-rust-cli-mode-diff.out` record exit 0.
- Rust readability: `/tmp/pi-rust-cli-mode-readability.out` records the changed-file audit with no new threshold violation or suppression.

## Differential/typeproof proof

- Differential RED: `/tmp/pi-rust-cli-mode-differential-red.out` records request `13176` at Rust revision `f9a20d1`, exit 101, with the expected structured `Error` response and message `unsupported operation: cli_mode` from the skeleton oracle.
- Differential GREEN: `/tmp/pi-rust-cli-mode-differential-green.out` records the first exit-0 run with exactly 1 passed and 0 failed; after a formatting-only line wrap, `/tmp/pi-rust-cli-mode-differential-green-current.out` confirms the current source still passes 1/0. The oracle echoes only the three typed final literals.
- Direct oracle source typecheck: `/tmp/pi-rust-cli-mode-oracle-source-typecheck.out` exits 0 against pinned source HEAD `532f7d12cd19235081bd4d6e425bde185652d86f`.
- Standalone direct-module typeproof: `/tmp/pi-rust-cli-mode-typeproof-final.out` exits 0. Its generated fixture includes the pinned source's existing `packages/coding-agent/src/utils/highlight-js-lib-index.d.ts` as a root file for the transitive module graph; no declaration copy, source suppression, dependency, or package-index claim is used.
- Aggregate oracle/typeproof: `/tmp/pi-rust-cli-mode-oracle.out` records `./run-tests.sh oracle` exit 0 after the standalone proof was corrected to consume that pinned declaration.
- Node syntax: `/tmp/pi-rust-cli-mode-node-oracle-check-final.out` and `/tmp/pi-rust-cli-mode-node-typeproof-check-final.out` exit 0.
- Targeted Clippy, formatting, diff, and readability: `/tmp/pi-rust-cli-mode-clippy-final-2.out`, `/tmp/pi-rust-cli-mode-fmt-final.out`, `/tmp/pi-rust-cli-mode-diff-final.out`, `/tmp/pi-rust-cli-mode-readability-final.out`, and `/tmp/pi-rust-cli-mode-readability-typeproof-final.out` exit 0 with no suppression or threshold finding.

## Pending proof

- Broad workspace/static, final integration, documentation closeout, clean-pin, and inherited verification remain pending.

## Exclusions

`Args`, `parseArgs`, `isValidThinkingLevel`, `VALID_THINKING_LEVELS`, parsing, help, dispatch, runtime mode consumers, TUI/RPC/text behavior, constructors, `FromStr`, `Display`, serde, validation, invalid runtime types, `cli`/crate-root re-exports, dependencies, protocol/agent-core behavior, and unrelated APIs remain excluded.
