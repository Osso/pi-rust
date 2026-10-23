# Native HTTP failure identity

Shared `CallbackError` in `crates/pi-ai/src/request.rs` carries native fetch failures from `crates/pi-ai/src/http.rs`. [Traceability](../porting/traceability/native-http-failure-identity.md) records source evidence and the bounded diagnostic deviation.

## What it must do

- [x] Keep `CallbackError::new(message)` as name `Error`, no cause, and message-only `Display`; expose native UTF-16 `name`/`message` and `cause: Option<String>`.
- [x] Report refused native connections as `TypeError` / `fetch failed`, preserving useful native connection diagnostics separately without the request URL or its credential/query secrets.
- [x] Resolve headers/status before reporting an incomplete response body as `TypeError` / `terminated`; preserve useful native EOF diagnostics separately.
- [x] Preserve supplied abort rejection values and Error/DOMException brands through fetch send and body reads. Body reads match Node's falsey-reason replacement with its body-specific AbortError. Never eagerly stringify arbitrary reasons.
- [x] Keep request-builder error identity; do not reclassify it as a network failure.
- [x] Preserve existing method/header/body and chunked response behavior.

## How it works

- [Native HTTP traceability](../porting/traceability/native-http-failure-identity.md).

## Implementation inventory

- `crates/pi-ai/src/request.rs`: shared error representation and legacy constructor/display.
- `crates/pi-ai/src/http.rs`: native send/body failure mapping and native cause diagnostics.
- `oracle/native-http-failure-oracle.ts`: bounded real Node fetch observation, with full own error properties, causes, and stderr retained.

## Tests asserting this spec

- `crates/pi-ai/tests/differential_http_failures.rs`: closed loopback and premature content-length body vs real Node fetch; name/message, response status, failure ordering/boundary, native causes and URL removal.
- `crates/pi-ai/tests/http.rs`: constructor/display, invalid method/URL/header, cancellation, successful HTTP request/response.

## Known gaps (current cycle)

- [ ] Parent integration must wire generator consumers to read error name/cause and independently verify the integrated revision.

## Out of scope

- Full WebIDL/Node HTTP stack; bounded send/body abort and native network failures are covered. Generic provider/SDK consumers of rejection values remain separately tracked.
- Untested DNS/TLS/proxy/redirect/compression diagnostic details; no exact diagnostic parity is claimed beyond the fixtures.
- Generator CLI streams/exit behavior and JavaScript stack presentation: owned by parent integration, not established by this shared transport slice.
- Other JavaScript callback error types: new fields do not approve unrelated cause-representation deviations.
