# `codex-image-generation` extension traceability

Source: `packages/coding-agent/extensions/codex-image-generation/src/index.ts`.
Rust: `crates/pi-coding-agent-extensions/src/bundled_extensions/codex_image_generation/mod.rs`.

| TypeScript symbol / branch | Rust item | Proof |
|---|---|---|
| `default` factory (`image_gen` tool only) | `codex_image_generation_extension`, `register` | `codex_hosted_tools.rs::image_generation_registers_tool_without_payload_handler` |
| `createImageGenerationToolDefinition` metadata | `create_image_generation_tool_definition` | differential `image-metadata`; behavioral registration test |
| `isOpenAIHostedImageGenerationModel` | `is_openai_hosted_image_generation_model` | differential `image-model-*` |
| `executeImageGeneration` (prompt trim, model guard, auth, timeout) | `execute_image_generation` | differential `image-execute*`; behavioral tests |
| `runImageGenerationWithTimeout` (`AbortSignal.any`) | `hosted_deadline::spawn_deadline` | `differential_codex_hosted_abort`, behavioral `image_generation_applies_timeout_to_the_request_signal` |
| `runHostedImageGeneration` (`streamSimple`, hosted payload, error/aborted, image extraction) | `run_hosted_image_generation` | default tool path; payload transform differential |
| `addImageGenerationToolToPayload` | `add_image_generation_tool_to_payload` | differential `image-payload-*` |
| `saveGeneratedImage` (`image-gen-<hex>.png`, `wx` flag, base64 decode) | `save_generated_image` | differential `image-execute`; behavioral file-content test |
| `formatTimeout` | `hosted_deadline::format_timeout` | custom timeout reason message in actual-module differential |

## Discrepancies / gaps

- The generated file name uses 8 OS-random bytes in hex, matching
  `randomBytes(8).toString("hex")`; differential output substitutes the random
  name.
- Common-pin reason/lifetime proof: `differential_codex_hosted_abort` imports the actual module. Nine cases compare custom timeout Error, default/caller native Error/raw-value reasons, pre-abort (worker still invoked, unlike web search), cleared timer and retained caller forwarding after worker failure. `hosted-abort-reasons-red.log` → `hosted-abort-reasons-green.log`. Shared `hosted_deadline.rs` uses synchronous AbortSignal.any and clears only the timer after completion. Fractional/negative/nonfinite timeout options remain unported (`u64`), not covered by this bounded proof.
- `ImageGenerationToolOptions::{run_generation, timeout_ms}` are test seams.

## Whole-module re-pin audit (2026-10-25)

All symbols and branches in the table were compared with unmodified `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`: default registration, metadata/schema, payload tool filtering and duplicate/no-change branches, model/prompt/auth gates, request forwarding, timeout composition/cleanup, hosted stream stop/error handling and output extraction. Exact UTF-16 carriers remain in prompt, payload and results. Hosted tools use scripted worker seams, never live providers.

Current proof: frozen helper/execute differential, 12 behavioral tests, and actual-module abort/lifetime differential; see [lane ledger](../bundled-ext-b-proof.md). Gap: pathological JSON shapes, fractional/extreme timers, entropy and OS faults follow the approved user-observable parity policy. Default hosted network integration is not exercised against live services.

Restored the frozen `openai-codex-team` model branch; new frozen-oracle model case failed before the change. Image file naming, exclusive-create mode, base64 bytes and returned text/image order remain exact (random path normalized by the oracle).
