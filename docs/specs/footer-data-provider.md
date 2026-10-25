# Footer data provider

The provider supplies branch, executable, extension status and displayed-session data from common-pin `core/footer-data-provider.ts`. [Branch map](../porting/traceability/footer-data-provider.md).

## What it must do

- [x] Resolve regular/worktree/reftable metadata and executable names with exact Node path/text semantics, including raw UTF-16 and metadata-kind walk/stop rules.
- [x] Use frozen git arguments, stdout decoding/trim and default1MiB limits; synchronous stderr is ignored, asynchronous stderr is bounded, and overflow uses SIGTERM after captured pipes close.
- [x] Preserve lazy caching, no-change suppression,500ms debounce, pending refresh coalescing and in-flight disposal/CWD behavior.
- [ ] Use live branch-listener iteration and preserve original callback errors. Synchronous Result proofs pass; the added scheduled-refresh process-authority proof awaits its final run.
- [x] Match HEAD/reftable setup failure,5s retry and disposal. Only WSL Windows-mounted HEAD files are polled; compare mtimeMs/ctimeMs/size with exact environment truthiness and case-insensitive mount prefix.
- [x] Keep ordered text/pure status callbacks evaluated on every read with the current override; omitted text does not remove the carrier.
- [x] Expose isolated live session reads through the [readonly session view](readonly-session-manager.md), retaining only its explicit goal mutators.
- [x] Preserve binary64 provider count, including fractional/negative/nonfinite/negative-zero values.

## How it works

- [Complete source/native branch map and accepted boundaries](../porting/traceability/footer-data-provider.md)
- [Readonly session map](../porting/traceability/readonly-session-manager.md)
- [Shared capture contract](child-process-capture.md)

## Implementation inventory

- `pi-coding-agent-tools/src/core/footer_data_provider.rs`: provider, System git/watch backends and hidden footer-only runtime-failure injection.
- `pi-coding-agent-storage/src/core/readonly_session_manager.rs`: isolated live session view.
- Core/default/custom footer and interactive mode adapters: use readonly snapshots, f64 counts and callback Results.
- `pi-coding-agent-base/src/utils/child_process.rs`: explicit overflow signal and pipe destruction; existing callers retain SIGKILL.

## Tests asserting this spec

- Tools lib `core::footer_data_provider::{tests,tests_refresh,tests_runtime_watch}`: actual-source real watcher/poll/refresh/error/retry scenarios.
- `differential_footer_provider.rs`, `footer_provider_numbers.rs`, `footer_git_process.rs`, `footer_branch_uncaught.rs`, `child_process_kill_signal.rs`: private source/native metadata, listener, number, process and uncaught boundaries.
- `readonly_session_manager.rs`, `footer_data_provider.rs`, `raw_footer_statuses.rs`, `footer_callable_status.rs`, `differential_footer.rs`, `differential_default_footer.rs`, `default_footer_extension.rs`: read isolation, raw text and current render/extension consumers.

## Known gaps (current cycle)

- [ ] Current final consumer/static/inventory confirmation and coverage checkpoint remain pending. Individual behavioral proofs are in the trace; no whole-row acceptance yet.

## Out of scope

- Impure status callbacks mutating the map, arbitrary structural session-manager implementations and TS-only Pick enforcement use the explicitly approved boundaries in traceability.
- The frozen transient stale-CWD branch flash is preserved, not repaired by a delta.
- Live host repositories/state, Windows-only runtime branches, new dependencies and unapproved semantic deviations.
