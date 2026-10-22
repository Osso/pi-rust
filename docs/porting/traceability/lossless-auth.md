# Q18 native auth integration

Scope: native login answers, auth payload text, credentials, configured keys/env/headers/base URLs, and leaf login/OAuth presentation.

| Capability | Status | Proof |
|---|---|---|
| ApiKey/OAuth credential JSON retaining unmatched units | implemented | `lossless_auth_interaction::credential_round_trip_preserves_unmatched_key_and_oauth_fields`; old serde behavioral RED recorded; 45b00bd2 focused sandbox PASS |
| Native prompt answer to stored key / Cloudflare env | implemented | actual login callback tests PASS at 45b00bd2 |
| Native manual code parsing and to_auth handoff | implemented | parser/form and to_auth tests PASS at 45b00bd2 |
| Node ByteString/URL request boundary | helper implemented; main owns routing | exact high/low/Euro/Latin-1 and URL tests PASS at 45b00bd2; Node probe confirmed expected strings/bytes |
| Native coding UI/config/composition/storage | integrated, acceptance pending | `raw_error_consumer_components` tests unexecuted |

Frozen reference: source lock in `../source-lock.json`. These checkpoints do not prove complete auth-module parity or end-to-end login. Source Copilot policy-enable catches request construction/fetch failures (`packages/ai/src/utils/oauth/github-copilot.ts:307-321`); native construction failures follow that same catch branch. No scalar token conversion is used for presentation or to_auth.

Remaining pre-existing boundary: some OAuth device/token HTTP responses still use scalar `OAuthHttpResponse.json` / `serde_json::Value`; migrating prompt answers does not establish lossless response ingress. No approval claimed for that gap.

JWT decoding now passes the native token segment to atob and parses its byte-string units without a whole-token UTF-8 gate. Source `utils/oauth/openai-codex.ts:112-121` explicitly catches atob/JSON.parse errors and returns null. Pre-Q18 Rust used UTF-8 decoding of atob bytes; native propagation now follows source Latin-1 byte-string representation, not an arbitrary UTF-8 projection.

Legacy OAuthLoginCallbacks prompt/manual/select methods now return explicit `Result` so Promise rejection is representable. Registry and provider-composer bridge no longer substitute empty strings or absent choices. `lossless_oauth_callbacks` compiler RED established missing Result API on the prior green pi-ai library; three actual provider login paths assert the same native message/name/code on rejection. Runtime proof: `lossless_oauth_callbacks` 3/3 PASS at 45b00bd2, preserving raw message/name/code across actual GitHub prompt, Anthropic manual and Codex select login paths.

## Focused proof ledger

- `cargo test -j4 -p pi-ai --test lossless_auth_interaction --test lossless_oauth_callbacks`, 45b00bd2: mandatory sandbox, **9+3 PASS**, zero warnings; `/tmp/pi-rust-repin-lossless-leaf/auth-native-green-2.log`. Does not prove OAuth response ingress or coding-agent composition.
- `cargo test -j4 -p pi-ai --test lossless_auth_interaction resolve_error_exposes_native_message_without_display_projection`, 810b198d: mandatory sandbox, **1 PASS**, zero warnings; `/tmp/pi-rust-repin-lossless-leaf/resolve-message-green.log`. Display unchanged; native accessor preserves unmatched units.
- All coding-agent leaf/config/storage/CLI tests remain explicitly unexecuted until shared compilation is available. No clippy, workspace acceptance or independent verification claim.
