# Shared native HTTP failure identity

Contract: [spec](../../specs/native-http-failure-identity.md). Source operation is real Node native `fetch`, called by pinned generator source; shared Rust boundary is `request::CallbackError` / `http::default_fetch`. This is a bounded prerequisite, not a new JavaScript error framework.

## Exact approval (2026-10-20)

Question: “For nativeHTTPfailures mayRust preserve top-levelerrorname/message butuse nativecausediagnostics insteadofreproducingNode nestederrno/address/causeobjects? Failureordering/exitbehaviorstillmatch.”

Answer: “Allow native causes (Recommended)”.

Approval permits native cause diagnostics for native HTTP failures only. It does not waive top-level name/message, failure ordering, exit behavior, or JavaScript frames outside the separately approved generator CLI. It does not approve generalized transport causes for unrelated callback types.

## Observed discrepancy and RED

`/tmp/pi-rust-image-generator-proof/closed-loopback-source.json` records source exit 1, initial stdout, then two stderr events containing `TypeError: fetch failed` with nested `ECONNREFUSED`. The corresponding `closed-loopback-native-stderr.txt` instead contained `Error: error sending request for url (...)`. The old shared HTTP adapter discarded the reqwest chain and the shared callback error carried only a message.

Before production changes, two real-loopback differential tests failed at the public error-message assertion:

| Failure boundary | Actual Node | Old native | Proof |
|---|---|---|---|
| Closed ephemeral loopback port, send rejects before response | `TypeError` / `fetch failed`, `ECONNREFUSED` cause | URL-bearing `error sending request for url (...)` | `refused_connection_matches_native_fetch_failure_identity` |
| Response status 200 and bytes `abc`, declared content-length 10, then EOF | `TypeError` / `terminated`, content-length mismatch cause | `error decoding response body` | `premature_body_matches_native_fetch_failure_identity_after_headers` |

Full raw own Node error properties/stacks/causes and stderr, native errors, commands, and results live in `/tmp/pi-rust-http-failure-proof/`; the initial oracle-entry-point error is classified separately from behavioral RED in its ledger.

## Symbols and branches → tests

| Rust symbol/branch | Contract | Proof |
|---|---|---|
| `CallbackError::{message,name,cause}`, `new`, `Display`, clone/equality | New fields, constructor `Error`/no cause, message-only display | `callback_error_constructor_and_display_keep_the_legacy_contract`; differential errors assert display and fields |
| `to_send_error`, reqwest request failure | `TypeError` / `fetch failed`; native cause includes actual OS refusal | Closed-port differential test |
| `to_body_error`, body/decode failure | `TypeError` / `terminated`; native cause describes premature EOF | Premature-body differential test; headers/status and received bytes asserted before error |
| `native_fetch_error` | Strip reqwest request URL; retain source chain, not fabricated Node errno/address | Both differential cases; closed-port native request additionally includes user/password, path/query secrets |
| `build_request`, non-network errors | Preserve legacy `Error`/no cause, no network-name rewrite | `request_builder_errors_are_not_reclassified_as_network_failures` (invalid method, URL, header) |
| `until_aborted` | Preserve legacy abort message/name/no cause | `cancelled_signal_aborts_pending_fetch` |
| Successful fetch/body | Preserve request/response behavior | `default_fetch_sends_method_headers_body_and_streams_response` |

`ReqwestBody` normalizes body/decode failures only; send normalizes reqwest request failures only. Other adapter branches retain the legacy constructor. Fake callback constructors and callback source flow are unchanged. No additional struct-literal migrations were necessary.

## Fixture limits and remaining proof

- Only loopback addresses are used. Each native fetch/body operation and server connection has a four-second bound; Node fetch has a two-second signal, oracle process a five-second deadline, shared parent watchdog and process-group teardown. Fixture tasks abort on drop and the successful body test joins its task.
- Node source reads the same response bytes and EOF as native. Assertions compare top-level name/message, status, and send-vs-body event ordering; own cause objects and stderr are retained without filtering. Native cause is asserted meaningful, not compared with Node cause objects.
- No DNS/TLS/proxy/redirect/compression error-detail equivalence claimed. Generic reqwest source-chain diagnostics beyond these observed network cases remain unproven.
- Shared tests do not establish generator CLI output/exit/stack parity. Consumer wiring and independent final gates belong to parent integration.
