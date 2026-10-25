# `codex-web-search` extension traceability

Source: `packages/coding-agent/extensions/codex-web-search/src/index.ts`. Rust:
`crates/pi-coding-agent-extensions/src/bundled_extensions/codex_web_search/mod.rs`.

| TypeScript symbol / branch | Rust item | Proof |
|---|---|---|
| `default` factory (`web_search` tool + `before_provider_request`) | `codex_web_search_extension`, `register` | `codex_hosted_tools.rs::web_search_registers_tool_and_payload_handler` |
| `createWebSearchToolDefinition` metadata | `create_web_search_tool_definition` | differential `web-metadata`; behavioral registration test |
| `isOpenAIHostedWebSearchModel` | `is_openai_hosted_web_search_model` | differential `web-model-*` |
| `executeWebSearch` (query trim, model guard, auth, timeout) | `execute_web_search` | differential `web-execute*`; behavioral tests |
| `runSearchWithTimeout` (`AbortSignal.any`, timeout message) | `run_search_with_timeout` | behavioral `web_search_times_out_with_source_message`, `web_search_preserves_caller_abort` |
| `waitForWebSearch` / `throwIfAborted` / `toAbortError` | synchronous reason check + combined signal select, `web_search_abort_error` | `differential_codex_hosted_abort` |
| `formatTimeout` | `format_timeout` | timeout message assertion |
| `runHostedWebSearch` (`streamSimple`, hosted payload, error/aborted, text extraction) | `run_hosted_web_search`, `extract_text` | code path shared with the default tool; payload transform differential |
| `addWebSearchToolToPayload` | `add_web_search_tool_to_payload` | differential `web-payload-*` |
| `createWebSearchTool` / `isHostedWebSearchTool` / `isFunctionWebSearchTool` | helpers in the module | differential payload cases |

## Discrepancies / gaps

- Common-pin reason/lifetime proof: `differential_codex_hosted_abort` imports the actual bundled module. Nine cases compare custom timeout Error, default/caller native Error/raw-value reasons, pre-abort/no worker, missing caller, cleared timer and retained signal forwarding after success. `hosted-abort-reasons-red.log` → `hosted-abort-reasons-green.log`. `hosted_deadline.rs` shares literal setTimeout/AbortSignal.any setup with image generation; no forwarding tasks or timed_out side channel. Source Error/DOMException reasons retain their message; plain lookalikes become the fixed generic abort message. Fractional/negative/nonfinite timeout option inputs remain unported (`u64`), not covered by this bounded reason proof.
- `streamSimple` uses `pi_ai::compat::stream_simple`; HTTP is the shared
  `pi_ai::request::Fetch` client.
- `WebSearchToolOptions::{run_search, timeout_ms}` are test seams; production
  callers use `default()`.

## Whole-module re-pin audit (2026-10-25)

All symbols and branches in the table were compared with unmodified `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`: default registration, metadata/schema, payload tool filtering and duplicate/no-change branches, model/prompt/auth gates, request forwarding, timeout composition/cleanup, hosted stream stop/error handling and output extraction. Exact UTF-16 carriers remain in prompt, payload and results. Hosted tools use scripted worker seams, never live providers.

Current proof: frozen helper/execute differential, 12 behavioral tests, and actual-module abort/lifetime differential; see [lane ledger](../bundled-ext-b-proof.md). Gap: pathological JSON shapes, fractional/extreme timers, entropy and OS faults follow the approved user-observable parity policy. Default hosted network integration is not exercised against live services.
