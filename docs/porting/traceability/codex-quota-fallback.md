# codex-quota-fallback.ts

Pin: [source-lock](../source-lock.json). Contract: [spec](../../specs/provider-restriction-recovery.md).

| Export / branch | Rust | Proof |
|---|---|---|
| CODEX_FALLBACK_PROVIDERS ordered map | `core/codex_quota_fallback.rs` ordered static pairs | `differential_codex_quota::quota_detection_and_provider_order_match_frozen` |
| isQuotaExhaustionError: error stop, truthy error message, literal case-insensitive pattern | same | Actual frozen module, 77 messages: all five stop tags, every pattern arm, absent/empty/nonmatching messages, raw UTF-16 prefix |

No fallback execution is introduced; AgentSession consumer integration remains separate.
