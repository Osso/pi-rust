# Build-performance round

Contract: [crate layers](../specs/coding-agent-crate-layers.md). Parent: `09f10e9c`. Artifacts: `/tmp/pi-build-perf-proof/` on the development host. Measurements use Cargo's own Finished lines and `-j4`; uptime is recorded per touch. Main reassigned linker and test-consolidation work to parallel lanes and replaced the full baseline with representative before/after probes.

## Incremental compilation

Checkpoint `b5d47697a` changes only `.cargo/config.toml`, enabling incremental compilation for both development and tests. Test consolidation in the parallel lane bounds test cache duplication; this checkpoint does not change test discovery or the runner.

| Touch | Disabled check | Enabled check | Disabled pi build | Enabled pi build |
|---|---:|---:|---:|---:|
| Runtime: `core/agent_session.rs` | 28.19s | 5.30s | 1m11s | 8.57s |
| Tools: `core/tools/read.rs` | 49.42s | 9.46s | 1m10s | 11.55s |

Disabled touch loads were approximately 45–53; enabled loads approximately 41–51. Initial incremental warm-up cost: 2m19s check and 2m27s build. Target size grew from 67G to 71G; incremental state after workspace all-target check was 5.5G. The 67G starting size includes an aborted full workspace no-run: main explicitly stopped that baseline after work became parallel. No full-suite baseline or full-suite equivalence claim is made by this lane.

Proof at `b5d47697a`: formatting PASS; workspace all-target check PASS, zero warnings/errors, 1m59s; source inventory PASS, 686 rows / 186,679 locked lines.

## Independent interactive UI crate

The 45 files under `components/`, `model_search.rs` and `login_options.rs` move unchanged (15,397 lines) from interactive to `pi-coding-agent-ui`. The new crate depends on core, not bundled extensions. Interactive re-exports the old module paths and retains `InteractiveMode`; the facade's integration tests and runner are untouched. A root `ParsedSkillBlock` alias preserves an existing component import. No visibility or algorithm changes occur inside moved modules. Existing external versions/features remain unchanged.

Pure-move commit: `67b86fd5`; wiring: `d2556a1d1`, `0ffde3a86`, `5cfabf6dd`. The pure-move checkpoint intentionally precedes module wiring, so its formatting traversal cannot resolve the moved modules; final wiring formatting passes. All 45 source files compare byte-identical against `b5d47697a`.

Preliminary widget touch: check 16.47→2.79s, pi build 1m00s→6.04s. Mode touch: check 24.22→2.77s, pi build 29.90→5.52s. Host load fell substantially between these samples; these are observations, not controlled split speedup claims.

### Proof ledger

Code snapshot: `5cfabf6dd`; the lockfile records only the new local package and interactive-to-UI edge.

| Proof | Result | Log |
|---|---|---|
| Workspace all-target check | PASS, 0 errors / 0 warnings, 1m45s | `ui-workspace-check.log` |
| Facade components/status/tree differential tests | Before/after both 9 passed, 1 ignored, 0 failed | `ui-{baseline,after}-tests.log` |
| Relocated UI unit tests | 4/4 PASS | `ui-unit-tests.log` |
| Clippy, interactive/UI all targets including lower dependencies | 576 warnings before and after; normalized multiset has zero added/removed diagnostics | `ui-{baseline,after}-clippy.log` |
| CLI version/help | Captured output byte-identical | `baseline-{version,help}.txt`, `ui-{version,help}.txt` |
| Frozen inventory | PASS; 686 rows / 186,679 locked lines | `scripts/source-inventory.py` |
| Readability/content audit | Moved implementations unchanged; new root has only exports/module declarations | Git diff and `ui-move-files.json` |

## Rejected or deferred profile changes

- `profile.dev.debug=0` was measured, not kept: initial build 5m03s, runtime/tools touches 1m13s/1m17s, at load approximately 87–96. Different host load prevents a causal comparison; there is no evidence to justify removing existing line-table diagnostics.
- No external package/version upgrades, linker changes, test-layout changes or product behavior changes are included in this lane.
