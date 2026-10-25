# Coding-agent core split, round 2

Contract: [crate layers](../specs/coding-agent-crate-layers.md). Technique: [round 1](crate-split.md). [Complete round-2 content audit](crate-split-2-audit.md).

## Goal and graph

Preserve behavior and every old public core/facade module path. Touching a file in each coding-agent layer must give `cargo check -j4 -p pi-coding-agent` in at most 30 seconds at comparable load. Debug builds are measured but are not subject to that check-only limit. Profiles/lints and disabled incremental compilation stay unchanged.

The upper chain remains facade → interactive → bundled extensions → core → tools. Core now owns only session runtime/services, SDK, provider recovery, extension actions and footer. Tools keeps the existing tool/extension/detached-runtime cycles together, but not agent session or control storage.

| Lower layer | Rust source lines | Lower coding-agent dependencies | Ownership |
|---|---:|---|---|
| data | 3,851 | base | CLI args, permission policy/presets, event bus and small shared leaves |
| settings | 2,364 | data/base | settings persistence, HTTP dispatcher, compaction threshold |
| models | 4,214 | data/base | auth, config values, model registry/cache/resolver |
| storage | 26,122 | data/base | control SQLite, sessions/transcripts/archive, lifecycle/process storage |
| packages | 5,068 | settings/data/base | package manager, semver, source metadata |
| tool-support | 5,647 | packages/data/base | tool algorithms, truncation/output, theme/UI leaves, HandlerError |
| tools | 30,037 | storage/tool-support/models/packages/data/base | built-in tools, native extension contracts/runner, resources, Supervisor and host-relay resident |
| core | 12,608 | tools/base | session runtime and compatibility exports |

Line counts include Rust unit-test code physically in `src/`, not test-support symlinks. Storage does not depend on settings, models, packages or tool-support. Models does not force storage/package rebuilds. Storage and support can compile independently.

## Boundary technique

Whole module trees move with `git mv` in pure-move commits. New roots reproduce their old `crate::core::...`, CLI, Supervisor and mode trees. Re-exports preserve downstream extension/interactive/facade imports and all facade integration targets. Newly introduced internal data does not re-export models/settings, which would recreate the removed dependency.

The unchanged `CycleDirection`, `ModelCycleResult` and serializer move down to tools. The unchanged `HandlerError` implementation moves down to tool-support. Error conversions move beside their owning input/output types to satisfy Rust's orphan rule. `host_relay_resident`, the only storage settings consumer, moves up to tools. No trait/callback inversion is introduced. Only compiler-proven cross-layer helpers become public; the [audit](crate-split-2-audit.md) enumerates every content edit.

## Measured cuts

The first tools touch took 30.22s (wall 30.31s), so a tool-support cut followed. The next model-resolver touch took 31.76s because unrelated storage/package branches rebuilt, so models became a separate sibling. A later three-run comparable-load window passed every layer except data: settings-manager touches took 32.02/30.24/31.64s, median **31.64s**. Settings then became a separate layer, independent of storage.

All exploratory timings, including rejected high-load samples, remain in `/tmp/pi-crate-split-2-proof/`. Rejections are never silently removed. Final acceptance uses the main-requested median of three touch checks per layer, with both start/end one-minute load at most 30; uptime is captured per command. A debug build is measured once per layer in the same window.

## Proof ledger

Code snapshot: `a87f64cc`, merged parent `4c28cdf4`. Later documentation-only commits do not invalidate these source proofs. Artifacts: `/tmp/pi-crate-split-2-proof/` on the development host.

