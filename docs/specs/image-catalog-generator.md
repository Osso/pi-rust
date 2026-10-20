# Image catalog generator

Native maintenance counterpart of pinned `packages/ai/scripts/generate-image-models.ts`. Runtime image lookup continues embedding the unchanged checked-in `crates/pi-ai/data/image-models.json`. [Traceability](../porting/traceability/image-catalog-generator.md) maps source branches to focused proofs.

## What it must do

- [x] Parse raw JSON model data with source modality filtering/deduplication, first-usable-ID deduplication, default text input, fixed API/provider/base URL, required string names on usable nonduplicates, and all four string-price parseFloat-times-one-million costs.
- [x] Retain UTF-16 identity and binary64 values through parsing. Non-finite costs become JSON null only when serialized. Reject wrong consumed field types with stable native TypeErrors under the narrow approval below.
- [x] Accept repeated `--strict` only; fetch one GET `/models?output_modalities=image`; preserve strict/non-strict controlled errors, progress/error logging order, exit status, native writing, and filesystem failure metadata in the 23-case pinned workflow corpus.
- [x] Emit lossless native JSON with JavaScript canonical integer-key enumeration. Preserve the classified source object-literal `__proto__` disappearance for valid string IDs. Object-valued id/name are rejected, not projected.
- [x] Compare every generated model value and ordered key against the real pinned script, not a reimplemented oracle, including lone-surrogate string model IDs and 11 raw-ID/name cases.
- [x] Reject malformed usable IDs independent of V8 sort invocation, retaining concrete source witnesses.
- [x] Propagate structured shared `CallbackError` name/message/cause through request and body failures without parsing diagnostic strings.
- [x] Consume real runtime HTTP transport against a bounded loopback chunked server and write only a temporary artifact in tests.

### Approved adaptations

2026-10-20 question: “For remaining model/image catalog generators, should generated artifacts use Rust native JSON catalog layout or preserve original TypeScript package artifacts?” Alessio answered **“Native JSON catalogs (Recommended)”**.

The maintenance binary `generate-image-models` writes exactly `crates/pi-ai/data/image-models.json` under its build-time package root. Artifact shape is `{"openrouter": {"<model id>": <model>}}`, two-space JSON indentation, JavaScript key enumeration/number formatting/escaping, and no terminal newline (`JSON.stringify(catalog, null, 2)`). No TypeScript path fallback, extra flags, or baseline refresh accompanies this implementation.

2026-10-20 error presentation decision: **“Omit JS frames (Recommended)”**. Only JavaScript-specific stack frames may differ; error names/messages, filesystem error metadata, logging order, and process exit behavior remain observable contracts. Differential evidence retains full unaltered source stdout/stderr before omitting stack frames for comparison.

### Wrong-field approval (2026-10-20)

Question: “For catalog responses with wrong-typed fields such as `id: null`, should Rust reject invalid records or reproduce V8’s accidental sort/exception behavior exactly? Exact matching now requires porting V8’s sorting algorithm; valid string IDs are unaffected.”

Answer: **“Reject wrong field types (Recommended)”**. This is not a blanket invalid-input exclusion.

Outer missing/empty/nonarray `data` behavior stays source-exact in both strict modes. Records must be objects. At source read points, architecture/pricing may be missing or null; otherwise they must be objects. Present modality arrays may be null (the source `?? []` contract); non-null arrays must contain strings. Unsupported string modalities remain filtered, preserving dedup/order/default text. Usable model IDs must be strings even when absent/null, before dedup. Names have no source default: missing or invalid name rejects on usable nonduplicate records, at the original name read before pricing. Skipped and duplicate names remain unread. Missing price fields default to zero; present prices must be strings, not null/coercible numbers/arrays/objects. String parseFloat NaN/Infinity/-0/overflow remain unchanged. No finite/range checks, validation of unconsumed extra fields, or all-required DTO gate. Fields after a source skip/dedup boundary remain unread.

Errors use `TypeError: Invalid OpenRouter image model <field>: expected <type>`. Non-strict fetch logs rejection then writes the empty catalog; strict logs/rethrows without writing. Wrong-type fixtures retain full actual-source witnesses and explicitly assert this approved native rejection instead of skipping comparisons.

### Native-cause approval (2026-10-20)

Question: “For native HTTP failures, may Rust preserve the top-level error name/message but use native cause diagnostics instead of reproducing Node’s nested errno/address/cause objects? Failure ordering and exit behavior would still match.”

Answer: **“Allow native causes (Recommended)”**. Only native cause diagnostics may differ; top-level error name/message, console order and exit status still match Node. JS-frame omission remains separately scoped above. Request and body failures retain shared structured identity; native cause diagnostics are printed beneath the unchanged top-level name/message. Tests retain raw Node inspection and omit only JS frames and the stackless fetch-error inspection brackets when comparing headers.

## How it works

- [Source-to-test mapping, representation, and discrepancies](../porting/traceability/image-catalog-generator.md)
- [Shared lossless JSON contract](js-json.md)

## Implementation inventory

- `crates/pi-ai/src/image_catalog_generator.rs` — public lossless parser and fetch/generate/write workflow.
- `crates/pi-ai/src/bin/generate-image-models.rs` — scoped maintenance entrypoint; not the standalone `pi-ai` CLI port.
- `crates/pi-ai/src/lib.rs` — public module declaration.

## Tests asserting this spec

- `crates/pi-ai/tests/image_catalog_generator.rs` — exported parser differential; pinned script subprocess, streams/events/errors/exit and artifact values/order; native binary argument rejection.
- `crates/pi-ai/tests/image_catalog_prototype.rs` — actual generated TypeScript artifact evaluation for `__proto__` source bugs.
- `crates/pi-ai/tests/image_catalog_http.rs` — real HTTP client, chunked body, surrogate file boundary, temporary filesystem.
- `oracle/image-catalog-generator-oracle.ts` — imports the actual pinned parser; copies exact script bytes into an isolated package, overrides only fetch/console observation, runs through source tsx under the shared watchdog, and imports the real generated artifact.

## Known gaps (current cycle)

- [x] Concrete malformed-ID source witnesses (lengths 3–128, including V8-success length65) now assert approved TypeError rejection in strict mode, independent of sort invocation.
- [x] Required-name workflows exercise missing/null/bool/number/array/object names in both modes, name-before-pricing rejection, and unread skipped/duplicate name/pricing fields.
- [x] Actual isolated closed-loopback and truncated-body transport exercise both strict modes: `TypeError: fetch failed` / `TypeError: terminated`, retained native causes, source-exact event order/status/stdout and catalog/write boundary.
- [x] Shared ICU4C UTF-16 collation passes 256 Node pairs; shared Node filesystem records pass both generator write-failure cases.
- [ ] Integrated at `abc0058`; independent protected verification remains pending. Development slice ran no check/Clippy/readability or broad gate; no deployment or live refresh. [Current proof](../porting/batch-a-proof.md).

## Out of scope

- Refreshing the checked-in catalog, live OpenRouter calls, live configs, credentials, and changes to runtime image APIs.
- `generate-models.ts`, model-data/checker modules, reasoning-options generation, and the approved standalone `pi-ai` CLI gap.
- A typed model facade: Rust UTF-8 model fields cannot represent all valid raw JSON strings. Public generator values remain `JsValue` rather than silently replacing lone surrogates.
