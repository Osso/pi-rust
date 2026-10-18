# node-http-proxy traceability

Source: `packages/ai/src/utils/node-http-proxy.ts`.
Rust: `crates/pi-ai/src/utils/node_http_proxy.rs`.
Tests: `crates/pi-ai/tests/node_http_proxy.rs` (16 behavioral, including three real local-socket `Fetch` tests), `crates/pi-ai/tests/differential_node_http_proxy.rs` (2 tests; 64 target/env combinations vs pinned TS).

## Representation
- `resolveHttpProxyUrlForTarget(targetUrl, env?)` → `resolve_http_proxy_url_for_target(&str, Option<&ProviderEnv>) -> Result<Option<Url>, ProxyError>`. The TypeScript `URL` input overload is not part of the typed Rust API; callers pass the string form.
- Testable form `resolve_http_proxy_url_for_target_with(target, env, process_env)` injects the process environment (Rust tests cannot mutate process-global env under `unsafe_code = "forbid"`).
- `URL.toString()` is reproduced with `url::Url::to_string`.
- `UNSUPPORTED_PROXY_PROTOCOL_MESSAGE` is a pinned `&str`.
- Errors: `ProxyError::InvalidProxyUrl { proxy, message }` renders `Invalid proxy URL "<proxy>": <message>`; `UnsupportedProxyProtocol { protocol }` renders the pinned message plus `Got <scheme>:`.
- `proxy_fetch(env)` is the `Fetch` wrapper required by the slice: it selects a proxy per request URL and sends through a reqwest client configured with `reqwest::Proxy::all`. Requests without a proxy use the shared default client.

## Symbols / branches → proof
| TS | Rust | Test |
|---|---|---|
| `DEFAULT_PROXY_PORTS` | `default_proxy_port` | `no_proxy_port_matching_uses_protocol_defaults`, differential (ws/wss/ftp/gopher) |
| `getProxyEnv` order: scoped lower → scoped upper → process lower → process upper, `\|\|` truthiness | `get_proxy_env` | `scoped_proxy_aliases_win_over_process_aliases`, `lowercase_scoped_proxy_wins_over_uppercase_scoped_proxy`, `scoped_empty_values_fall_through_to_process_values`, differential |
| invalid target URL / no protocol or host → `undefined` | `Url::parse().ok()?` + `host()?` | `non_http_targets_are_not_proxied`, differential (`not a url`, `mailto:`) |
| `host.replace(/:\d*$/,"")` | `strip_port_suffix` (unit test) | `strip_port_suffix_matches_the_typescript_regex`, differential (ports, IPv6) |
| `parseInt(port) \|\| DEFAULT_PROXY_PORTS[protocol] \|\| 0` | `parsed.port().or_else(default)` | `no_proxy_port_matching_uses_protocol_defaults`, differential |
| `NO_PROXY` empty → proxy | `should_proxy_hostname` first branch | `resolves_https_proxy_from_process_environment`, differential |
| `NO_PROXY === "*"` → no proxy | second branch | `no_proxy_supports_wildcards_domains_ports_and_separators`, differential |
| split `/[,\s]/`, empty entries ignored | `split(',' or whitespace)` | `no_proxy_supports_wildcards_domains_ports_and_separators`, differential |
| `^(.+):(\d+)$` proxy host/port | `rsplit_once` digit check | `no_proxy_supports_wildcards_domains_ports_and_separators`, `no_proxy_port_matching_uses_protocol_defaults`, differential |
| port mismatch keeps proxying | `proxy_matches` port branch | same tests |
| exact hostname match for non-`.`/`*` entries | `proxy_matches` exact branch | same tests |
| `*` prefix strip + `endsWith` | `proxy_matches` suffix branch | same tests |
| `${protocol}_proxy` then `all_proxy` | `get_proxy_for_url` | `all_proxy_is_the_fallback`, differential |
| scheme-less proxy gets `${protocol}://` | `get_proxy_for_url` | `proxy_without_scheme_gets_the_target_protocol`, differential |
| invalid proxy URL throws | `Url::parse` error mapping | `invalid_proxy_urls_are_reported_with_the_quoted_value` |
| non-http(s) proxy protocol throws | scheme check | `socks_and_pac_proxies_are_rejected`, differential (SOCKS5) |
| `Fetch` proxy application | `ProxyFetch` | `proxy_fetch_routes_through_the_selected_proxy`, `proxy_fetch_honors_no_proxy_exclusions`, `proxy_fetch_reports_invalid_proxy_configuration` |

## Gaps
- The invalid-proxy-URL error detail comes from the `url` crate, not Node's WHATWG `URL` parser, so the suffix after `Invalid proxy URL "<value>":` is not byte-identical. The prefix, quoted value, and throw behavior are tested; invalid proxy URLs are excluded from the differential.
- `new URL(targetUrl)` accepts a `URL` instance; the Rust API is string-only (typed-API exclusion).

## Discrepancies
- JavaScript `\s` in the `NO_PROXY` split also matches Unicode space separators; the Rust split uses `char::is_whitespace`, which covers the same hostname-relevant separators. No differential case distinguishes them.
- `parse::<f64>` for the `host:port` suffix is equivalent to `Number.parseInt(suffix, 10)`: the same guard requires a non-empty all-ASCII-digit suffix (the TS regex `^(.*):(\d+)$`), and both overflow to infinity for absurdly long digit strings.
