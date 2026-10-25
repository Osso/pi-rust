# Whole RPC mode checkpoint

Verified: 2026-10-24. Frozen source: [source-lock](source-lock.json), `packages/coding-agent/src/modes/rpc/rpc-mode.ts` (827 lines). [Contract](../specs/rpc-mode.md), [complete branch/field trace](traceability/rpc-mode.md).

## Scope and coverage

| Surface | Capability | Proof |
|---|---|---|
| Commands/input/events | Literal JsValue server dispatch; raw ids/types/errors; strict model/entry lookup; trim/ToPrimitive throws; every frozen command case and preserved unknown thinking-level query | Full actual-source/native process matrix29/29 |
| Lifecycle | Owner-bound subscriptions; takeover before binding; first-turn TERM/HUP; repeated signals during pending disposal; persisted switch; next-input shutdown polling | Actual process RED→GREEN; shutdown polling is a preserved source discrepancy |
| Extension UI | Callable status, every one-way request, cancellation-first raw replies, empty replies, first-notification response/abort/timeout settlement, both theme setters, ignored factories/no-op getters | Process29 plus owned unit/surface/process-oracle9/9 |
| Native client DTOs | RpcResponse.command carries JsString; caller adaptation only | Response-types11/11; workspace all targets |
| Explicit exclusions | Approved per-field deep-core/image/typed-return rejection gaps, Node timer warning/precision gap, inherited-object/runtime and stack exclusions | Exact fields and source lines in traceability; no silent normalization/drop |

No unapproved mode branch is intentionally disabled. This checkpoint does not claim RPC client/types whole-module revalidation or whole interactive-mode completion.

## Code and integration

Functional code and fixture publication proof: `af555494` (includes latest parent merge `e14c4d5a`; crate-split parent also merged at `de1839e4`). Later checkpoint edits only reconcile tracked proof/spec/coverage.

- Original behavior REDs cover wrong subscription owner, missing stdout takeover, old-only user_bash, first-turn signal installation, raw command loss, raw external lookup/echo semantics, UI cancellation precedence, overflow timers, notification ordering and Theme-object rejection.
- First parallel integration:19/29 passed; ten native children failed before mode startup because the shared padded package directory was visible before its theme files were copied. `af555494` gives each invocation a unique PID/sequence directory of the same required length and completes copying before spawning. Parallel29/29 then passed; no serial-test switch or assertion relaxation.
- The shutdown-immediate-exit draft failed on frozen itself. Parent classified the discrepancy; the accepted test checks the next command response and normal exit0.

## Proof ledger

Artifacts: `/tmp/pi-rust-repin-lossless-interactive/`; command/revision/invalidation history: `rpc-mode-proof-ledger.md` there.

| Command | Relevant scope | Result/artifact |
|---|---|---|
| `cargo test -j4 -p pi-coding-agent --test differential_rpc_mode -- --nocapture` | af555494, parallel complete process target |29/29 GREEN; `rpc-mode-final-process-isolated.log` |
| `cargo test -j4 -p pi-coding-agent --lib modes::rpc::rpc_mode::tests -- --nocapture` | e14c4d5a; subsequent helper fixture change does not intersect these tests |9/9 GREEN; `rpc-mode-final-unit.log` |
| `cargo test -j4 -p pi-coding-agent --test rpc_types -- --nocapture` | ace7f540 carrier, unchanged afterward |11/11 GREEN; `rpc-mode-green-rpc_types-all.log` |
| `cargo test -j4 -p pi-coding-agent --test differential_print_mode print_text_prints_the_final_assistant_text -- --nocapture` | Shared package fixture's other owned consumer at af555494 |1/1 GREEN; `rpc-mode-shared-fixture-print-smoke.log` |
| `cargo check -j4 --workspace --all-targets --keep-going --message-format=json` | af555494 source plus checkpoint documentation |0 errors/0 warnings; `rpc-mode-workspace-final.jsonl` |
| `cargo clippy -j4 -p pi-coding-agent --all-targets --message-format=json` | Same source; owned hunks versus merged parent |0 owned findings;1174 diagnostics outside owned changed lines; `rpc-mode-clippy-final-owned.json` |
| `cargo fmt --all --check` | Same Rust source |PASS |
| Pinned `tsc -p /tmp/pi-rust-repin-lossless-interactive/rpc-mode-tsconfig.json` | All owned TS fixtures/oracles |PASS; `rpc-mode-typecheck.log` |
| Pinned `biome check` on the three owned RPC/mode TS files | Same fixture code |PASS; `rpc-mode-biome.log` |
| `python3 scripts/source-inventory.py` | Coverage reconciled to RPC done |PASS686 files/186679 lines;452 done/19 gap/48 partial/167 todo; `rpc-mode-inventory-final.log` |

Manual Rust/TS changed-code readability review completed. Effect-bearing domain handlers are explicitly named dispatch_*; callback errors retain the shared carrier. No dependencies, warning suppressions, compatibility path or warning subsystem were added. The optional Rust metric executable is unavailable, so no automated metric pass is claimed.
