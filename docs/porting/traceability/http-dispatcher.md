# coding-agent http-dispatcher traceability

Pinned source: `packages/coding-agent/src/core/http-dispatcher.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential proof: request `27013` compares `parseHttpIdleTimeoutMs` across 25 values (choice labels, case and whitespace variants, empty strings, numeric strings, fractional values, negative, hex/binary/octal, infinities, `NaN`, malformed exponents, numbers) and `formatHttpIdleTimeoutMs` across ten timeout values. Behavioral proof: `crates/pi-coding-agent/tests/http_dispatcher.rs` runs the configured `Fetch` against a real TCP server that stalls mid-body.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `DEFAULT_HTTP_IDLE_TIMEOUT_MS` | `300_000` | `DEFAULT_HTTP_IDLE_TIMEOUT_MS` |
| `DEFAULT_AUTO_SELECT_FAMILY_ATTEMPT_TIMEOUT_MS` | `2_000` | `auto_select_family_attempt_timeout_ms`; not applied (see Gaps) |
| `HTTP_IDLE_TIMEOUT_CHOICES` | Five label/timeout pairs | `HTTP_IDLE_TIMEOUT_CHOICES` |
| `parseHttpIdleTimeoutMs` | Strings trimmed; `disabled` → 0; empty → undefined; numeric strings re-parse as numbers; numbers must be finite and non-negative; floor | `parse_http_idle_timeout_ms` + `parse_javascript_number`; focused and differential `27013` |
| `formatHttpIdleTimeoutMs` | Known choice label, else `<ms/1000> sec` | `format_http_idle_timeout_ms`; `formats_known_choices_and_derived_seconds`, differential `27013` |
| `applyHttpProxySettings` | Blank ignored; `HTTP_PROXY`/`HTTPS_PROXY` filled only when unset | `resolve_http_proxy_settings` (pure; the caller applies the pairs); `applies_proxy_settings_only_to_unset_variables` |
| `configureHttpDispatcher` | Invalid timeout → `Invalid HTTP idle timeout: <value>`; installs HTTP/1-only, env-proxy, idle-timeout dispatcher | `configure_http_dispatcher` returns `Arc<dyn Fetch>` built by `pi_ai::http::fetch_with_options` (`http1_only`, `read_timeout`); `rejects_invalid_dispatcher_timeouts_with_the_source_message`, `idle_timeout_aborts_a_stalled_response_body`, `disabled_timeout_keeps_waiting_for_the_body` |
| `withUndiciErrorListener` | Swallows dispatcher-level `error` events so a mid-stream abort cannot crash the process | N/A: `reqwest` surfaces body errors through the stream only; there is no EventEmitter `error` special case |
| `undici.install()` | Keeps `fetch` and the dispatcher on one undici build | N/A: one HTTP stack (`reqwest`) backs every `Fetch` |

## Approved deviation (user decision 2026-10-18)

`configureHttpDispatcher` mutated a process-global dispatcher. The port returns a configured `Fetch` that callers inject; no global is mutated.

## Gaps

- `autoSelectFamilyAttemptTimeout: 2000` has no `reqwest` 0.13 equivalent; connection attempts keep the HTTP stack's default.
- Node's `EnvHttpProxyAgent` reads `HTTP_PROXY`/`HTTPS_PROXY`/`NO_PROXY`; `reqwest` does the same for its default client, so no explicit wiring is needed.
- Typed-representation boundary: JavaScript numbers above `u64::MAX` saturate to `u64::MAX` (documented assertion in differential `27013`).

## Discrepancies

None.
