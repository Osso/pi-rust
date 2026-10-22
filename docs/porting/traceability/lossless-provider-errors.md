# Human provider errors — Q18

Source: immutable common pin in [source lock](../source-lock.json).
Scope: human error text only; existing ID/name/type/code metadata policy remains pending approval and unchanged. Source Mistral text/thinking sanitizer remains untouched. Nonstream image payload ingress remains outside this slice.

## Native API

`AssistantMessage.error_message`, `AssistantImages.error_message`, `CallbackError.message`, SDK/provider error messages and bodies, retry-limit provider messages, diagnostic messages and thrown text use `JsString`. Provider inner-error payloads use `JsValue`. Error formatters return `JsString`; formatting prefixes concatenate UTF16 units. Callback/fetch and retry callbacks carry native error text. Coupled OAuth/auth, image and other-provider callback consumers use the same native contract; actual Rust Display and rendered OAuth HTML are UTF8 display boundaries.

## Proof matrix

| Boundary | Behavioral proof | State |
|---|---|---|
| SDK SSE error → completions/Responses/Azure → Error event + final result | `lossless_provider_errors::sse_human_error_is_persistent_and_final_exact_utf16` | Initial RED: scalar SDK inner-error projection replaced the provider message with a parse error |
| Codex SSE error → native API error + diagnostics | Same loopback test | PASS at `0db07504`; expanded parser cases pending |
| Codex HTTP human errors, raw SSE/WS parser excerpts, Mistral HTTP UTF16 truncation | `codex_http_error_message_keeps_unmatched_units`, `sse_parser_failure_keeps_split_surrogate_excerpt`, `codex_websocket_error_and_parser_failure_keep_native_units`, `mistral_http_body_truncation_splits_exact_utf16_unit` | Behavioral RED established before these fixes; PASS at `d352d3b2` for error transport proofs |
| Responses state-machine named `error` and `response.failed` | `responses_state_machine_human_errors_remain_native`: loopback + frozen provider, Responses/Azure | PASS at `05711a09` |
| Callback + HTTP errors | `callbacks_and_sdk_http_errors_match_frozen_provider_errors`: actual frozen provider, 5 APIs × payload/response/HTTP | PASS at `05711a09`; Mistral response-call discrepancy below |
| Retry/overflow + diagnostic persistence | `retry_overflow_and_diagnostics_use_native_error_authority`, `codex_retry_callback_keeps_raw_fetch_failure_message` | Retry callback RED: U+FFFD instead of unmatched units; PASS at `d352d3b2` for error transport proofs |

No whole-provider parity or broad verification claimed. Final proof ledger will be recorded in `/tmp/pi-rust-repin-lossless-provider-errors/REPORT.md`.

## Pre-existing port discrepancy

Frozen `mistral-conversations.ts` does not call `onResponse`; Rust already does. This slice retains that invocation, tests its human error text without claiming source invocation parity, and leaves full Q4 reconciliation open. This is neither a source bug nor an approved deviation.

`ProviderRetryEvent.reason` is now `JsString`; Codex retry notifications use error message data, never Display. Native retry/overflow classification executes the existing patterns against UTF16 units with the existing regress dependency; public scalar overflow-pattern enumeration remains available for callers.

Retry classification retains frozen non-Unicode `/i` behavior (no Unicode lowercasing projection): `retry::frozen_regex_uses_javascript_characters` rejected the initial Kelvin-sign fold; regress `i` also folded long-s unlike frozen non-Unicode JS. The matcher now folds ASCII code units only and uses native UCS2 regex without flags. Unmatched units and all non-ASCII units remain unchanged.

## Proof boundaries

SDK native inner error and nested human `metadata.raw` are asserted directly and through a completions SSE final error. `provider_inner_payload_and_retry_limit_text_remain_native` asserts retry-limit formatting without Display. Two behavior mutants (SDK U+FFFD projection, Codex parser Display) compiled and failed exact final/persisted UTF16 assertions; production files were restored byte-identically.

Readability: manual changed-line Rust and oracle audit; `rust-code-analysis-cli` unavailable on PATH. No warning suppressions or new dependency. Existing large provider state machines were not redesigned. No check/clippy/type/coverage/broad acceptance gate was run.

Existing direct provider/inline fixture targets are not completion evidence: they contain unrelated base `ec129b36` content-API fixture drift (native JsValue versus scalar values, native citation/content strings). Native error fixture consumers observed in those targets were adapted; remaining integration belongs to the full port gate. Focused runtime proofs are separate and passing.

Final focused error target: 11 PASS at `05711a09`, no warnings. Existing ingress target: 6 PASS at `d352d3b2`; subsequent edits do not change its content paths. Focused utility evidence is recorded in the external handoff ledger.

## Q18 core error consumers

`lossless_provider_errors` in pi-agent-core proves raw StreamFailure UTF16 through agent-loop failure results, failed/aborted compaction response prefixes, and failed assistant JSON serialization. AgentState, CompactionError, BranchSummaryError and proxy human errors retain JsString; Display alone encodes. Expanded core target also proves Agent state/listener/final serialized failure and compaction retry scheduled/finished callbacks. Core hook and coding-session retry integration proof remains in progress. No parent integration claim.

Coding consumer contracts: CompactionError.message(), BranchSummaryResult.error, AutoRetryEndEvent.final_error, ModelRuntimeError::Message/message(), CredentialSynchronizationError.cause, ModelRuntime/ModelRegistry.get_error(), ResolvedRequestAuth.error and model_catalog_refresh_outcome retain JsString. Native auth callback reasons feed availability and lazy-stream errors without Display. Coding proof target `lossless_provider_errors` is currently blocked by concurrent-lane library compile errors; not runtime parity evidence.
