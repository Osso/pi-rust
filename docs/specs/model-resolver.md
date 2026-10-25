# Frozen model resolution

Port the selection rules of `packages/coding-agent/src/core/model-resolver.ts` at the [common pin](../porting/source-lock.json).

## What it must do

- [ ] Provider defaults match the frozen record exactly, including absence of old-only providers.
- [ ] Exact/canonical/fuzzy matching, bracket handling and thinking suffixes select the same model or emit the same native warning/error.
- [ ] Scope globs use frozen minimatch behavior, preserve order/deduplication and emit only type=warning, message and pattern diagnostics.
- [ ] CLI, initial and restored model choices use the frozen registry catalog/configured-auth/available semantics.

## How it works

- [Traceability and the read-only Rust structural test seam](../porting/traceability/model-resolver.md).

## Implementation inventory

`crates/pi-coding-agent-data/src/core/model_resolver.rs`: matching, scopes and model selection. ModelRegistry is the sole production implementation of its four-method read surface.

## Tests asserting this spec

`tests/model_resolver.rs` and `tests/differential_model_resolver/mod.rs` under pi-coding-agent, with `oracle/model-resolver-oracle.ts`. The oracle uses the actual pinned resolver and registry; one Rust control uses the unmodified full registry.

## Known gaps (current cycle)

- [ ] Close current frozen REDs and complete targeted proof before marking this module done.

## Out of scope

Network discovery and old runtime/snapshot/store APIs.
