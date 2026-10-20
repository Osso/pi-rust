# generate-image-models traceability

Source: pinned `packages/ai/scripts/generate-image-models.ts` (165 lines), original runtime source lock. Rust: `crates/pi-ai/src/image_catalog_generator.rs` and `src/bin/generate-image-models.rs`. Contract: [feature spec](../../specs/image-catalog-generator.md).

## Symbols and branches

| Source | Rust | Behavioral proof / current status |
|---|---|---|
| exported `parseOpenRouterImageModels` | `parse_openrouter_image_models(&JsValue, bool)` | `parser_matches_actual_pinned_export_for_raw_json`: actual pinned export, 18 raw payloads × both strict modes; GREEN |
| payload missing/nonarray/empty; strict errors | same | Null/array/object root; missing/wrong/empty data; strict exact errors |
| architecture optional/nullish; modality filter/dedup; image required/default text | `modalities` | Unsupported/duplicate modalities, no-image, first skipped ID then usable duplicate; approved stable wrong-type rejection for scalar/nonstring modalities |
| ID/name preservation, first usable ID wins | parser raw `JsValue` records | Ordinary duplicate ID; lone surrogate vs replacement identity; missing-name omission; usable IDs required strings under explicit approval |
| four price expressions `parseFloat(value || "0") * 1e6` | `price`, shared `parse_float`, `is_truthy`, `to_js_string` | All four string fields, trailing text, whitespace, NaN, Infinity, signed zero, string overflow; wrong numeric/object/array/bool/null prices explicitly rejected with raw source witnesses |
| `readStrictOption` | `read_strict_option` | Empty/repeated strict accepted; unknown args exact error before fetch/write; native binary subprocess |
| `fetchOpenRouterImageModels` | `fetch_openrouter_image_models`, runtime `Fetch` | Controlled request asserts GET/exact URL; response status/body JSON/body rejection/fetch rejection, strict/non-strict; logs and error ordering; 23 controlled workflow cases GREEN. Actual transport failure RED (see below) |
| `generateImageModelsFile` | `generate_image_models_file` | Source-generated artifact imported through real tsx; every model value and key order compared through lossless stringify; 11 raw-ID/name cases × strict/non-strict GREEN: valid case differential, wrong types explicitly rejected with actual-source witnesses; 8 long malformed-ID cases explicitly rejected |
| nullish defaults, source skip/dedup boundary; valid long catalog | same parser/generator | `nullish_defaults_and_long_valid_string_catalog_keep_source_contract`: 4 pinned subprocess cases (nullish containers/modalities, missing name/prices, empty and nonfinite string prices, unread duplicate fields, 128 shuffled valid string IDs), exact artifact/streams/events GREEN |
| source `.sort(id.localeCompare)` then Object.fromEntries/Object.entries | shared UTF-16 collation, shared JSON key writer | Numeric key order and raw surrogate artifact bytes covered; shared ICU4C UTF-16 collation GREEN against 256 Node pairs |
| `main`: options → fetch → generate → write → log, catch/exit1 | `run`, maintenance binary | Temporary FS; source subprocess exact bytes; unknown arguments, missing/empty/noimage lists, HTTP/JSON/fetch errors, write failure; 23-case workflow GREEN including exact ENOENT/EISDIR names/messages/metadata |
| actual runtime HTTP body → JSON → native file | existing HTTP/read-body infrastructure | `generator_reads_real_chunked_http_and_writes_lossless_native_file`: bounded loopback HTTP, chunked Unicode bytes, surrogate keys/values and non-rounded finite price; GREEN |

## Representation and approved adaptations

The generator accepts/returns shared `JsValue`, with `JsString` code-unit identity. It does not expose the runtime `ImagesModel` facade: that type cannot represent lone-surrogate names/IDs. Parsed costs retain f64 NaN/Infinity/-0; only the shared JSON writer applies JSON.stringify's non-finite-to-null rule. Native artifact layout and JS-frame omission are explicitly approved in the [spec](../../specs/image-catalog-generator.md); no other workflow change is implied.

Sorting happens before native object construction, whose shared writer applies JavaScript integer-key enumeration. Source TypeScript output text is not compared to native JSON bytes; its imported `IMAGE_MODELS` model values and order are compared exactly, including absent properties and raw UTF-16 strings. Checked-in baseline `data/image-models.json` remains untouched.

