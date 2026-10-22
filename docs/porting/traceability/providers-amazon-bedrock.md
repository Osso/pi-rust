# Amazon Bedrock provider — common pin

Frozen `packages/ai/src/providers/amazon-bedrock.ts`, `4f9e6f8a4` (35 lines). Rust `providers/amazon_bedrock.rs`.

| Frozen symbol / branch | Lines | Proof |
|---|---|---|
| `bedrockAuth.name` / no login | 11–13 | `provider_auth::bedrock_provider_matches_frozen_module_without_login`: actual frozen descriptor oracle |
| stored truthy key → bearer apiKey, no credential env publication | 14 | `bedrock_resolve_detects_stored_and_ambient_credentials` |
| ambient bearer/profile/access-pair/ECS-relative/ECS-full/web-identity, priority order, none | 15–23 | `bedrock_resolve_detects_stored_and_ambient_credentials`; actual frozen auth oracle 5 scenarios |
| `amazonBedrockProvider`: id/name/catalog/API, no baseUrl | 27–35 | actual frozen descriptor including full ordered model IDs |

Deleted old-only interactive bearer/profile/chain login and its tests; added exact no-login descriptor assertion. Deleted credential-env profile merging and env publication. Stored keys now surface as `auth.apiKey` rather than empty auth, matching frozen line 14. JsString stays native.

RED 2 tests before production rewrite. GREEN 2 tests, including frozen descriptor + 5 actual-import auth cases, revision `4032dd49` + Bedrock slice; `/tmp/pi-rust-repin-delta-ai/ledger.md`, `bedrock-provider-{red,green}.log`.

No module-specific gaps or source bugs identified. API transport implementation tracked separately under [bedrock-converse-stream](bedrock-converse-stream.md).
