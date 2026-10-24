# model-data.ts / checker — retired

Frozen `4f9e6f8a4` has no `packages/ai/scripts/model-data.ts` or `check-model-data.ts`; its scripts directory contains only the chat/image model generators and the waived test-image generator. Parent approved complete deletion2026-10-24.

Removed `crates/pi-ai/src/model_data_tools.rs`, `src/bin/check-model-data.rs`, the lib export, `tests/model_data_tools.rs`, and `oracle/model-data-oracle.ts`. No production consumer remained after generator cleanup. No schema3/grouped maintenance manifest, structure hash, directory checker, TS-layout reader, native checker fallback, CLI or validation helper remains. The test-only frozen-source/payload provenance `.manifest.json` is unchanged; it belongs to [literal catalog proofs](providers-models.md), not runtime refresh.

## Named product-test retirement

The following15tests asserted removed product contracts, not current-pin behavior:

- `manifest_date_validation_preserves_lone_utf16_units`
- `exact_ids_and_manifest_hashes_match_ts_without_reordering_responses`
- `filesystem_structure_and_checker_success_match_pinned_modules`
- `model_validation_branches_accumulate_in_source_order`
- `manifest_directory_and_json_failures_match_ts`
- `aggregator_shards_and_provider_structure_failures_match_ts`
- `checker_exit_stdout_stderr_match_actual_pinned_script_on_fixtures`
- `import_line_terminators_match_js`
- `manifest_record_order_matches_js`
- `json_model_identity_with_unpaired_surrogate_matches_ts`
- `filesystem_io_failures_and_ignored_non_catalog_entries_match_ts`
- `numeric_provider_model_and_api_keys_keep_js_diagnostic_order`
- `raw_surrogate_keys_and_nested_values_match_full_ts_responses`
- `native_checker_defaults_to_source_root_from_unrelated_working_directories`
- `typed_utf16_ids_structure_hash_and_manifest_match_complete_ts_bytes`

The2generic tests `oracle_flag_watchdog_owner` and `caller_node_flag_and_parent_death_watchdog_both_work` move to `tests/oracle_process_lifetime.rs` with unchanged assertions. Its minimal oracle only reports the VM caller flag and process PID; the locked existing `ai/src/types.ts` path verifies the pin without loading a deleted module. Existing watchdog RED reproduced ERR_MODULE_NOT_FOUND for the old source; standalone GREEN proves caller flags and orphan process-group teardown without retaining the removed APIs.

All pre-repin implementation, layout approval, checker discrepancy and test evidence in the former documents is historical, superseded by source absence. Shared date/JSON/filesystem authorities remain independently covered; no behavior is retained here as a fallback.
