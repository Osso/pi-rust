# llama huggingface traceability

Pinned source: `packages/coding-agent/src/extensions/llama/huggingface.ts` at
`532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-coding-agent/src/bundled_extensions/llama/huggingface.rs`.

Differential: `crates/pi-coding-agent/tests/differential_llama_huggingface.rs`
drives `oracle/llama-huggingface-oracle.ts` against a local Node HTTP server and
compares token discovery, search requests/results, details payloads, and error
messages in source order. The request comparison keeps the client-controlled
`authorization` header only; transport headers are not module output. No
external service is contacted.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `huggingface.ts:5-9` | Default base URL, quantization and shard patterns | constants; differential details cases |
| `huggingface.ts:11-30` | `HuggingFaceModel`, quantization, details, gated union (`false`/`auto`/`manual`) | `HuggingFaceModel`, `HuggingFaceQuantization`, `HuggingFaceModelDetails`, `HuggingFaceGated`; differential details JSON |
| `huggingface.ts:32-37` | `payloadError`: non-empty string `error` else fallback | `payload_error`; differential 401/500 cases |
| `huggingface.ts:39-42` | `/(?:^|;)t=(\d+)/` on `ratelimit` | `parse_rate_limit_delay`; differential 429 ratelimit case |
| `huggingface.ts:44-51` | Read token file, trim, empty → absent | `read_token`; token differential/behavioral cases |
| `huggingface.ts:53-72` | `HF_TOKEN`, `HF_TOKEN_PATH`, `HF_HOME/token`, `XDG_CACHE_HOME/huggingface/token`, `~/.cache/huggingface/token`; dedupe, first hit | `find_hugging_face_token`; token differential (env/path/HF_HOME/missing) + home-cache behavioral case |
| `huggingface.ts:74-88` | Trim trailing slashes; bearer auth; 15s timeout | `HuggingFaceClient`; differential authorization header + request paths |
| `huggingface.ts:90-113` | 429 delay from `retry-after` (`Number`) or `ratelimit`, messages with/without delay | `HuggingFaceClient::request`; differential 429 cases |
| `huggingface.ts:115-130` | Search query order/encoding, array check, id filter, downloads default 0 | `search`; differential 7 cases + behavioral order test |
| `huggingface.ts:132-157` | `encodeURIComponent` per id segment, siblings `.gguf` filter, mmproj skip, shard strip, quantization capture/uppercase, size aggregation, Q4_K_M first then size/name, gated mapping, id fallback | `details`; differential 6 cases + 3 behavioral tests |

## Gaps

- `os.homedir()` is injected as the `home` argument of
  `find_hugging_face_token(env, home)`; `find_hugging_face_token_from_process`
  supplies `HOME`. Node's `getpwuid` fallback when `HOME` is unset is not
  reproduced.
- Typed deserialization excludes catalog fields with mismatched JSON types
  (typed-core exclusion); the source only checks the top-level shape.

## Discrepancies

- The source's `details` shape check is `typeof payload !== "object" || payload === null`,
  which accepts arrays. Rust mirrors that by accepting objects and arrays; `null`
  and scalars produce `Hugging Face returned invalid model details`.
- `Array.prototype.sort` comparator returns a number; Rust returns `Ordering`
  with the same Q4_K_M / size / `localeCompare` precedence.

## Proof inventory

- Differential: 4 token cases, 7 search cases, 6 details cases.
- Behavioral: 8 cases.
