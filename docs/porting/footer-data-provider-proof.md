# Footer provider / readonly session checkpoint

Verified:2026-10-24. Frozen [common pin](source-lock.json): `core/footer-data-provider.ts`456lines and approved prerequisite `core/readonly-session-manager.ts`45lines. [Footer branch map](traceability/footer-data-provider.md), [readonly map](traceability/readonly-session-manager.md), [footer contract](../specs/footer-data-provider.md).

## Capability coverage

| Surface | Current capability / evidence | Missing or excluded |
|---|---|---|
| Metadata/process/executable | Regular/worktree/reftable discovery; raw paths/text; correct metadata-kind upward walk; fixed git argv/stdio/UTF8/trim/default1MiB limits and overflow TERM | No missing native branch; platform proof and approved runtime boundaries are explicit in the map |
| Watch/cache/lifecycle | Real System watcher error early returns; HEAD-only WSL polling;500ms debounce/pending/finally;5s error retry; cache/read/dispose/CWD ordering | Runtime error event injection only after real deletion/chmod/private-proc attempts failed to produce one; no System-constructor substitution |
| Listeners/status/count | Live branch callback removal/addition/disposal, original synchronous and scheduled errors; ordered per-read pure statuses; complete f64 count | Impure map-mutating status callbacks retain the existing approved snapshot gap |
| Session view |21 live methods with owned object/tree snapshots and explicit goal mutators;128-descendant isolation and exact storage errors | Approved typed-context exclusion for custom structural managers; native concrete SessionManager input. TS-only readonly Pick is a type projection, not a new trait |
| Source discrepancy | Transient old-repo completion after setCwd, then self-correction | Parent explicitly preserves direct→next→direct flash; no delta/generation guard |

## Current bounded runtime proof

All commands use Cargo's configured isolation guard. Logs and exact command/revision/invalidation ledger: `/tmp/pi-rust-repin-lossless-interactive/footer-provider-proof-ledger.md`. Individual genuine RED logs remain there; no ignored test or relaxed assertion.

| Command scope | Relevant revision / artifact | Result |
|---|---|---|
| `cargo test -j4 -p pi-coding-agent-tools --lib core::footer_data_provider -- --nocapture` |2a20b7398; `footer-lib-final.*` |4/4 tests,14 actual-source watcher/poll/refresh scenarios;29.48s |
| Footer/readonly/default/status/getter consumer targets | Same source; `footer-consumers-final.*` |45/45 distinct cases across12 original targets; scheduled process case later moved into existing differential target without a duplicate |
| `--test child_process --test child_process_kill_signal --test differential_ls_rtk_io --test ls_rtk` | Same source; `footer-shared-capture-final.*` |14/14; original default KILL controls and Node TERM trap/closed-pipe side effects |
| `--test interactive_components_c footer` | Same source; `footer-render-consumers-final.*` |4/4 |
| Moved scheduled-error case plus remaining adapted component/example fixtures |f7bae08cd; `footer-moved-uncaught-final.*`, `footer-adapter-render-final.*`, `footer-adapter-example-final.*` |1 already-counted case +3 additional cases GREEN |

Composite:70 distinct passing cases (4+45+14+4+3). Nested child-test repetition in logs is not counted. Every new production change has a behavioral or missing-typed-API RED before implementation. Existing correct refresh behavior is revalidated with actual frozen process/watcher scenarios rather than changed to create a RED.

## Compile, format and source proof

| Gate | Scope / result |
|---|---|
| Pinned strict no-emit TypeScript | Five changed oracles plus unchanged frozen footer/readonly modules PASS; `footer-strict-typescript-final.log`. Temporary ES2024 target only under standing [TS1501 approval](oracle-gate.md#frozen-v-target-discrepancy-2026-10-24); builder/source bytes unchanged |
| Biome | Five changed oracles PASS; `footer-biome-final.log` |
| Workspace all-target check |f7bae08cd:0 errors/0 warnings; `footer-workspace-current.*` |
| All-target/all-feature warning clippy | Workspace passes atf7bae08cd; one owned test borrow then removed at12e220d71 and its exact target recompiled. Composite0 owned changed-hunk findings againstaaecdac2;1677 outside-owned warnings. `footer-clippy-owned-final.json` and focused `footer-clippy-borrow-fix.*` |
| Readability/format | Manual Rust/TS changed-code audit; watcher registration/polling/publication stages named explicitly. No suppression or automated metric claim (metric tools unavailable). `cargo fmt --all --check` PASS before commits |

The sole code change after workspace0/0 removes an unnecessary borrow in a fixture; focused clippy recompiles that target, and no runtime/production behavior proof is invalidated. No repeated broad runtime suite was run for a milestone.

## Shared prerequisite and integration

Shared async capture takes an explicit kill signal, destroys/awaits captured readers before signaling and reaps. Existing wait/stream/RTK callers retain explicit SIGKILL; footer's execFile path uses SIGTERM. Actual Node/Python trap RED→GREEN proves the signal and EPIPE side effect, not just a return label. No alternate escalation or new dependency/feature is added; Cargo.lock is unchanged versusaaecdac2.

Parent owns integration and independent verification. No push, deployment, live provider/repository mutation or spawned agent is included. Coverage/source-inventory reconciliation is recorded in the final checkpoint commit; locked totals remain686 files/186679lines.
