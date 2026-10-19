# llama provider traceability

Pinned source: `packages/coding-agent/src/extensions/llama/provider.ts` at
`532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-coding-agent/src/bundled_extensions/llama/provider.rs`.

Differential: `crates/pi-coding-agent/tests/differential_llama_provider.rs`
drives `oracle/llama-provider-oracle.ts` and compares the loaded-model catalog,
auth check/resolve, login credential/prompts/requests, and refresh
publications/models/requests in source order (dynamic ports and `checkedAt` are
substituted in place).

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `provider.ts:10-11` | Provider id `llama.cpp`, default server `http://127.0.0.1:8080` | `LLAMA_PROVIDER_ID`, `DEFAULT_LLAMA_SERVER_URL`; metadata test |
| `provider.ts:12-16` | Credential `env.LLAMA_BASE_URL` must be non-empty; normalize | `credential_server_url`; differential auth credential cases |
| `provider.ts:18-25` | Credential URL wins over `ctx.env("LLAMA_BASE_URL")`; empty env ignored | `resolve_server_url`; differential auth cases |
| `provider.ts:27-50` | `toPiModel`: context window from `n_ctx`/`n_ctx_train` (>0 else 128000), image modality, zero cost, openai-completions compat | `to_pi_model`; differential catalog + 2 behavioral catalog tests |
| `provider.ts:52-56` | Controller exposes provider plus `setCatalog` | `LlamaProviderController`; catalog tests |
| `provider.ts:58-64` | `setCatalog` keeps only `loaded` and maps models | `set_catalog`; differential catalog |
| `provider.ts:66-72` | Provider id/name/baseUrl/auth | `LlamaProvider`; metadata test |
| `provider.ts:73-119` | API-key login: URL + optional secret prompts, catalog probe, credential env | `LlamaApiKeyAuth::login`; differential login + behavioral login test |
| `provider.ts:120-128` | `check`: configured URL yields `{type, source}` | `ApiKeyAuth::check`; differential auth |
| `provider.ts:129-138` | `resolve`: key fallback `LLAMA_API_KEY` then `local`, baseUrl `/v1`, merged env | `ApiKeyAuth::resolve`; differential auth + behavioral resolve test |
| `provider.ts:139-174` | `refreshModels`: restore stored llama models, publish update gate, network/abort/api-key guards, catalog refresh, persist + update | `LlamaState::refresh`; differential 3 scenarios + 2 behavioral tests |
| `provider.ts:176-177` | Streams delegate to compat `stream`/`streamSimple` | `LlamaProvider::stream`/`stream_simple`; delegated to pi-ai compat (covered by pi-ai tests) |

## Gaps

- `os.homedir()`-style process environment reads in `login` use `std::env`; the
  placeholder and fallback are captured at login time, matching the source.
- Catalog entries that do not deserialize into `pi_ai::types::Model` are rejected
  with the serde message (typed-core exclusion); TypeScript keeps the object.

## Discrepancies

- `Model` serialization uses the shared `pi_ai::types` field order. The source's
  compat object literal places `supportsStrictMode` before `maxTokensField`; the
  shared type places `maxTokensField` first. Values are identical; only key order
  differs. The differential serializes in the source literal order explicitly.
- `AbortSignal.reason` is not representable; refresh cancellation uses the
  `CancellationToken` state only.

## Proof inventory

- Differential: 4 catalog models, 5 auth cases, login (credential/prompts/requests),
  3 refresh scenarios.
- Behavioral: 9 cases.
