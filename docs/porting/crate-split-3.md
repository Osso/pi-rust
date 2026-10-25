# Crate split, round 3: build performance

Contract: [crate layers](../specs/coding-agent-crate-layers.md). [Detailed proof ledger](build-performance.md). Baseline parent: `09f10e9c`; incremental checkpoint: `b5d47697a`; UI split/proof checkpoint: `963998e48`. Main accepted the UI split on 2026-10-25 for its modest cold-check improvement and independent widget dependency graph. No further profile or split changes are justified by these measurements.

## Retained changes

- Incremental compilation is enabled for development and tests. Initial cache population cost 2m19s check / 2m27s pi build. All-target incremental state was 5.5G, versus the former hundreds-of-test-binaries problem addressed by the parallel consolidation lane.
- `pi-coding-agent-ui` owns 45 byte-identical moved files / 15,397 lines: widgets, model search and login options. It depends on core, not bundled extensions. Old interactive/facade module paths remain re-exported. No test assertions, discovery, runner, implementation algorithms or external dependencies changed.

## Measurements

Cargo Finished times, `-j4`; uptime/load and raw logs are in `/tmp/pi-build-perf-proof/`. Host load is variable, so these are observations, not universal speed guarantees.

| Probe | Before | After |
|---|---:|---:|
| Incremental: runtime touch check | 28.19s | 5.30s |
| Incremental: runtime touch pi build | 1m11s | 8.57s |
| Incremental: tools touch check | 49.42s | 9.46s |
| Incremental: tools touch pi build | 1m10s | 11.55s |
| UI split: nonincremental mode touch check | 9.22s | 7.95s |
| UI split: nonincremental mode touch pi build | 16.96s | 16.62s |
| UI split: incremental widget/mode touch check, rapid A/B | 2.38s / 2.00s | 5.61s / 8.51s |
| UI split: incremental widget/mode touch pi build, rapid A/B | 6.27s / 5.61s | 15.82s / 12.09s |
| UI split: equivalent widget/mode body-edit check | 3.59s / 5.91s | 10.39s / 13.89s |
| UI split: equivalent widget/mode body-edit pi build | 9.85s / 8.69s | 15.04s / 8.87s |

Incremental before/after load ranges were approximately 45–53 / 41–51. Cold UI check/build comparison loads were approximately 32–36 / 37–38. Rapid hot A/B loads rose approximately 32→45; body-edit runs used approximately 40–44 before and 49–51 after. Hot samples do not establish a split benefit or a causal regression. Temporary body edits introduced an equivalent local return binding and were fully restored. Earlier, strongly load-confounded UI samples remain in the detailed ledger; they are not the split's acceptance evidence.

Fresh pi build at the baseline took 4m19s; initial check population took 2m17s. A full workspace no-run baseline was stopped when main reassigned linker/consolidation work and requested only representative measurements. This lane makes no full-suite equivalence claim. Linker and consolidation results belong to the parallel lanes.

## Rejected tuning and disk

`profile.dev.debug=0` was not kept: initial pi build 5m03s; runtime/tools touch builds 1m13s / 1m17s at load approximately 87–96. There was no supported benefit over existing line-table diagnostics. Codegen-unit/dependency-optimization/split-debug changes were not applied without evidence of a remaining hotspot. Build/test Tokio feature sets differ (`rt-multi-thread`/`test-util`); unifying them would change the production feature set, so this is not included. Existing image versions belong to separate pinned dependency trees.

Target size: 67G after the aborted baseline, 71G after initial incremental warming, 85G after experiments/verification. Removed 997 stale test executables from the aborted baseline, releasing 61.5GiB; final own target is 24G. No other worktree artifacts were removed.

## Proof

At UI code `5cfabf6dd`, lockfile recorded by `963998e48`: workspace all-target check PASS, 0 errors / 0 warnings; inventory PASS, 686 rows / 186,679 locked TS lines; focused facade tests identical before/after (9 passed / 1 ignored), relocated UI unit tests 4/4 PASS. Targeted clippy diagnostics are exactly unchanged after path normalization: 576 before / 576 after, no added or removed warning signatures. Formatting passes. Captured pi version/help output is identical. All 45 moved implementations compare byte-identical. Main owns full-suite integration acceptance after consolidation.
