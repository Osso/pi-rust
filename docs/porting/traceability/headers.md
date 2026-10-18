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

## Gaps
- `Headers` constructor validation (invalid names/values, non-ASCII names) is not reproduced; callers pass already-valid entries. Typed-input exclusion.

## Discrepancies
- None.