| Proof | Result | Artifact |
|---|---|---|
| `cargo check -j4 --workspace --all-targets --keep-going --message-format=json` | exit 0; 0 errors / 0 warnings | `settings-final-check.{stdout,stderr}` |
| `cargo clippy -j4 --workspace --all-targets --all-features --keep-going --message-format=json` | exit 0; 1,669 warnings / 0 errors; baseline 1,676 at `726033aa` | `settings-final-clippy.{stdout,stderr}` |
| `cargo test -j4 -p pi-coding-agent --no-run` | exit 0; all 706 executable targets build | `settings-final-test-no-run.{stdout,stderr}` |
| Frozen inventory | PASS; 686 rows / 186,679 locked TS lines | `settings-inventory.log` |
| `differential_print_mode` / `output_accumulator` | 23/23 and 10/10 GREEN | `settings-final-print-output.{stdout,stderr}` |
| `pi-ai` `differential_openai_codex_responses` | 38/38 GREEN | `settings-final-codex.{stdout,stderr}` |
| `agent_session`, serial and default parallel | 72/72 GREEN in each run; no exceptions | `settings-final-session-{serial,parallel}.{stdout,stderr}` |
| `pi --version` / `--help` | exit 0; stdout byte-identical (7 / 10,166 bytes), stderr byte-identical and empty | `before-*` / `settings-final-*` CLI bytes |
| All-layer touch checks | all 12 accepted medians ≤30s; maximum 28.83s | [tracked timing records](crate-split-2-timings.json) |
| Formatting and content/readability audit | `cargo fmt --all --check` passes; moved bodies unchanged; no new runtime complexity or suppressions | [content audit](crate-split-2-audit.md) |

The seven fewer clippy warning signatures are inherited parent changes, not lost checks: four `collapsible_if` warnings in model registry, two `useless_conversion` warnings in bundled bwrap and one in the doom-overlay example. Path-normalized comparison adds **zero** warning signatures. See `clippy-signature-audit.json`.

The former round-1 session exception is retired: parent `5315a68a` fixes the stale fixtures/isolation. This checkpoint requires all 72 agent-session tests GREEN, serial and parallel; it does not accept 68/3.

## Invariants

External lockfile records/versions/features, workspace profiles/lints and `.cargo/config.toml` are unchanged. Seven new exact `=0.0.0` path packages divide compilation units. No runtime fallback, new external dependency, push or deployment is part of this split. Rust documentation paths change mechanically; locked TS counts/status values are changed only by merged parent work.

## Accepted timing checkpoint

Main accepted the median-of-three criterion on 2026-10-24. All **12 medians pass**. The eligible tool-support outlier **35.34s** remains in the three samples and in the median calculation; its other samples are 25.21/26.70s. This is not a hard every-run 30s guarantee.

Commands: `cargo check -j4 -p pi-coding-agent` and `cargo build -j4 -p pi-coding-agent --bin pi`. Times below are wall seconds. [Raw baseline/final records, exact probe paths, start/end uptime and all loads](crate-split-2-timings.json) include both load-rejected final samples.

| Layer | Before check | Final check samples | Final median | Before debug build | Final debug build |
|---|---:|---|---:|---:|---:|
| data | 31.64 | 16.83 / 16.47 / 17.91 | **16.83** | 59.92 | 29.96 |
| settings | 31.64 | 19.19 / 18.46 / 15.58 | **18.46** | 59.92 | 38.63 |
| models | 34.50 | 29.70 / 28.83 / 25.26 | **28.83** | 51.55 | 39.07 |
| storage | 28.10 | 26.71 / 25.64 / 25.35 | **25.64** | 44.31 | 44.93 |
| packages | 33.00 | 24.66 / 23.31 / 24.43 | **24.43** | 62.17 | 49.11 |
| support | 22.35 | 35.34 / 25.21 / 26.70 | **26.70** | 36.55 | 76.40 |
| tools | 41.70 | 17.81 / 17.85 / 18.64 | **17.85** | 73.20 | 31.55 |
| runtime | 48.18 | 13.90 / 12.83 / 13.89 | **13.89** | 53.37 | 21.89 |
| extensions | 15.40 | 11.31 / 11.81 / 11.05 | **11.31** | 24.83 | 22.51 |
| interactive | 10.11 | 7.39 / 6.08 / 7.10 | **7.10** | 15.17 | 11.96 |
| base | — | 22.35 / 21.51 / 22.97 | **22.35** | — | 36.81 |
| facade | — | 7.94 / 3.35 / 3.12 | **3.35** | — | 15.63 |

Before values use `726033aa`, except support before its cut at `2dd11f78` and data/settings before their final cut at `8c04347c`. Data/settings share the former data-crate settings-manager probe; the final data probe is event_bus, while settings keeps settings_manager. Base/facade are additional acceptance probes without a new before sample. Parent lane changes landed between stages, so these are recorded rebuild observations, not a controlled same-code performance experiment.

Debug builds are observations, not acceptance gates; the tool-support debug observation is 76.40s. Its start/end loads and uptime are retained with the other samples.
