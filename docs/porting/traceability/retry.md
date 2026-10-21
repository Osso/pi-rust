# Retry classifier traceability

Frozen authority: `packages/ai/src/utils/retry.ts`, commit `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d` ([lock](../source-lock.json)); Rust `crates/pi-ai/src/utils/retry.rs`; [contract](../../specs/retry.md).

## Source capability → focused proof

| Source region / branch | Rust / test |
|---|---|
| lines 3–15 canonical restriction, exact API and stop gate, missing/empty text, both case-sensitive prefixes and arbitrary suffix | `is_codex_cybersecurity_restriction`; both-export source differential API × stop × prefix matrix; `frozen_codex_cybersecurity_wins_over_transient_suffix` |
| lines 17–19 pattern construction / case-insensitive search | `build_provider_error_pattern`; differential embedded/uppercase patterns and optional separator matrix |
| lines 21–42 all 10 terminal alternatives precede retryable alternatives | `non_retryable_pattern`; differential each terminal string embedded with `503` and `timeout`, uppercase |
| lines 44–57 generic HTTP/load alternatives | `retryable_pattern`; concrete source differential status/load/server fixtures |
| lines 59–61 provider wrapper | differential `provider returned error` plus separator matrix |
| lines 63–82 network/transport | differential concrete network fixtures plus separator matrix; legacy-only DNS/resource messages negative against source |
| lines 84–87 WebSocket generic + anchored 101 alternative | differential URL scheme/content/whitespace, optional Error prefix, prefix/suffix, case and line terminator fixtures; `frozen_exact_transport_and_access_errors` |
| lines 89–97 incomplete streams | differential concrete incomplete/stream/HTTP2 fixtures; deterministic terminal reasons remain excluded |
| lines 99–101 delay | differential `retry delay` |
| lines 103–104 Daybreak Blue | differential exact sentence, optional request ID, punctuation, empty ID, CR/LF/Unicode IDs, anchors and case |
| lines 106–110 explicit guidance | differential all three guidance strings |
| lines 121–125 missing/empty text, non-error stop, cybersecurity exclusion, terminal exclusion, positive/default result | `is_retryable_assistant_error`; differential all five stop reasons and absence/empty/transient text, API/prefix cross product |
| JS legacy regex character semantics | ASCII folding (patterns are ASCII), one-BMP-unit dot excluding JS line terminators, exact JS whitespace class; differential supplementary separator, long-s/Kelvin, U+FEFF/U+0085, whitespace in URL/status/prefix; `frozen_regex_uses_javascript_characters` |
| package surface | existing wildcard root reexport unchanged; exact new `is_codex_cybersecurity_restriction` call in `tests/public_index.rs` |

`tests/differential_retry.rs` calls only the two actual frozen source exports through `oracle/retry-oracle.ts`; no retry-loop operation or native expected-value substitute remains. Historical source-absent loop oracle assumptions removed. Test driver compares actual booleans without numeric normalization. Typed invalid runtime input exclusion is unchanged.

## Development proof

Before production edits: actual-source differential failed request 7 (`ResourceExhausted`: Rust true / TS false); focused native tests failed concrete WebSocket-101, cybersecurity precedence, supplementary separator and ResourceExhausted classifiers. Direct protected source probes confirmed the regex/anchored cases. At `28e5d63f047dc9e42c4d816be9865eb1ba84e65d`, focused source differential passes 508 requests × both exports (1,016 boolean comparisons). Unchanged retry/public-index targets passed at `44fbc129` (13 + 2 tests: four native classifier tests, nine historical loop regressions, two public-index tests). All 31 temporary semantic mutants failed concrete classifier assertions against the unchanged source oracle: all ten terminal exclusions individually, cybersecurity/API/stop/prefix gates, retry stop gate, legacy ResourceExhausted acceptance, ASCII folding, UTF-16 dot, JS whitespace, separator cardinality, WebSocket scheme/quotes/anchors/whitespace cardinality, Daybreak punctuation/anchors/request ID. Mutations were restored; no mutant survives in committed code. Exact commands, full revisions, scope hashes, mutation patches and logs: `/tmp/pi-rust-repin-ai-retry/{REPORT.md,ledger.md}`. Cargo tests used the mandatory runner; direct source probes used `python3 scripts/test-sandbox.py`. Development proof only: no check/clippy/readability/broad-suite/final-acceptance claim.

## Source-absent native surface: staged removal BLOCKED

Retained unchanged solely to compile: `RetryPolicy`, `RetryCallbacks`, `retry_assistant_call`, `LastRetry`, `should_retry`, `retry_delay_ms`, `finish_assistant_call`, `wait_for_retry`. No such policy/callback/loop symbols exist in the frozen module. Their removal is authorized, not waived; this slice does not convert coupled callers.

| Coupled consumer owner | Remaining dependency |
|---|---|
| `pi-agent-core/src/harness/agent_harness.rs` | public `AgentHarnessOptions.retry`, stored retry policy, getter/setter/default |
| `pi-agent-core/src/harness/compaction/mod.rs` | `NoRetryCallbacks`, retry-bearing options, completion wrapper invokes loop, generic summary/compact helpers |
| `pi-agent-core/src/harness/compaction/branch_summarization.rs` | retry-bearing options and generic callbacks |
| `pi-coding-agent/src/core/compaction.rs` + `compaction/branch_summarization.rs` | callback-generic completion/summary/compact APIs, retry options and direct loop invocation |
| `pi-coding-agent/src/core/agent_session.rs` | retry-policy construction and callback-bound compaction calls |

Associated tests importing historical APIs: pi-ai `retry` and `public_index`; agent-core `agent_harness`, `differential_agent_harness`, `compaction`, `branch_summarization`; coding-agent `create_harness`, `compaction_generation`, `branch_summarization`, `package_index`. Existing compaction traceability mentions the native infallible retry producer; that is caller-owned historical behavior, not this source module's contract. No separate retry-loop spec found; obsolete retry traceability claims replaced here. Existing native loop tests remain historical only, not new-pin evidence.

Whole-module status remains TODO until those owners convert/remove the native-only API, its private helpers and public leakage. Classifier proof cannot promote the row, waive the blocker or claim whole-module parity. No source bug or new deviation classified.
