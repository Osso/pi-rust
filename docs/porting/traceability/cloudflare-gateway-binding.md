# cloudflare-gateway-binding.ts

Source: `packages/ai/src/api/cloudflare-gateway-binding.ts` (pinned revision).
Rust: `crates/pi-ai/src/api/cloudflare_gateway_binding.rs`.
Tests: `crates/pi-ai/tests/cloudflare_gateway_binding.rs`,
`crates/pi-ai/tests/differential_cloudflare_gateway_binding.rs`.

## Exports

| TypeScript | Rust | Tests |
| --- | --- | --- |
| `AiGatewayBinding` | `AiGatewayBinding` trait | fake binding in both test files |
| `AiGatewayBindingGateway` | `AiGatewayBindingGateway` trait (`run`) | fake gateway in both test files |
| `AiGatewayUniversalRequestLike` | `AiGatewayUniversalRequest` | provider/endpoint, query, header tests |
| `CLOUDFLARE_GATEWAY_BINDING_AUTH_SENTINEL` | `CLOUDFLARE_GATEWAY_BINDING_AUTH_SENTINEL` | header-stripping test, differential scenario 2 |
| `GatewayBindingFetchOptions` | `GatewayBindingFetchOptions` | construction in both test files |
| `createGatewayBindingFetch` | `create_gateway_binding_fetch` returning a `Fetch` impl | all behavioral and differential scenarios |

## Branches

| Source branch | Rust | Tests |
| --- | --- | --- |
| base URL parse + prefix path normalization | `create_gateway_binding_fetch` | differential scenarios 0/10 |
| URL parse failure → outside error | `Fetch::fetch` outside check | `rejects_unparseable_urls`, differential scenario 4 |
| origin/path prefix check | `Fetch::fetch` outside check | `rejects_urls_outside_the_gateway_prefix`, differential scenario 4 |
| non-POST | `unexpressible("only POST is supported")` | rejection test, differential scenario 5 |
| missing provider/endpoint path | `unexpressible("missing provider/endpoint path")` | rejection test, differential scenario 5 |
| endpoint keeps query string | endpoint construction | query test, differential scenario 1 |
| missing body | `unexpressible("missing body")` | rejection test, differential scenario 5 |
| non-JSON body | `unexpressible("non-JSON body")` | rejection test, differential scenario 5 |
| header lowercasing, duplicate collapse, strip set | `collect_headers` | header tests, differential scenarios 2/8 |
| signal forwarding | signal passthrough | signal test, differential scenario 6 |
| binding response passthrough | returns `run` result unchanged | response test, differential scenario 7 |
| URL-normalized path split (dot segments, fragment) | `url::Url` parsing | normalization test, differential scenarios 3/9 |

## Differential normalizations

- object key order sorted (TypeScript object iteration order).
- integral numbers canonicalized to integers.

## Gaps / discrepancies

- The Rust `Fetch` contract carries one `HttpRequest`; the TypeScript `Request | URL`
  input plus `RequestInit` split (including `signal: null` clearing a `Request`
  signal and `body: null` clearing a `Request` body) has no Rust equivalent.
- Non-string init bodies (`URLSearchParams`, `FormData`, `Blob`, `ReadableStream`)
  are outside the typed API; `HttpRequest::body` is bytes.
- Body bytes are decoded with UTF-8 replacement, matching `TextDecoder` behavior for
  valid UTF-8; lone invalid sequences follow Rust's replacement rules.
- An invalid `baseUrl` returns `Result::Err` instead of throwing from the factory.
