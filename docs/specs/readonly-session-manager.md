# Readonly session manager

Common-pin `core/readonly-session-manager.ts` returns a live session view with isolated object reads. It is the approved prerequisite for [footer overrides](footer-data-provider.md). [Mapping](../porting/traceability/readonly-session-manager.md).

## What it must do

- [x] Reads observe current underlying session state, not a constructor-time copy.
- [x] Entries, headers, branches, context entries and tree nodes are owned snapshots; edits to returned data cannot mutate session storage.
- [x] Expose all21 frozen accessors and explicit set/clear goal mutators without exposing the backing mutex or structural session mutators.
- [ ] Propagate original goal/storage Result errors without reclassification; the private invalid-database wrapper proof awaits its final run.

## How it works

- [Export and branch map](../porting/traceability/readonly-session-manager.md)

## Implementation inventory

- `crates/pi-coding-agent-storage/src/core/readonly_session_manager.rs`: private live session handle and owned read surface.
- Footer data/default/core/interactive consumers: construct and read this restricted view.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/readonly_session_manager.rs`
- `oracle/readonly-session-manager-oracle.mts` imports the unchanged frozen wrapper and real SessionManager.

## Known gaps (current cycle)

- [ ] Final128-descendant, exact error and consumer gate pending; row is not yet complete.

## Out of scope

- Custom structural manager inputs: native accepts a concrete SessionManager. Parent approved this typed-context exclusion2026-10-24; no built-in consumer passes a custom manager. See the gap row in traceability.
- JS returned-object identity/mutation-through-reference is unnecessary here: frozen already requires isolated snapshots.
