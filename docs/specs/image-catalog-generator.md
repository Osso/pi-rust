# Image catalog generator

Native maintenance counterpart of pinned `packages/ai/scripts/generate-image-models.ts`. Runtime image lookup continues embedding the unchanged checked-in `crates/pi-ai/data/image-models.json`. [Traceability](../porting/traceability/image-catalog-generator.md) maps source branches to focused proofs.

## What it must do

- [x] Parse raw JSON model data with source modality filtering/deduplication, first-usable-ID deduplication, default text input, fixed API/provider/base URL, optional id/name fields, and all four parseFloat-times-one-million costs.
- [x] Retain UTF-16 identity and binary64 values through parsing. Raw JSON malformed records retain source TypeErrors; they are not excluded as invalid typed inputs. Non-finite costs become JSON null only when serialized.
- [x] Accept repeated `--strict` only; fetch one GET `/models?output_modalities=image`; preserve strict/non-strict controlled errors, progress/error logging order, exit status, native writing, and filesystem failure metadata in the 23-case pinned workflow corpus.
- [x] Emit lossless native JSON with JavaScript canonical integer-key enumeration. Preserve the classified source object-literal `__proto__` disappearance, including nested raw id/name values.
- [x] Compare every generated model value and ordered key against the real pinned script, not a reimplemented oracle, including lone-surrogate string model IDs and 11 raw-ID/name cases.
- [ ] Preserve malformed-ID sort invocation and actual native transport failure diagnostics; concrete RED cases remain below.
- [x] Consume real runtime HTTP transport against a bounded loopback chunked server and write only a temporary artifact in tests.

### Approved adaptations

2026-10-20 question: “For remaining model/image catalog generators, should generated artifacts use Rust native JSON catalog layout or preserve original TypeScript package artifacts?” Alessio answered **“Native JSON catalogs (Recommended)”**.

The maintenance binary `generate-image-models` writes exactly `crates/pi-ai/data/image-models.json` under its build-time package root. Artifact shape is `{"openrouter": {"<model id>": <model>}}`, two-space JSON indentation, JavaScript key enumeration/number formatting/escaping, and no terminal newline (`JSON.stringify(catalog, null, 2)`). No TypeScript path fallback, extra flags, or baseline refresh accompanies this implementation.

2026-10-20 error presentation decision: **“Omit JS frames (Recommended)”**. Only JavaScript-specific stack frames may differ; error names/messages, filesystem error metadata, logging order, and process exit behavior remain observable contracts. Differential evidence retains full unaltered source stdout/stderr before omitting stack frames for comparison.

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

- [ ] A concrete 65-record malformed-ID input succeeds in V8 but Rust sorting invokes the null receiver and throws. Validation versus faithful V8 sort invocation awaits user decision; the behavioral RED remains enabled.
- [ ] Actual closed-loopback transport returns a message-only native `CallbackError`; source returns `TypeError: fetch failed` with ECONNREFUSED cause metadata. No native-network waiver is approved; the behavioral RED remains enabled. Shared transport changes require parent coordination.
- [x] Shared ICU4C UTF-16 collation passes 256 Node pairs; shared Node filesystem records pass both generator write-failure cases.
- [ ] Parent owns integrated verification; no check, clippy, readability, broad suites, final gate, merge, deployment, or live refresh is part of this development slice.

## Out of scope

- Refreshing the checked-in catalog, live OpenRouter calls, live configs, credentials, and changes to runtime image APIs.
- `generate-models.ts`, model-data/checker modules, reasoning-options generation, and the approved standalone `pi-ai` CLI gap.
- A typed model facade: Rust UTF-8 model fields cannot represent all valid raw JSON strings. Public generator values remain `JsValue` rather than silently replacing lone surrogates.
