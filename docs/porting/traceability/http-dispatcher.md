# coding-agent http-dispatcher traceability

Whole-module audit: `packages/coding-agent/src/core/http-dispatcher.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`.

| Frozen symbol / branch | Native mapping and proof |
| --- | --- |
| `DEFAULT_HTTP_IDLE_TIMEOUT_MS`, five `HTTP_IDLE_TIMEOUT_CHOICES` | Same constants; `http_dispatcher::exposes_the_source_constants`. |
| `parseHttpIdleTimeoutMs`: trimmed case-insensitive disabled, empty, JS Number, finite/nonnegative/floor | `parse_http_idle_timeout_ms`; string/numeric/JS-number tests plus frozen `differential_core_leaves::http_dispatcher_parsing_and_formatting_match_typescript`. |
| `formatHttpIdleTimeoutMs`: choice or divided seconds | `format_http_idle_timeout_ms`; choice/derived/frozen numeric-format tests. |
| `applyHttpProxySettings`: blank ignored, nullish assignment preserves empty environment variables | Pure `resolve_http_proxy_settings`; caller applies values. Unset/preserved/partial/blank native tests. |
| `configureHttpDispatcher`: normalize and reject invalid values | Configured `Fetch`; invalid-number tests preserve Node number text. |
| `allowH2:false`, body/header timeout, environment proxies | Shared HTTP/1-only `FetchClientOptions`, read idle timeout; real TCP stalled-body and disabled-timeout tests. |
| `withUndiciErrorListener`, `createUndiciClient`, `createUndiciOriginDispatcher` | Native stream errors return through Fetch, without EventEmitter special-case process failure; stalled-body test. No separate JS Client/Pool objects exist. |
| `originalGlobalFetch`, `installedGlobalFetch`, conditional `undici.install` | Approved configured-Fetch injection instead of global mutation; deliberate caller Fetch overrides stay separate. One reqwest stack handles decoding. |

Current-pin proof at 567abb2c7: all ten native HTTP tests GREEN in `suite_core_3` (`/tmp/runtime-services-green-http.log`); frozen parse/format differential GREEN in `suite_core_2` at ccb73a6d2 (`/tmp/runtime-services-green-leaves.log`). No parser/formatter behavior changed afterward. Removed the unused old-only autoSelectFamilyAttemptTimeout accessor/constant: neither exists in the common pin; no callers.

Approved deviation retained: configure returns an injected `Arc<dyn Fetch>`, not a global dispatcher mutation (2026-10-18). Gap: pathological input types, numeric extremes and Node/Undici scheduling/fault details. Native `u64` timeout representation excludes oversized values; normal timeout inputs remain exact. No live-provider proof or full HTTP implementation acceptance claimed by this module audit.
