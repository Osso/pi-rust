# Image catalog generator

Native counterpart of frozen `packages/ai/scripts/generate-image-models.ts` at 4f9e6f8a4. Runtime image lookup keeps the checked-in catalog; this work does not refresh it.

## Contract

- Fetch one GET `/models?output_modalities=image`, then parse the body without checking HTTP status. Print the frozen progress messages. Fetch/body/parse failures print the frozen error header and produce an empty catalog.
- Preserve every image-capable record until generation, including duplicate IDs. Filter/deduplicate modalities in first-seen order, default an empty input list to text, and compute all four `parseFloat(price || "0") * 1_000_000` prices.
- Sort model IDs with the shared collation authority. Duplicate IDs use the last record. Preserve JavaScript key enumeration, exact UTF-16 names and evaluated generated-module values.
- Ignore command-line arguments. Write the artifact even after a fetch failure; propagate write failures. Print the generated path only after a successful write.
- Keep frozen generated-literal `__proto__` behavior as a documented discrepancy.

## Approved native boundaries

The existing 2026-10-20 native-JSON decision selects `crates/pi-ai/data/image-models.json` instead of generated TypeScript. The artifact is the evaluated source catalog as two-space lossless JSON, without a terminal newline. There is no alternate path or automatic baseline refresh.

JavaScript stack frames are omitted. Existing native-cause approval permits native HTTP cause diagnostics, while top-level names/messages, console ordering, status and output remain exact.

The 2026-10-25 user-observable parity decision excludes pathological JSON shapes, OS-fault shapes and Node/V8 runtime oddities. This supersedes the old pin's strict-parser/validation matrices; no strict option or exported parser exists at the common pin. Realistic paths must not panic or lose text.

## Proof

[Traceability](../porting/traceability/image-catalog-generator.md) maps the frozen functions and branches. The isolated oracle executes exact source bytes with a local fetch fixture, then evaluates the emitted module. Tests compare complete values, ordered serialization, stdout/stderr events, exit status, overwrite and filesystem failure behavior. Real loopback tests cover chunked bodies, connection refusal and truncated bodies without contacting a provider.

## Exclusions

No live OpenRouter calls, catalog refresh, credentials, deployment, runtime image API changes, new dependencies or standalone `pi-ai` CLI implementation.
