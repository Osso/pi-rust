# xAI provider — common pin

Frozen `packages/ai/src/providers/xai.ts`, `4f9e6f8a4`, lines 1–16, compared whole to `providers/xai.rs`.

| Export / branches | Frozen lines | Proof |
|---|---|---|
| `xaiProvider`, identity/baseURL/catalog | 6–15 | `provider_auth::xai_provider_matches_frozen_without_subscription_oauth`, actual-import complete descriptor/catalog oracle |
| envApiKeyAuth XAI_API_KEY, no OAuth | 11 | descriptor RED→GREEN; shared [helper proof](auth-helpers.md) |
| single OpenAI Completions API, no Responses map | 1/13 | exact native factory mapping; API behavior tracked separately |

Deleted old-only subscription OAuth and OpenAI Responses dispatch registration. Existing provider metadata fixtures for completed Bedrock/Anthropic/Kimi/xAI slices reconciled to cited frozen factories; remaining old-only metadata inventory is explicitly not parity proof.

GREEN 1 actual frozen descriptor test (`fe21f1be` + slice), `/tmp/pi-rust-repin-delta-ai/ledger.md`. Shared auth-helper cancellation/model-input gaps remain; row partial, no new deviation approved. No source bugs identified.
