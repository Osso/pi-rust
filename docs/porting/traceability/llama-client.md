# llama client traceability

Pinned source: `packages/coding-agent/src/extensions/llama/client.ts` at
`532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-coding-agent/src/bundled_extensions/llama/client.rs`.

Differential: `crates/pi-coding-agent/tests/differential_llama_client.rs` drives
`oracle/llama-client-oracle.ts` (real module under `tsx`) and compares leaf
outputs, request bytes (method, path, content-type, authorization, body), list
results, and catalog error messages in source order. The request comparison
keeps only headers the module sets; Node's transport adds accept/user-agent/
connection headers that are not module output. An absent request body is
normalized to `null` on both sides. SSE timing paths are proven behaviorally.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `client.ts:1-44` | Model/status/event/progress wire shapes (`exit_code`, `n_ctx`, `input_modalities` stay snake_case) | `LlamaModelInfo` family; differential list result + behavioral router-metadata test |
| `client.ts:46-53` | `errorMessage` reads `error.message`, falls back when absent/empty | `error_message`; differential `boom` + 503 fallback cases |
| `client.ts:55-60` | `isModelInfo` accepts objects with string `id` and `status.value` | `is_model_info`; differential router-mode case |
| `client.ts:62-72` | Link caller signal to watcher; already-aborted aborts immediately | child `CancellationToken` + `WatcherGuard`; cancellation tests |
| `client.ts:74-84` | Abortable sleep rejects with the abort reason or `Error("Cancelled")` | `sleep`; cancelled poll-loop test |
| `client.ts:86-106` | `parseLoadProgress`: stage/current, string stages, clamped value, stage-ratio weighting, `Loading <stage>` | `parse_load_progress`; behavioral SSE load test (`Loading text model`) |
| `client.ts:108-126` | `parseDownloadProgress`: nested-or-flat entries, sums, `total <= 0` skip, byte detail | `parse_download_progress` / `download_progress_from_counts`; behavioral download test (`512 B / 1.00 KiB`) |
| `client.ts:128-142` | `formatBytes` with 1024 units and 1/2 decimals | `format_bytes`; differential 10 cases |
| `client.ts:144-158` | URL normalization: trim, http(s) only, drop hash/query, strip trailing slashes and `/v1`, keep `/` | `normalize_llama_server_url`; differential 15 cases |
| `client.ts:160-162` | `llamaInferenceUrl` appends `/v1` | `llama_inference_url`; differential 2 cases |
| `client.ts:164-186` | 15s timeout, JSON body decode with empty `catch`, auth/content-type headers, HTTP error text | `request_json` + `LlamaClient::request`; differential request bytes and error cases |
| `client.ts:188-199` | `list`: optional `?reload=1`, catalog shape check, router-mode check | `LlamaClient::list`; differential list/reload + catalog errors |
| `client.ts:201-207` | `load`/`unload` POST `{model}` | `load`/`unload`; differential bodies |
| `client.ts:209-215` | `unloadAndWait` polls until absent or `unloaded` | `unload_and_wait`; behavioral polling test |
| `client.ts:217-219` | `download` POST `/models` | `download`; differential body |
| `client.ts:221-247` | SSE parse: streaming UTF-8, CRLF folding per chunk, `data:` join, malformed events ignored | `watch` + `StreamingTextDecoder`; SSE load/download tests |
| `client.ts:249-292` | `loadAndWait`: watcher, loaded/failed/exit-code handling, synthetic loaded entry, 250ms poll | `load_and_wait`; behavioral load/exit-code/cancel tests |
| `client.ts:294-332` | `downloadAndWait`: finished/failed/progress, polling shortcut, reload catalog | `download_and_wait`; behavioral download test |

## Gaps

- `HttpResponse` always carries a body stream, so the source's `!response.body`
  guard in `watch` has no Rust counterpart. A 2xx response with no body ends the
  stream normally; a 204 would not throw `llama.cpp SSE returned HTTP 204`.
- `AbortSignal.reason` is not representable on `CancellationToken`. `sleep`
  reports `Cancelled` (the extension's abort reason); requests report
  `This operation was aborted` (DOMException AbortError text).
- Typed deserialization rejects catalog entries whose optional fields have
  non-matching JSON types; TypeScript keeps the received object. This is the
  typed-core exclusion for invalid runtime input.

## Discrepancies

- `LlamaModelStatus.value` is a closed TypeScript union but `isModelInfo` only
  checks `typeof === "string"`; Rust keeps `value: String` so unknown router
  statuses remain accepted. Status comparisons use the exported constants.

## Proof inventory

- Differential: 26 leaf cases, 5 management actions, 5 catalog error responses.
- Behavioral: 11 cases.
