# generate-image-models traceability

Source: pinned `packages/ai/scripts/generate-image-models.ts` (165 lines), original runtime source lock. Rust: `crates/pi-ai/src/image_catalog_generator.rs` and `src/bin/generate-image-models.rs`. Contract: [feature spec](../../specs/image-catalog-generator.md).

## Symbols and branches

| Source | Rust | Behavioral proof / current status |
|---|---|---|
| exported `parseOpenRouterImageModels` | `parse_openrouter_image_models(&JsValue, bool)` | `parser_matches_actual_pinned_export_for_raw_json`: actual pinned export, 18 raw payloads × both strict modes; GREEN |
| payload missing/nonarray/empty; strict errors | same | Null/array/object root; missing/wrong/empty data; strict exact errors |
| architecture optional/nullish; modality filter/dedup; image required/default text | `modalities` | Unsupported/duplicate modalities, no-image, first skipped ID then usable duplicate; malformed scalar `.filter` TypeError |
| ID/name preservation, first usable ID wins | parser raw `JsValue` records | Ordinary duplicate ID; lone surrogate vs replacement identity; absent fields; nonstring raw IDs retained |
| four price expressions `parseFloat(value || "0") * 1e6` | `price`, shared `parse_float`, `is_truthy`, `to_js_string` | All four fields, trailing text, whitespace, NaN, Infinity, signed zero, numeric overflow, object/array/bool coercion and own-toString TypeError |
| `readStrictOption` | `read_strict_option` | Empty/repeated strict accepted; unknown args exact error before fetch/write; native binary subprocess |
| `fetchOpenRouterImageModels` | `fetch_openrouter_image_models`, runtime `Fetch` | Controlled request asserts GET/exact URL; response status/body JSON/body rejection/fetch rejection, strict/non-strict; logs and error ordering; 23 controlled workflow cases GREEN. Actual transport failure RED (see below) |
| `generateImageModelsFile` | `generate_image_models_file` | Source-generated artifact imported through real tsx; every model value and key order compared through lossless stringify; 11 raw-ID/name cases GREEN; malformed-ID sorting remains RED |
| source `.sort(id.localeCompare)` then Object.fromEntries/Object.entries | shared UTF-16 collation, shared JSON key writer | Numeric key order and raw surrogate artifact bytes covered; shared ICU4C UTF-16 collation GREEN against 256 Node pairs |
| `main`: options → fetch → generate → write → log, catch/exit1 | `run`, maintenance binary | Temporary FS; source subprocess exact bytes; unknown arguments, missing/empty/noimage lists, HTTP/JSON/fetch errors, write failure; 23-case workflow GREEN including exact ENOENT/EISDIR names/messages/metadata |
| actual runtime HTTP body → JSON → native file | existing HTTP/read-body infrastructure | `generator_reads_real_chunked_http_and_writes_lossless_native_file`: bounded loopback HTTP, chunked Unicode bytes, surrogate keys/values and non-rounded finite price; GREEN |

## Representation and approved adaptations

The generator accepts/returns shared `JsValue`, with `JsString` code-unit identity. It does not expose the runtime `ImagesModel` facade: that type cannot represent lone-surrogate names/IDs. Parsed costs retain f64 NaN/Infinity/-0; only the shared JSON writer applies JSON.stringify's non-finite-to-null rule. Native artifact layout and JS-frame omission are explicitly approved in the [spec](../../specs/image-catalog-generator.md); no other workflow change is implied.

Sorting happens before native object construction, whose shared writer applies JavaScript integer-key enumeration. Source TypeScript output text is not compared to native JSON bytes; its imported `IMAGE_MODELS` model values and order are compared exactly, including absent properties and raw UTF-16 strings. Checked-in baseline `data/image-models.json` remains untouched.

## Source discrepancies (classified, faithfully preserved)

- **Object-literal prototype setters:** source `Object.fromEntries` retains ID `__proto__`, but emitted quoted TypeScript object-literal keys become a prototype setter rather than an own model key. Actual source subprocess reports Fetched1 and success; importing its artifact gives `JSON.stringify(IMAGE_MODELS) === '{"openrouter":{}}'`. Native generation therefore omits this own catalog key. Object-valued raw `name`/`id` JSON expressions similarly lose nested `__proto__` own keys when evaluated as TypeScript literals; the native projection preserves this observable source behavior. `generated_artifact_matches_source_prototype_named_model` compares both concrete cases against actual generated source. RED and GREEN: `/tmp/pi-rust-image-generator-proof/prototype-{red,green}.log`.
- Raw malformed records can throw despite source TypeScript record declarations. These are representable JSON inputs, not the approved invalid-JavaScript-typed-input exclusion. Parser and workflow tests retain these throws.

## Oracle isolation and evidence

`oracle/image-catalog-generator-oracle.ts` imports the actual pinned parser. Workflow tests copy the script's exact bytes to a new temporary package and check byte equality before executing its unchanged main through the pinned tsx/config. Its relative output remains inside that package. Fetch is replaced by a controlled `Response`; console observation records events while forwarding unchanged calls, so raw stdout/stderr and exit remain intact. Generated TypeScript is actually imported, not regex-parsed or reimplemented. Outer oracle process uses existing `TsOracle` source-HEAD verification, watchdog, timeout and process-group cleanup; inner subprocess has a 20-second timeout.

Raw per-case results and native stderr: `/tmp/pi-rust-image-generator-proof/{workflow,raw}-*-source.json`, `*-native-stderr.txt`. Source diagnostics retain all JS frames; comparisons remove only approved JS frames and substitute the two approved artifact paths. Streams and event order are otherwise exact. Revision/command evidence belongs in that directory's ledger.

## Current proof level

Partial: controlled workflow GREEN (23 cases), raw generation GREEN (11 cases), parser GREEN (18 × strict/non-strict). Shared filesystem/collation prerequisites are integrated. Two enabled behavioral REDs remain: `malformed_first_id_keeps_source_sort_boundary_for_longer_inputs` (65 records; V8 succeeds, Rust invokes null receiver) and `native_closed_loopback_failure_matches_pinned_generator_error_header` (native message-only error versus source TypeError plus ECONNREFUSED cause metadata). The sort decision is user-owned; transport shape changes await parent coordination. Full raw source/native transport diagnostics are retained as `closed-loopback-source.json` and `closed-loopback-native-stderr.txt` in the proof directory. No waiver or ignored test is used. Parent owns integration gates. This slice performs only focused development RED/GREEN and formatting; no live refresh or broad/final acceptance is claimed.
