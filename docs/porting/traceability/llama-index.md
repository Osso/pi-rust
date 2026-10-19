# llama extension traceability

Pinned source: `packages/coding-agent/src/extensions/llama/index.ts` and
`packages/coding-agent/src/extensions/index.ts` at
`532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-coding-agent/src/bundled_extensions/llama/index.rs`,
`crates/pi-coding-agent/src/bundled_extensions/mod.rs`.

No differential oracle: the module exports only the extension factory; its
observable behavior is registration and the interactive command flow, proven
with a fake `Fetch`, a real `ModelRegistry`, and a fake `ExtensionUiContext`
that runs the factory (`crates/pi-coding-agent/tests/llama_index.rs`).

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `llama/index.ts:9-11` | `modelIsLoaded` (`loaded`/`sleeping`) | `model_is_loaded`; helper test |
| `llama/index.ts:13-18` | Connection errors match `fetch failed`/`timeout`/`network` in `name + message` | `is_connection_error`; helper test (message-only, see gaps) |
| `llama/index.ts:20-23` | Connection message replacement | `connection_error_message`; helper test |
| `llama/index.ts:25-33` | `owner/repo[:quant]` split at the first colon after the first slash | `parse_hugging_face_model`; 4 helper cases |
| `llama/index.ts:35-46` | Configured client from provider auth; warning when unconfigured; normalized URL + key | `configured_client`; TUI flow test (stored credential) |
| `llama/index.ts:48-60` | Extension registers the native provider and `/llama` command | `llama_extension_with_fetch`; registration test |
| `llama/index.ts:55-72` | `syncCatalog`: 15s timeout, list fallback, catalog update, refresh, aborted/error propagation | `sync_catalog`; TUI flow + provider refresh tests |
| `llama/index.ts:74-127` | `loadModel`: loaded-model prompt, replace/restore, progress cancel, refreshed status notification, restore on failure | `load_model`; helper-level client/provider/UI tests; TUI flow covers catalog/notify wiring |
| `llama/index.ts:129-140` | `unloadModel`: confirm, wait, resync, notify | `unload_model`; TUI flow test |
| `llama/index.ts:142-201` | `downloadModel`: HF search/details, gated prompt, quantization choice (recommended label), progress, resync, notify | `download_model`; HF/provider/UI tests + differential quantization/search |
| `llama/index.ts:203-230` | Command handler: TUI-only warning, configured client, read/retry/close loop, action dispatch, connection-error suppression | `run_llama_command`; non-TUI warning test + TUI flow test |
| `extensions/index.ts:1-4` | Built-in hidden `llama.cpp` inline extension | `register_builtin_extensions`; registry test |

## Gaps

- `CallbackError` carries no `Error.name`; `is_connection_error` checks the
  message only. The default `Fetch` reports reqwest transport text rather than
  Node's `fetch failed`, so transport failures may not classify as connection
  errors until pi-ai normalizes the text.
- `loadModel`'s prompt/restore branches are covered by their components
  (`LlamaUi`, client load/unload, catalog sync) rather than one end-to-end
  multi-prompt test; the TUI flow exercises the command loop and unload path.
- `downloadModel`'s HF token lookup reads the process environment/home (no
  injection); the differential covers `findHuggingFaceToken` separately.

## Discrepancies

- The source's `loadModel` catch swallows restore failures to preserve the
  original error; Rust does the same explicitly.
- `runWithProgress` cancellation aborts with `Error("Cancelled")`; Rust cancels
  the `CancellationToken` (no reason payload).

## Proof inventory

- Behavioral: 5 cases (helpers, registration, non-TUI warning, TUI unload flow).
- Supporting differentials: llama client/huggingface/provider/ui.
