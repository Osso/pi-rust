# model-data.ts maintenance parity

Source: pinned `packages/ai/scripts/model-data.ts` (280 lines). Rust: `crates/pi-ai/src/model_data_tools.rs`. Contract: [spec](../../specs/model-data-tools.md). Only this module and its checker are ported. Shared date parsing fixes were separately authorized as direct validator prerequisites; [date proof](js-compat.md#dateparse-srcdaters).

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
| JS line terminators and record enumeration/serialization | import matcher, shared JS object enumeration | `import_line_terminators_match_js`, `manifest_record_order_matches_js`, `numeric_provider_model_and_api_keys_keep_js_diagnostic_order` |
| Early provider IO throw vs accumulated JSON errors; ignored unrelated files | filesystem adapters | `filesystem_io_failures_and_ignored_non_catalog_entries_match_ts` |
| Caller Node flag plus orphan process-group teardown | existing oracle support with caller flags appended before watchdog | `caller_node_flag_and_parent_death_watchdog_both_work` |
| Raw JSON unpaired surrogate diagnostic | **blocked** by shared serde string representation | `json_model_identity_with_unpaired_surrogate_matches_ts` remains RED |
| Raw surrogate model/API keys, distinct surrogate/U+FFFD keys, nested metadata strings | **blocked** by shared serde string representation | `raw_surrogate_keys_and_nested_values_match_full_ts_responses` remains RED; complete response encoded once for lossless transport |

RED: focused test compile fails for missing maintenance export and checker binary (2026-10-20). At `2379d79`, all 8 initial filesystem/API/checker differential tests pass. Subsequent expanded corpus retains a real raw-surrogate failure; neither module is claimed complete. No external service or real catalog mutation. Oracle uses existing `tests/support/oracle.rs` process groups, parent watchdog, source-lock verification; no raw tsx command. No response field filtering/sorting.

Typed-input exclusions: invalid JS argument types cannot enter typed structure, manifest constructor, paths or allowlist APIs. JSON file values, including invalid runtime model metadata, are still validated. No new dependencies. Shared `pi-js-compat` owns JSON parse/stringify/object enumeration/date semantics. Generated runtime `providers/models_data.rs` is unchanged and does not substitute for these workflows.

## Blocked representable case

`json_model_identity_with_unpaired_surrogate_matches_ts`: raw provider JSON contains `"id":"\\ud800"`. TS reports `has id "\\ud800", expected "model-a"`; Rust reports `has id "�", expected "model-a"`. `pi-js-compat/src/js_json.rs::parse_string` uses `String::from_utf16_lossy`, so `serde_json::Value::String` has already lost the code unit before validation. This is not an invalid-JS-input-type exclusion. A lossless shared string/value representation is required; no local diagnostic patch or gap approval. Shared prerequisite implementation is independently authorized; consumer production changes wait for its integration. Regressions and RED logs retained.

Expanded corpus proves that lone high/low surrogate keys and actual U+FFFD keys collide under the old parser, changing model cardinality and diagnostic order. Lone-surrogate API keys and nested object/array metadata also lose identity. Optional oracle `encodeResponse` serializes the entire response as an opaque JSON string: every field/key/code unit remains intact; no normalization, filtering or sorting.

No TS source bugs corrected. V8 legacy-date seconds quirk is preserved and classified in shared date traceability.

## Proof ledger

All commands run in `/home/osso/.worktrees/pi-rust-model-data-tools`, with `timeout 900`. Logs are local artifacts under `/tmp/`.

| Revision / scope | Command suffix | Result / artifact | Validity |
|---|---|---|---|
| `07971b0` + initial fixture tests | `cargo test -p pi-ai --test model_data_tools -- --nocapture` | RED missing API/binary; `model-data-tools-red.log` | Historical RED |
| `816a8e5` | same focused target | 4 pass / 2 fail; `model-data-tools-green-1.log` | Superseded by fixes |
| `e9f2e4c` | same target, `checker_exit_stdout_stderr` filter | 1 pass; `model-data-tools-checker-green.log` | Checker branch proof, later expanded |
| `a75fd1a` | `cargo test -p pi-js-compat --test date --test differential_date_parse -- --nocapture` | 8 unit + 4 differential pass; `model-data-tools-date-green-1.log` | Expanded legacy grammar invalidated this old scope |
| `2379d79` | initial focused model-data target | 8 pass; `model-data-tools-green-2.log` | Later shared date/test changes require expanded run |
| `3e63f4f` | model-data target, `caller_node_flag` filter | 1 lifetime/flag proof pass; `model-data-tools-watchdog-green.log` | Expanded to inspect both group leader and oracle |
| `b9ed3c9` | focused date targets above | 8 unit + 5 differential pass; `model-data-tools-date-green-3.log` | Current date source proof; no later date changes |
| `b9ed3c9` + raw-surrogate fixture regression | model-data target, `json_model_identity_with_unpaired` filter | RED exact escaped-surrogate diagnostic; `model-data-tools-surrogate-red.log` | Current unresolved failure |
| `c67b159` | full focused `cargo test -p pi-ai --test model_data_tools -- --nocapture` | 12 pass / 1 fail, no ignores; `model-data-tools-expanded-proof.log` | Model-data production source unchanged; later transport/blocked fixtures require expanded final run after prerequisite |
| `fa543e5` + full-response transport/key fixtures | same target, `raw_surrogate_keys` filter | RED key collisions/order, API keys, nested strings; `model-data-tools-surrogate-keys-red.log` | Additional blocked corpus; no consumer production change |

Expanded passing proof includes both caller Node flag and death of oracle plus process-group leader after owner exits without Drop, native checker success/failure output printed from real subprocesses, ordered numeric provider/model/API keys, ignored unrelated files, provider-directory IO throws, and the newly repaired legacy timestamps through the actual validator.

No broad/check/clippy/readability/final-integration/deploy gates run. Shared date changes invalidate relevant integration proof after merge, not the unchanged canonical checkout's earlier proof.
