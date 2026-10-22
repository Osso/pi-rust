# Q18 native auth integration

Scope: native login answers, auth payload text, credentials, configured keys/env/headers/base URLs, and leaf login/OAuth presentation.

| Capability | Status | Proof |
|---|---|---|
| ApiKey/OAuth credential JSON retaining unmatched units | implemented | `lossless_auth_interaction::credential_round_trip_preserves_unmatched_key_and_oauth_fields`; old serde behavioral RED recorded, GREEN pending |
| Native prompt answer to stored key / Cloudflare env | implemented | actual login callback tests, execution pending shared library integration |
| Native manual code parsing and to_auth handoff | implemented | parser/form and to_auth behavioral tests written |
| Node ByteString/URL request boundary | helper implemented; main owns routing | exact high/low/Euro/Latin-1 and URL test targets written; actual Node probe confirmed expected strings/bytes |
| Native coding UI/config/composition/storage | integrated, compiler-blocked | `raw_error_consumer_components` tests unexecuted |

Frozen reference: source lock in `../source-lock.json`. These checkpoints do not prove complete auth-module parity or end-to-end login. Source Copilot policy-enable catches request construction/fetch failures (`packages/ai/src/utils/oauth/github-copilot.ts:307-321`); native construction failures follow that same catch branch. No scalar token conversion is used for presentation or to_auth.

Remaining pre-existing boundary: some OAuth device/token HTTP responses still use scalar `OAuthHttpResponse.json` / `serde_json::Value`; migrating prompt answers does not establish lossless response ingress. No approval claimed for that gap.
