# model-data.ts maintenance parity

Source: pinned `packages/ai/scripts/model-data.ts` (280 lines). Rust: `crates/pi-ai/src/model_data_tools.rs`. Contract: [spec](../../specs/model-data-tools.md). Only this module and its checker are in scope.

| TS export / branches | Rust | Behavioral proof in `tests/model_data_tools.rs` |
|---|---|---|
| `MODEL_DATA_SCHEMA_VERSION`, `MODEL_DATA_MANIFEST_FILE`, `ModelDataStructure`, `ModelDataManifest` | same constants; typed insertion-ordered records | `filesystem_structure_and_checker_success_match_pinned_modules`, manifest test |
| `assertExactModelIds`: dedup, equal, missing, extra, UTF-16 sorting | `assert_exact_model_ids` | `exact_ids_and_manifest_hashes_match_ts_without_reordering_responses` |
| `modelDataStructureHash`: sorted nested records, JS numeric keys, Unicode, SHA-256 | `model_data_structure_hash` | same test, real pinned API response |
| `createModelDataManifest`: schema 3, timestamp unchanged, sorted content hashes | `create_model_data_manifest` | same test; exact values, CRLF/Unicode input |
| `readModelDataProviderIds`: exact import pattern, none, duplicates, sorted IDs, read error | `read_model_data_provider_ids` | `aggregator_shards_and_provider_structure_failures_match_ts`, filesystem success |
| `readModelDataStructure`: shard sets, provider JSON object/parse errors, API object, duplicate models, empty data | `read_model_data_structure` | same tests |
| `validateModelDataDirectory`: directory missing/not directory, provider sets, manifest object/parse/missing, schema, timestamp, structure stamp, hash set/file hash | `validate_model_data_directory` | `manifest_directory_and_json_failures_match_ts` |
| model identity/provider/API/name/baseUrl/reasoning/modalities/limits/cost fields; nonobject; duplicate group; missing/extra IDs; wrong grouping | internal validation | `model_validation_branches_accumulate_in_source_order` |
| source-ordered errors; first 30 plus remainder | internal validation | same test, complete error string equality |
| `validateGeneratedModelData` | `validate_generated_model_data` | filesystem success; checker failure/success |

RED: focused test compile fails for missing maintenance export and checker binary (2026-10-20). Initial GREEN pending. No external service or real catalog mutation. Oracle uses existing `tests/support/oracle.rs` process groups, parent watchdog, source-lock verification; no raw tsx command. No response field filtering/sorting.

Typed-input exclusions: invalid JS argument types cannot enter typed structure, manifest constructor, paths or allowlist APIs. JSON file values, including invalid runtime model metadata, are still validated. No new dependencies. Shared `pi-js-compat` owns JSON parse/stringify/object enumeration/date semantics. Generated runtime `providers/models_data.rs` is unchanged and does not substitute for these workflows.

Discrepancies: none classified yet; no source bug intentionally corrected.
