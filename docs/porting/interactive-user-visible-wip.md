# Interactive user-visible parity — active successor

## Authorized scope

User-observable `interactive-mode.ts` startup/render/events and command/session-transition slices, `agent-session-runtime.ts`, `self-restart.ts` and `keybindings.ts`, against common pin `4f9e6f8a4`. Keep existing lossless carriers. Mechanical cross-lane adapters are cleared; semantic deviations/dependencies/TS deltas still need approval. No agents, full-suite runs, push, merge, deploy or live providers. Parent owns integration and independent verification.

## Baseline repair checkpoint (2026-10-25)

- `fbe31b1ec`: fixture streams capture their own provider instead of the global `faux` API. Deterministic RED receives another session's answer after registration/refresh; GREEN preserves each complete turn. This fixes the notification waits behind both baseline hangs and the tree/fork fixture collision without timeouts or serialization.
- `da8ea24dc`: authentication snapshot assertions use existing lossless `JsValue` Number semantics. Full model/count/chat/error objects are unchanged; all nine source scenarios pass. The failures were serde `Number(1.0)` versus `Number(1)` equality, not product values.
- `aa2a3af94`: live expansion now follows frozen :5411–5425: flag → active header → resource/chat children → render, without the old output-padding write. Ctrl+O reveals the background-tool hint and Ctrl+B still detaches. The actual header oracle covers 48 complete original frames across four startup states and four header registrations/restorations.
- `ac3c00885` plus formatting follow-up: strict types for the AST/oracle and source-selected compiler proof. No frozen source changes.

Proof ledger root: `/tmp/repin-interactive-2-proof/`. Rust source scope is `aa2a3af94`; later oracle-only type/format edits do not invalidate Rust compilation/clippy or the other eight tests.

| Command / scope | Result | Artifact / invalidation |
|---|---|---|
| `cargo test -j4 -p pi-coding-agent-interactive --lib --` with the two hangs, two auth tests, background keybinding, tree, fork, fixture ownership and header filters | 9/9 parallel, no ignores, 14.27s | `baseline-parallel-green.log`; current Rust and pre-type-only oracle |
| Same command, only header filter, final oracle | 1/1; all 48 frames | `header-final-runtime.log` |
| `cargo check -j4 -p pi-coding-agent-interactive --lib` | PASS, zero warnings/errors | `check.log`; current Rust |
| `cargo clippy -j4 -p pi-coding-agent-interactive --lib --tests --no-deps --message-format=json` | PASS; 102 existing diagnostics, zero primary spans intersect changed Rust lines | `clippy-baseline.{jsonl,stderr}`; current Rust; not whole-crate warning-clean |
| Frozen `tsgo --project` via `buildSourceTsconfig`, header oracle only, target/lib ES2024 under oracle-gate approval | PASS, strict/noEmit, original source/compiler | `header-final-typecheck.log`; formatting-only follow-up does not change types |
| Frozen Biome, header oracle only | PASS | `header-biome.log` records initial info; final formatter applied and clean check reported at checkpoint |
| `cargo fmt --all --check` | PASS before each commit | Current Rust |
| `python3 scripts/source-inventory.py` | PASS, 686 / 186679; 467 done / 19 gap / 47 partial / 153 todo | Baseline checkpoint; no status promotions |

Changed-line readability review: fixture ownership is explicit; expansion is a short ordered sequence and one bounded native-type dispatcher; oracle frame capture names each surface and transition. No new warning suppression, dependency, alternate provider path or carrier is added.

## Next startup slice

Frozen init :985–1112 still differs from native init:

- Emit the configured model-scope console line before mounting the UI.
- Invoke the complete rebind path (provider count, editor border, title and cwd), not just extension binding.
- Register resource themes and show resources/startup notices after extension binding; restored messages remain after resources.
- Subscribe footer branch-change renders and prove current startup/restored-session frames.

These are observed source/control-flow differences, not yet behavioral REDs. Add representative normal/quiet/scoped/restored actual-source initialization/rebind proofs before production changes. Existing header/reload/session-option oracles remain available. No whole-mode/runtime/restart/keybindings acceptance is claimed.
