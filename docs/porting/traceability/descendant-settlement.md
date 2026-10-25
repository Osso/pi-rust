# Descendant settlement — frozen 4f9e6f8a4

Source `extensions/agents-core/src/descendant-settlement.ts`; native `agents_core/descendant_settlement.rs`, called by dispatch runtime.

| Symbol / branch | Proof |
| --- | --- |
| `waitForActiveDescendants`: refresh before checking, empty completion, abort only when parent is not cancelling | `agents_core_runtime::cancelling_parent_waits_for_descendants_even_after_abort`; initial native RED returned early with an aborted cancelling parent |
| `waitForDescendantStateChange`: subscribe, 250ms poll, abort wake, unsubscribe | Same test wakes on a local terminal update; persistence test wakes without a local event; scoped subscription/timer futures drop on completion |
| `refreshPersistedDescendantSnapshots`: no persistence/no state, descendant-only projection, publish missing or newer revision | `descendant_settlement_reconciles_persisted_newer_revision_without_local_event`: separate SQLite writer, real 250ms reconciliation, revision and terminal lifecycle asserted; original native RED timed out |
| `listDescendantSnapshots`: parent index, stack traversal with sibling insertion order | Native parent index traverses persisted snapshots using the same stack ordering; deeper/missing-child matrix pending |

Proof: `/tmp/agents-core-descendants-red.log`, `/tmp/agents-core-descendants-green2.log` (21 agents-core runtime tests passed). Exact error propagation uses Result; database failure is not converted into absence. Frozen in-memory abort differential is in progress. Runtime fault/scheduling oddities are a gap under the project parity boundary.