## Source discrepancies (classified, faithfully preserved)

- **Object-literal prototype setters:** source `Object.fromEntries` retains ID `__proto__`, but emitted quoted TypeScript object-literal keys become a prototype setter rather than an own model key. Actual source subprocess reports Fetched1 and success; importing its artifact gives `JSON.stringify(IMAGE_MODELS) === '{"openrouter":{}}'`. Native generation therefore omits this own catalog key. Object-valued raw `name`/`id` JSON expressions similarly lose nested `__proto__` own keys when evaluated as TypeScript literals. These are now outside valid record types under the exact wrong-field decision in the [spec](../../specs/image-catalog-generator.md), not a weakening of valid-string prototype preservation. `generated_artifact_matches_source_prototype_named_model` still compares the valid case against actual generated source, retains the nested source catalog witness, and explicitly asserts native name rejection. RED and GREEN: `/tmp/pi-rust-image-generator-proof/prototype-{red,green}.log`.
- Raw malformed records can throw despite source TypeScript declarations. The 2026-10-20 **“Reject wrong field types (Recommended)”** decision explicitly replaces only wrong consumed field types with meaningful native TypeErrors. Parser/workflow fixtures preserve full raw source success/error witnesses and assert the approved rejection. Missing defaults, nullish architecture/pricing/modalities, correct string prices and valid lone-surrogate IDs remain differential contracts. This is not the general invalid-JavaScript-typed-input exclusion.
- The separate 2026-10-20 **“Allow native causes (Recommended)”** decision permits native HTTP cause diagnostics only; top-level name/message, event order and exit stay exact. See [spec decisions](../../specs/image-catalog-generator.md#native-cause-approval-2026-10-20).

## Exact user decisions (2026-10-20)

Question: “For catalog responses with wrong-typed fields such as `id: null`, should Rust reject invalid records or reproduce V8’s accidental sort/exception behavior exactly? Exact matching now requires porting V8’s sorting algorithm; valid string IDs are unaffected.” Answer: **“Reject wrong field types (Recommended)”**.

Question: “For native HTTP failures, may Rust preserve the top-level error name/message but use native cause diagnostics instead of reproducing Node’s nested errno/address/cause objects? Failure ordering and exit behavior would still match.” Answer: **“Allow native causes (Recommended)”**.

Consumed-field validation stays at source read points: skipped IDs are not validated, missing-field defaults remain intact. These two approvals do not alter valid-string sorting, top-level transport errors, console order, or exit status.

## Oracle isolation and evidence

`oracle/image-catalog-generator-oracle.ts` imports the actual pinned parser. Workflow tests copy the script's exact bytes to a new temporary package and check byte equality before executing its unchanged main through the pinned tsx/config. Its relative output remains inside that package. Fetch is replaced by a controlled `Response`; console observation records events while forwarding unchanged calls, so raw stdout/stderr and exit remain intact. Generated TypeScript is actually imported, not regex-parsed or reimplemented. Outer oracle process uses existing `TsOracle` source-HEAD verification, watchdog, timeout and process-group cleanup; inner subprocess has a 20-second timeout.

Raw per-case results and native stderr: `/tmp/pi-rust-image-generator-proof/{workflow,raw}-*-source.json`, `*-native-stderr.txt`. Source diagnostics retain all JS frames; comparisons remove only approved JS frames and substitute the two approved artifact paths. Streams and event order are otherwise exact. Revision/command evidence belongs in that directory's ledger.

## Current proof level

Partial: targeted validation suite GREEN (8 tests), including parser18 × both modes, raw11 × both modes, 8 malformed-sort-boundary source witnesses and valid/nested-invalid prototype cases. Shared filesystem/collation prerequisites are integrated; unchanged collation proof was not rerun. Wrong-type approval and exact native error contract are in the spec. One enabled behavioral RED remains: `native_closed_loopback_failure_matches_pinned_generator_error_header` (native message-only error versus source TypeError plus ECONNREFUSED cause metadata). Shared helper integration is pending; native causes are permitted only under the separate explicit decision. Full raw source/native transport diagnostics are retained as `closed-loopback-source.json` and `closed-loopback-native-stderr.txt` in the proof directory. No waiver or ignored test is used. Parent owns integration gates. This slice performs only focused development RED/GREEN and formatting; no live refresh or broad/final acceptance is claimed.
