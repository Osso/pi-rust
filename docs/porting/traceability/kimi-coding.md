# Kimi Coding provider — common pin

Frozen `packages/ai/src/providers/kimi-coding.ts`, `4f9e6f8a4`, lines 1–16. Whole factory compared to Rust `providers/kimi_coding.rs`.

| Export / branch | Frozen lines | Proof |
|---|---|---|
| `kimiCodingProvider`: id/name/baseURL, ordered generated models | 6–15 | `provider_auth::kimi_provider_matches_frozen_without_subscription_oauth`, actual frozen descriptor/catalog |
| env API-key auth only, KIMI_API_KEY, no OAuth | 11 | descriptor regression RED → GREEN; shared [helper proof](auth-helpers.md) |
| Anthropic Messages lazy API | 1/13 | direct native factory mapping; API runtime separately [tracked](anthropic-messages.md) |

Removed old-only subscription OAuth registration. Frozen file exposes no such method; old-only OAuth module deletion tracked separately. GREEN 1 actual-import test, revision `020f8d48` + Kimi slice; `/tmp/pi-rust-repin-delta-ai/ledger.md`.

Factory metadata/catalog/auth-method parity proven. Shared env helper cancellation and request-model/auth interface gaps remain [explicit](auth-helpers.md); row partial until dependency parity. No source bugs identified.
