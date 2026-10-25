# Image catalog generator — common pin

Frozen: `packages/ai/scripts/generate-image-models.ts`, 4f9e6f8a4. Native: `image_catalog_generator.rs` and `generate-image-models` binary. Native artifact is the evaluated generated module projected to lossless JSON (existing approved boundary), not TypeScript source text.

| Frozen symbols / branch | Native behavior and proof |
|---|---|
| fetchOpenRouterImageModels:29–82 | One GET, parse JSON without response.ok guard, log before fetch and after projection. Any fetch/body/parse error logs and returns an empty list. `image_catalog_generator::native_workflow_matches_pinned_script_values_order_logs_and_failures`, loopback HTTP tests. |
| Record projection, modalities/pricing | Retain all usable records, including duplicate IDs until generation. Ordered text/image Set projection, default text input, parseFloat pricing ×1e6. Real workflow includes skipped non-image records, duplicated modality entries, all four prices and duplicate IDs. |
| generateImageModelsFile:84–124 | Sort IDs with shared collator, Object.fromEntries last duplicate wins, JS key enumeration, UTF-16 names and literal-materialization semantics. Compare full parsed values and ordered serialization with exact source-written/evaluated modules. |
| main:126–139 | Ignore CLI arguments; fetch failure still writes empty artifact, overwrite existing files, emit Generated path after successful write, propagate write failures. Ordered stdout/stderr events, artifact and status compared with pinned script. |
| GAP: adversarial input/OS/runtime oddities | Wrong-typed/exotic record shapes, sort faults, stdio/entropy faults and Node/V8 scheduling details are outside user-observable parity (2026-10-25); no new matrices or carriers. Existing prototype projection is retained and checked, not expanded. |

DISCREPANCY preserved: generated quoted `__proto__` keys change prototype rather than create an own property; both the outer model entry and nested source literals match existing frozen behavior.

## Retired pre-pin contracts

The frozen script has no exported parseOpenRouterImageModels, no strict option/parser, no early deduplication or custom field validation, and no HTTP status check. Removed those native behaviors. Retired parser-only/required-name/wrong-field/strict CLI fixtures and the malformed-sort fault matrix; these tested old-only interfaces or approved pathological exclusions, not the frozen script. The prototype fixture now runs through the real workflow instead of the removed parser API. HTTP tests prove empty artifact and success status both with no arguments and with ignored `--strict`.

Oracle no longer imports the auto-running script into the JSONL process. It copies exact source bytes to an isolated temporary package and executes them with an injected fetch; generated module is then evaluated for the complete value/order comparison. No provider request is made. The original baseline failed at oracle framing; after that was corrected, the unchanged workflow assertion reproduced native Fetched1 versus frozen Fetched2 for duplicates (`/tmp/ai-image-script-red.log`).
