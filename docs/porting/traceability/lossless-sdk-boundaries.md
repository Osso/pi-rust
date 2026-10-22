# Q18 SDK/auth boundary proof

Parent-authorized request consumers retain JsString endpoints, auth answers and header values until the actual HTTP/WS boundary. Shared request_url applies USVString/WHATWG URL semantics; header_value applies ByteString rejection. This is bounded integration proof, not all-provider or coding-agent acceptance.

| Capability | Concrete behavior | Proof |
| --- | --- | --- |
| OpenAI completions/responses, Azure, Mistral auth headers | lone high unit in API key produces the original unit-number ByteString error and fetch is never called | lossless_sdk_boundaries::unmatched_auth_header_is_rejected_before_fetch |
| Same four provider endpoints | lone high unit in URL path reaches HttpRequest as percent-encoded U+FFFD at URL conversion | lossless_sdk_boundaries::unmatched_endpoint_units_convert_only_at_request_url_boundary |
| Vertex service-account login | project, location and credential-file path retain distinct unmatched units in the returned ProviderEnv | lossless_sdk_boundaries::vertex_login_retains_raw_project_location_and_credential_path |
| Stored Radius gateway config | raw endpoint survives JsValue credential ingestion; shallow-invalid records filter; shallow-valid scalar ID decoding failure returns contextual error | lossless_sdk_boundaries::stored_radius_config_keeps_endpoint_and_rejects_invalid_scalar_model_id |

`cargo test -j4 -p pi-ai --test lossless_sdk_boundaries` at `190c3729`: **4/4 mandatory sandbox PASS**, artifact `/tmp/pi-rust-repin-lossless-interactive/sdk-boundaries-final-green.log`. The first two cases previously passed at44a5d91d; moving new private tests to this public target exposed a borrow-only compiler error, corrected before this run. No production change to these pi-ai modules occurred after the covered revision.

Whole-library internal unit fixtures and existing provider regression suites remain pending migration/execution. Coding-agent selector tests are authored but unexecuted under the lane's compiler-blocked instruction. Main owns independent review, workspace checks and acceptance.
