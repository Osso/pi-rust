# headers traceability

Source: `packages/ai/src/utils/headers.ts`.
Rust: `crates/pi-ai/src/utils/headers.rs`.
Tests: `crates/pi-ai/tests/headers.rs` (2 behavioral), `crates/pi-ai/tests/differential_headers.rs` (4 header + 3 provider requests vs pinned TS).

## Representation
- `headersToRecord(headers: Headers)` takes header entries (`IntoIterator<Item = (String, String)>`) because Rust has no Web `Headers` type. Normalization mirrors `Headers.entries()`: ASCII-lowercase names, trim HTTP whitespace (`SP HT LF CR`), append duplicates with `", "`.
- `providerHeadersToRecord` drops `None` (TS `null`) values and returns `None` for an empty record.

## Symbols → proof
| TS | Rust | Test |
|---|---|---|
| `headersToRecord` | `headers_to_record` | `lowercases_names_and_joins_duplicate_values_like_headers_entries`, differential 0-2, 6 |
| `providerHeadersToRecord` | `provider_headers_to_record` | `provider_headers_drop_null_values_and_empty_maps`, differential 3-5 |

## Approved native ingress boundary

| Approval | Native behavior and proof |
|---|---|
| [Opaque-scalar approval2026-10-21](../../../AGENTS.md), header names classified as protocol tokens2026-10-24 | `validate_opaque_header_names` rejects lone-unit keys explicitly with provider/model context before scalar-key decoding in models.json provider/model/override headers, catalog-cache→registry models and generated catalog reader. No replacement, silent drop or panic. `opaque_header_names` covers real config files and isolated cache-startup processes; `providers::models_data::tests::catalog_reader_*` covers the reader. Paired/scalar names and lone-unit header values stay exact. Frozen's later ByteString TypeError for a lone-unit name is unreachable under this approved native domain; header values retain Node ByteString validation. |

## Gaps
- `Headers` constructor validation (invalid names/values, non-ASCII names) is not reproduced; callers pass already-valid entries. Typed-input exclusion.

## Discrepancies
- None.
