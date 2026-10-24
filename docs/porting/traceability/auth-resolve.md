# auth/resolve.ts common-pin rewrite

Frozen `packages/ai/src/auth/resolve.ts:1–145` at `4f9e6f8a4`; native `crates/pi-ai/src/auth/resolve.rs`. Implementation replaced; direct matrix covers the module's branches within the typed environment-name exclusion below. Shared auth/types and image-collection implementation remain separate partial modules. [Contract](../../specs/auth-resolution.md).

| Frozen surface | Native behavior | Proof level |
|---|---|---|
| ModelsErrorCode/ModelsError (:14–29) | Exact message, separate shared reason cause, native source and branded callback | Existing actual-source constructor five-case matrix and Models contract error matrix |
| Overrides and resolveProviderAuth (:16–19,39–68) | Only api_key/env; actual chat/image model; explicit key bypasses store only with matching handler; stored credential owns provider | auth16/models regressions, actual-source runtime/model-sensitive chat/image differentials, and 12 direct dispatch/env cases covering both handlers, missing handlers, explicit/empty/ignored keys, stored type ownership and no ambient fallback |
| overlayEnvAuthContext (:70–75) | Nonempty override wins, empty override delegates to base; fileExists delegates unchanged; stored env spread preserves keys | 12 actual-source dispatch/env cases: raw high/low UTF-16 values, empty override/base lookup, empty env map versus omission, merged credential env, numeric-key JSON order, exact model name/base URL and fileExists argument forwarding; original stored state stays unchanged |
| resolveStoredOAuth (:84–118) | Expiry only; authoritative locked recheck; logout/type-change returns undefined; persist rotated credential; no deadline or cancellation override | `auth_resolve_contract`: real frozen import, full result/store/read/modify/refresh observations for valid short-lived and expired credentials; actual-source authoritative refresh/logout/API-key type-change matrix; forced simultaneous expired reads through real store locks yield two modifies but exactly one refresh; real 16-second callback exceeds the retired 15-second deadline, persists and resolves without timing emulation |
| resolveApiKey/readCredential (:120–145) | Every failure wraps in ModelsError with exact source message and original cause; only modify catch preserves an existing ModelsError | Existing auth/models wrapping regressions and Models raw-false differential; actual-source 50-case matrix: read/modify/refresh/derivation/API-key catches × false/null/raw UTF-16/object/native TypeError/existing ModelsError/name-spoof/nested ModelsError/default DOM abort/supplied-false abort; exact name/code/message/nested cause/store state/call counts |

## Typed environment-name exclusion

The [workspace environment rule](../../specs/lossless-content.md#what-it-must-do), reaffirmed by parent on 2026-10-23, keeps environment names as String and values as JsString. ProviderEnv is IndexMap<String,JsString> (`types.rs:199`); AuthContext.env accepts &str. Lone-surrogate environment keys are explicitly outside this typed input domain. This exclusion does not cover env values: they must retain exact UTF-16. No shared key-type expansion or lossy value conversion is introduced.

## Approved image caller boundary

Frozen `images-models.ts:205–213`, specifically line 212, reads `error.message` for Error instances. Auth resolution returns a native ModelsError, so generation copies its exact JsString message, not Display/to_string. `image_auth_failure_reads_the_exact_native_models_error_message` drives expired credential → store modify throws existing ModelsError → actual image collection catch, and compares the full result to the frozen caller with only separately bounded timestamps omitted. Raw high/low surrogate message RED (`q15b-image-auth-utf16-red-3.log`): former Display projected both to U+FFFD. Provider generation must remain uncalled. This targeted fix is explicitly approved; other image-module branches remain outside scope.

## RED and removal inventory

`q15b-auth-resolve-expiry-red.log`: native refreshed a token with 60 seconds remaining; frozen returned the unchanged token without modify/refresh. Expiry-only rewrite GREEN (`q15b-auth-resolve-expiry-green-1.log`). No test weakens frozen comparisons.

Deleted source-absent ResolveError, provider-status resolver, minimum-validity/deadline constants/options, timeout tasks and abort races. Public resolver and collection consumers return ModelsError directly. Image collection get_auth takes only the actual model; provider-ID and get_auth_for_model aliases are deleted. Other images-models implementation is outside this slice.

Retired old-only `auth::oauth_refresh_aborts_without_writing` (resolver cancellation override absent frozen). `models::valid_oauth_tokens_do_not_touch_modify_and_short_lived_tokens_refresh` is replaced by `unexpired_oauth_tokens_never_lock_or_refresh_even_when_short_lived`: assert unchanged short token and zero locks/refreshes, matching the actual-source RED. Image tests retire duplicate provider-ID getter and getter-only override assertions; actual-model getter and explicit generation-option assertions remain. Lossless ResolveError::Callback construction now tests the exported ModelsError native message. Removed enum destructuring is type migration, not removed error assertions. Image refresh assertions now check exact message and separate original cause instead of the obsolete concatenated message.

Related auth/type and login modules remain partial: [auth types](auth-types.md), [lossless auth](lossless-auth.md). Proof ledger: `/tmp/pi-rust-repin-delta-ai/q15-proof-ledger.md`.
