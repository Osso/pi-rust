# openai-remote-compact extension traceability

Pinned source: `packages/coding-agent/extensions/openai-remote-compact/src/index.ts` at
`532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-coding-agent/src/bundled_extensions/openai_remote_compact.rs`.
Focused tests: `crates/pi-coding-agent/tests/openai_remote_compact.rs` (2),
`crates/pi-coding-agent/tests/differential_openai_remote_compact.rs` (15 differential cases).

| TypeScript source | Contract | Rust and proof |
|---|---|---|
| `OPENAI_REMOTE_COMPACTION_SUMMARY`, guidance, `DETAILS_TYPE`, `DETAILS_VERSION`, limit, codex model | exact literals | same-named constants |
| `openAIRemoteCompactExtension` | registers the `compaction` handler | `openai_remote_compact_extension`; `registers_a_compaction_handler` |
| `handleCompaction` | non-OpenAI models no-op, auth failure notifies and no-ops, payload/post/details, provider-native result | `handle_compaction`; helper behavior covered by the differential |
| `buildOpenAICompactInstructions` | trim sections, custom focus line, `\n\n` join | `build_openai_compact_instructions`; differential (3 cases) |
| `isOpenAIResponsesModel` | openai responses + codex responses providers | `is_openai_responses_model`; `recognizes_openai_responses_models` |
| `buildOpenAICompactPayload` | converted messages, tool-pair reconciliation, pinned messages, previous-history fallback, input limit | `build_openai_compact_payload`; differential (2 payload cases) |
| `extractOpenAICompactDetails` | output/compaction validation, dedup, byte/token counts | `extract_openai_compact_details`; differential |
| `buildCompactEndpoint` | openai `/responses/compact`, codex `/codex/responses/compact` | `build_compact_endpoint`; differential (3 base URLs) |
| `buildOpenAIRequestHeaders` | content type, header passthrough, bearer fallback, codex account headers | `build_openai_request_headers`; differential (3 header cases) |
| `extractCodexAccountId` | JWT payload account id, exact failure text | `extract_codex_account_id` |
| `findLatestOpenAIReplacementHistory` | newest compaction details, provider/api/model gate | `find_latest_openai_replacement_history` |
| `limitOpenAICompactInput` | batch grouping, pinned/native sets, image dropping, text truncation | `limit_openai_compact_input`; differential (4 limit cases incl. 500k-char truncation) |
| `postOpenAICompact`, `parseOpenAICompactResponse`, `formatOpenAIError` | POST via `Fetch`, JSON fallback, status error text | `post_openai_compact`; HTTP through `pi_ai::http::default_fetch` |

## Numeric-key budget traversal

`truncateReducibleText` (`index.ts:535-552`) consumes a shared budget in
`Object.entries` order. Native `truncate_reducible_text` uses
`js_json::object_entries`: integer-index keys ascend before insertion-ordered
string keys. `numeric_properties_consume_compaction_budget_in_javascript_order`
compares the entire result with pinned TS for property `2` (B×300000) inserted
before `1` (A×300000), exceeding the 400000-character cap.

Traversal audit: `count_reducible_text_chars` only sums lengths, independent of
object order; `canonicalize_json_value` explicitly sorts by source localeCompare
before JS stringify; request-header traversal handles header names, not JSON
record budget allocation. Remaining traversals are arrays, typed collections,
or cloned records with named-field updates. No other object traversal consumes
a shared budget. Evidence: `/tmp/pi-rust-core-c-fixes/ledger.md`.

## Gaps

- The live HTTP round trip is not exercised in tests (no external services); all request construction,
  response parsing, and error formatting helpers are differentially covered.
- JS object identity in `limitOpenAICompactInput`/pinned-message filtering is represented by indices into
  the constructed item list.

## Discrepancies

- `String.prototype.slice` truncates UTF-16 code units and can split a surrogate pair; Rust strings cannot
  hold lone surrogates, so `utf16_prefix` replaces a split pair with U+FFFD. Only reachable when a text
  truncation boundary lands mid-surrogate. Approved deviation, user October 20, 2026 (“Approve like
  truncateLine”); recorded in [compatibility](../../specs/compatibility.md#out-of-scope).

## JS-value boundary regression (2026-10-20)

Shared JS stringify/object enumeration is used at this module's audited observable boundary; values and parser errors are unchanged. Concrete counterexamples and per-boundary tests: [JS-value proof](../js-value-semantics-proof.md).
