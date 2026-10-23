# List models

Port frozen `cli/list-models.ts` at4f9e6f8a4. [Traceability](../porting/traceability/list-models.md) maps the console and registry boundaries.

## What it must do

- [x] Print a nonempty models.json load warning to stderr before reading available models; a later availability error preserves that already-emitted warning and the concrete native error.
- [x] Print no-model guidance or no-match text as appropriate; search and human errors retain exact UTF-16 until the stdout/stderr boundary.
- [x] Fuzzy-match one concatenated provider/id key, including a single token spanning both fields; sort by provider then id with the shared locale comparison.
- [x] Print the six-column table one console line at a time, with source UTF-16 padding, token-count formatting and flags.
- [x] Suppress an empty load-error warning; empty/whitespace search keeps the catalog.

## How it works

- [Source branches and proof](../porting/traceability/list-models.md).

## Implementation inventory

- `crates/pi-coding-agent/src/cli/list_models.rs`: literal ordered console writes through the existing CLI IO-trait pattern, synchronous registry reads and table formatting.
- `crates/pi-coding-agent/src/main.rs`: production StdioListModelsIo caller; preserves NativeNodeError through CallbackError.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/differential_list_models.rs`:13 actual-frozen cases; complete stream-write sequence, partial output on rejection, constructor/own-error properties and raw UTF-16.

## Known gaps (current cycle)

- [ ] Targeted clippy/readability and independent final lane gates remain open.

## Out of scope

- Fetching live provider catalogs; availability is supplied by ModelRegistry.
- Provider/model IDs remain scalar under the common-pin approval; display model names are not part of the frozen table.
