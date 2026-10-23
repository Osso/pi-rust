# provider-restriction-recovery.ts

Pin: [source-lock](../source-lock.json). Contract: [spec](../../specs/provider-restriction-recovery.md).

| Export / branch | Rust | Proof |
|---|---|---|
| Request options; requestCodexRestrictionRescope; signal before/after requester; evidence; context bound | `core/provider_restriction_recovery.rs` | `differential_provider_restriction_recovery::advisory_request_and_decisions_match_frozen` |
| Request fields; project lookup; response error/wrong kind; rejection; exact strict JSON decision; repeated-task normalization; basis quote | same | Same actual-frozen differential; request captures include exact context, question, timeout, attempt count, signal identity |
| formatCodexRestrictionValidation; full proposal; text-only evidence | same | `validation_and_prompt_match_frozen` exact string comparison |
| parseCodexRestrictionValidation; stop/error/content gates; split text/thinking; tools; strict fields; task/quote/denial | same | Same actual-frozen differential |
| Arbitrary malformed runtime AssistantMessage/content types | typed exclusion | [Compatibility](../../specs/compatibility.md) |

The helper is ported; AgentSession restriction context, advisory lifecycle and guarded execution integration remain separate unfinished work. Shared requester signal was added in an interface-only commit; client consumption is the subsequent approved slice.
