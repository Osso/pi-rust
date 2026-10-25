# Coding-agent core split, round 2

Contract: [crate layers](../specs/coding-agent-crate-layers.md). Technique and prior checkpoint: [round 1](crate-split.md), [round-1 audit](crate-split-audit.md).

## Active goal

Preserve behavior, public module paths, profiles/lints and external dependency versions. Touching a file in each layer must give `cargo check -j4 -p pi-coding-agent` in at most 30 seconds at comparable recorded load. Debug builds are measured, not subject to that check-only limit.

## Layers

Facade → interactive → bundled extensions → core session runtime → tools/native extension contracts → storage and packages → data → base. Direct lower-layer edges preserve compatibility exports. The storage and packages branches can compile independently.

- Data: auth, model registry/cache/resolver, settings, permission policy/presets, CLI args and small shared leaves.
- Storage: control DB, multi-agent store, session manager/transcript/archive, process identity and lifecycle storage. Existing cycles stay together.
- Packages: package manager, semver and source metadata.
- Tools: built-in tools, native extension contracts/runner, detached Bash, resources, shared theme/UI leaves and Supervisor adapters. Existing tool/extension/detached cycles stay together.
- Core: agent session, runtime/services, SDK and provider recovery; footer/actions remain here.

## Boundary edits

The unchanged `CycleDirection`, `ModelCycleResult` and thinking-level serializer move down to a tools-owned `core::agent_session` compatibility module. Core re-exports them. The unchanged `AgentSessionError` → `HandlerError` conversion moves beside `AgentSessionError`; other callback conversions move with their owning types. No trait/callback inversion is introduced.

New roots preserve `crate::core::...` trees and all old paths through re-exports. Test support symlinks target the facade tests. The retention-contract include points at storage's physical file. Compiler-proven private cross-layer helpers are exposed; the final audit will enumerate them.

## Proof ledger

Artifacts: `/tmp/pi-crate-split-2-proof/` on the development host. Baseline revision `726033aa`: workspace/all-target/all-feature clippy exits 0 with **1,676 warnings, zero errors**. Before-touch check/debug-build logs and uptime records are `before-timings.json` and `before-*.log`. CLI stdout/stderr bytes are saved as `before-{version,help}.{stdout,stderr}`.

Checkpoint proof and final timing table remain pending. No completion claim is made by this initial wiring checkpoint.

## Second cut

The first tools touch took **30.22s** (wall 30.31s) at uptime load 30.14/29.78/30.98, so it failed the 30s goal. Main approved a further tool-support cut. `pi-coding-agent-tool-support` owns shared theme/UI, edit/diff/truncation/output helpers and the unchanged `HandlerError` implementation, re-exported under its former extension-types path. Its only widened function is compiler-proven cross-crate `observe_tool_producer`. Storage and support are sibling branches; support depends on data/packages, never session runtime or tools.

## Model leaf cut

The second-cut model-resolver touch rebuilt unrelated storage/package branches and took **31.76s** at load 21.16. Main approved `pi-coding-agent-models`: auth storage, config values, model registry/cache/resolver, defaults and provider display names. Models depends on data for CLI validation, but storage/packages/tool-support do not depend on models. Core and facade retain old auth/model paths through tools' re-exports. The newly introduced data crate no longer re-exports these leaves, to avoid restoring the dependency.

## Settings cut

The first comparable-load window (three checks per layer, start/end 1-minute load ≤30) passed every layer except data: settings-manager touches took 32.02/30.24/31.64s, median **31.64s**. Main approved `pi-coding-agent-settings` (settings manager, HTTP dispatcher and compaction threshold) above data and below packages/support/tools, but independent of storage. `host_relay_resident`, the sole storage settings consumer, moves unchanged into tools. No lower storage consumer relies on it. This removes storage rebuilds from settings touches and shortens data's broad rebuild path.
