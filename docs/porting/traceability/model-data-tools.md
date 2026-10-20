# model-data.ts maintenance parity

Source: pinned `packages/ai/scripts/model-data.ts` (280 lines). Rust: `crates/pi-ai/src/model_data_tools.rs`. Contract: [spec](../../specs/model-data-tools.md). This slice ports only this module and its [checker](check-model-data.md). Shared dates use the [V8 parser contract](../../specs/date-parse.md) and retain [bounded differential proof](js-compat.md#dateparse-srcdaters).

## Current capability / proof matrix

| TS export / branches | Rust | Behavioral proof in `tests/model_data_tools.rs` |
|---|---|---|
| Schema/version constants, structure and manifest records | same constants; insertion-ordered UTF-16 records | `filesystem_structure_and_checker_success_match_pinned_modules`, manifest tests |
| `assertExactModelIds`: dedup, equal, missing, extra, UTF-16 sorting | `assert_exact_model_ids` | `exact_ids_and_manifest_hashes_match_ts_without_reordering_responses`, `typed_utf16_ids_structure_hash_and_manifest_match_complete_ts_bytes` |
| `modelDataStructureHash`: sorted nested records, JS numeric keys, Unicode, SHA-256 | `model_data_structure_hash` | same tests; exact pinned API responses |
| `createModelDataManifest`: schema 3, timestamp unchanged, sorted content hashes, Node UTF-8 boundary | `create_model_data_manifest` | same tests; CRLF, Unicode and lone-surrogate keys/content/timestamp |
| `readModelDataProviderIds`: exact import pattern, none, duplicates, sorted IDs, read error | `read_model_data_provider_ids` | `aggregator_shards_and_provider_structure_failures_match_ts`, filesystem success |
| `readModelDataStructure`: shard sets, provider JSON object/parse errors, API object, duplicate models, empty data | `read_model_data_structure`; crate-visible `read_provider_structure` reused by native generator post-swap validation | same tests; raw surrogate corpus; generator fault-36 RED proves fresh empty-provider read must precede manifest validation (integration GREEN pending) |
| `validateModelDataDirectory`: directory missing/not directory, provider sets, manifest object/parse/missing, schema, timestamp, structure stamp, hash set/file hash | `validate_model_data_directory`; timestamp passes original UTF-16 units to shared `date_parse_utf16` | `manifest_directory_and_json_failures_match_ts`, `manifest_date_validation_preserves_lone_utf16_units` |
| Model identity/provider/API/name/baseUrl/reasoning/modalities/limits/cost fields; nonobject; duplicate group; missing/extra IDs; wrong grouping | internal validation | `model_validation_branches_accumulate_in_source_order` |
| Source-ordered errors; first 30 plus remainder | internal validation | same test; complete error equality |
| `validateGeneratedModelData` original TS-package contract | `validate_generated_model_data` | filesystem success and failure fixtures |
| JS line terminators and record enumeration/serialization | import matcher, shared JS enumeration | `import_line_terminators_match_js`, `manifest_record_order_matches_js`, `numeric_provider_model_and_api_keys_keep_js_diagnostic_order` |
| Early provider IO throw vs accumulated JSON errors; ignored unrelated files | filesystem adapters | `filesystem_io_failures_and_ignored_non_catalog_entries_match_ts` |
| Caller Node flag plus orphan process-group teardown | existing oracle support | `caller_node_flag_and_parent_death_watchdog_both_work` |
| Raw JSON unpaired surrogate diagnostic | shared `parse_lossless` and lossless diagnostic storage | `json_model_identity_with_unpaired_surrogate_matches_ts`, GREEN |
| Raw surrogate model/API keys, distinct surrogate/U+FFFD/pairs, nested metadata strings | shared `JsString`/`JsValue`/`JsObject` throughout traversal | `raw_surrogate_keys_and_nested_values_match_full_ts_responses`, GREEN; exact whole encoded response bytes |
| No-argument native checker, source-location default | `native_model_data_structure`, `validate_model_data_directory`; explicit-root API `validate_native_model_data` retained | [checker proof](check-model-data.md): sandboxed source-root positive/error cases from unrelated CWDs; previous CWD inference corrected |

Coverage rows remain partial pending parent-owned final integration/verification; bounded source-root correction proof lives in [checker traceability](check-model-data.md). No unresolved surrogate implementation blocker. Existing catalog-state discrepancy is recorded in checker traceability, not silently repaired.

Filesystem errors now use the [shared Node filesystem prerequisite](node-fs-errors.md); the private adapter wraps its message in `JsString` without local errno/message mappings. Model-data source contracts and UTF-16 diagnostics are unchanged.

## API and representation boundary

`ModelDataStructure = IndexMap<JsString, IndexMap<JsString, JsString>>`. Manifest constructor accepts structure, `IndexMap<JsString, JsString>` exact file contents, and `&JsString` generation timestamp. `model_data_structure_json`, manifest `to_json`/`stringify`, and error `message()` expose lossless output. `ModelDataError(pub JsString)` retains exact code units; Display alone is explicitly lossy. Hashing/filesystem UTF-8 encoding uses Node-equivalent replacement at that boundary only.

Shared prerequisite `850b6e8` (canonical integration `e31b4c8`) is merged in `cf0e110`. Raw metadata uses shared `parse_lossless`, shared numeric-first enumeration, and shared lossless stringify; no local parser, sentinel, rescanning, or duplicated JSON encoding. Number boundary unchanged. No dependencies added. Runtime `providers/models_data.rs` remains unchanged.

Original TS-layout provider discovery/structure/generated-validator exports keep their literal source contract; they are not a native checker fallback. User selected “Check Rust catalog (Recommended)” on 2026-10-20; exact question and decision live in [checker traceability](check-model-data.md#authorized-layout-decision).

Invalid JS argument types cannot enter typed structure, constructor, paths or allowlist APIs. Untrusted JSON files remain dynamically validated, including malformed metadata. No TS source bugs corrected. V8 legacy-date seconds behavior is preserved and classified in shared date traceability.

## Oracle transport

Oracle loads real pinned modules through existing `tests/support/oracle.rs`: process groups, caller flags, parent watchdog, source-lock verification; no direct tsx bypass. `encodeResponse` serializes the complete response as an opaque string. `requestJson` transports a complete encoded request through serde without losing UTF-16 keys/values. Both preserve every field/key/code unit; no filtering, sorting, normalization or response subset comparisons.

## Proof ledger

Commands run in `/home/osso/.worktrees/pi-rust-model-data-tools`. Historical runs used `timeout 900`; resumed runs use direct Pyrun argv commands. Logs below live under `/tmp/`; resumed log files retain stdout and command exit results are recorded in `/tmp/model-data-tools-proof-ledger.json`.

| Revision / scope | Command | Result / artifact | Validity |
|---|---|---|---|
| `07971b0` + initial fixtures | `cargo test -p pi-ai --test model_data_tools -- --nocapture` | RED missing API/binary; `model-data-tools-red.log` | Historical RED |
| `816a8e5` | same target | 4 pass / 2 fail; `model-data-tools-green-1.log` | Superseded |
| `e9f2e4c` | same target, `checker_exit_stdout_stderr` filter | 1 pass; `model-data-tools-checker-green.log` | Historical argument-only checker |
| `a75fd1a` | `cargo test -p pi-js-compat --test date --test differential_date_parse -- --nocapture` | 8 unit + 4 differential pass; `model-data-tools-date-green-1.log` | Superseded date scope |
| `2379d79` | initial module target | 8 pass; `model-data-tools-green-2.log` | Superseded |
| `3e63f4f` | module target, `caller_node_flag` filter | 1 pass; `model-data-tools-watchdog-green.log` | Later expanded owner-lifetime proof |
| `b9ed3c9` | focused date targets above | 8 unit + 5 differential pass; `model-data-tools-date-green-3.log` | Unchanged date proof; no rerun |
| `b9ed3c9` + raw-surrogate fixture | module target, `json_model_identity_with_unpaired` filter | RED exact surrogate diagnostic; `model-data-tools-surrogate-red.log` | Historical RED resolved by `f139cff` |
| `c67b159` | full module target | 12 pass / 1 fail; `model-data-tools-expanded-proof.log` | Historical surrogate RED |
| `908c2ce` corpus (production unchanged from `fa543e5`) | module target, `raw_surrogate_keys` filter | RED collisions/API/nested values; `model-data-tools-surrogate-keys-red.log` | Historical RED resolved by `f139cff` |
| `f139cff` production/tests (run on `cf0e110` + consumer diff, then formatted) | `cargo test -p pi-ai --test model_data_tools -- --nocapture` | GREEN 14/14, 0 ignored; `model-data-tools-lossless-green-1.log` | Native checker subsequently replaced; consumer proof retained |
| `f139cff` + native fixture | same target, `native_checker_defaults` filter | RED usage exit 2 vs expected 0; `model-data-tools-native-red.log` | Historical native RED |
| `cbe93f4` production/tests (run on `f139cff` + native diff, then formatted) | full module target above | GREEN 15/15, 0 ignored; `model-data-tools-native-green-1.log` | Historical native CWD behavior superseded by source-root correction; unaffected API proof retained |
| `cbe93f4` + typed UTF-16 corpus and opaque request transport | same target, `typed_utf16_ids` filter | GREEN 1/1, 15 filtered; `model-data-tools-typed-utf16-proof.log` | Additive API proof; no production change |

Integration timestamp regression: a valid `Jan1 2020 (` + lone high surrogate + `)` manifest timestamp was rejected by the UTF-8 conversion before shared parsing. `/tmp/pi-rust-batch-a-proof/date-adapter-red.log` records exact pinned-oracle RED (1 failure); the caller now passes original UTF-16 units. `/tmp/pi-rust-batch-a-proof/date-adapter-green.log` records all 17 module tests passing under the Cargo sandbox after the V8-parser merge and caller fix, before formatting. This is development proof, not independent Batch A acceptance.

Before the source-root correction, evidence was 15 existing tests plus one newly targeted test, not a claimed single 16-test run. Source-root correction reran only 3 checker-filter tests (13 filtered), preserving exact complete-stream differential comparison; see [checker ledger](check-model-data.md#proof-ledger). The added transport branch does not change existing requests lacking `requestJson`. Formatter-only edits do not invalidate behavior. No broad/check/clippy/readability/final-integration/deployment gates run; parent owns those gates. No catalog refresh, live HTTP/config/DB or canonical edits.
