# models-dev-reasoning-options.ts — retired

No `packages/ai/scripts/models-dev-reasoning-options.ts` exists at frozen `4f9e6f8a4`. The old native module had no production consumers after the [whole generator re-port](model-catalog-generator.md); its public export, module and obsolete fixtures/oracle are removed. Shared Model thinking-level types and current generator metadata remain unchanged.

Retired tests (old-pin optional reasoning-control inference, not frozen behavior):

- `mixed_options_emit_source_order_and_explicit_nulls`
- `absent_or_unusable_efforts_return_none`
- `none_only_and_max_only_keep_distinct_off_values`
- `option_union_preserves_wire_values_and_optional_budget_bounds`
- `pinned_source_matches_every_effort_subset_order_and_option_combination`

The former differential now reproduces ERR_MODULE_NOT_FOUND against the common pin; no shim/fallback or old source path is retained. Former532f7d12 proof/type exclusions are historical, not common-pin evidence. Frozen source-presence, consumer audit, removed-source registry and workspace/inventory gates govern this deletion.
