# AI API common-pin proof ledger

Lane: `repin/ai-api`, `/home/osso/.worktrees/pi-rust-repin-baseline-triage`. Reference is the unmodified [common source lock](source-lock.json). No live providers, new dependency, push or deployment. All Cargo commands use `-j3`, outside the mandatory runtime sandbox runner.

## Recorded proof

| Revision / exact scope | Command suffix after `cargo test -j3 -p` | Result / log |
|---|---|---|
| c5d1cf2c0 builder/test, merged parent9cffca162; later changes outside builder | `pi-ai --test suite_core_1 differential_simple_options` | GREEN2:45 base cases,168 budgets,all clamps; `/tmp/ai-api-simple-green.log`; genuine RED `/tmp/ai-api-simple-red-2.log` |
| efac14611 lazy implementation | `pi-ai --test suite_core_1 api_lazy::` | GREEN5; `/tmp/ai-api-lazy-green.log`; EOF RED `/tmp/ai-api-lazy-red.log` |
| efac14611 app handler/iterator failure boundary | `pi-coding-agent --test provider_lazy_acquisition` | GREEN3, two ignored helpers invoked by active parent tests; `/tmp/ai-api-lazy-inner-green.log`; genuine RED `/tmp/ai-api-lazy-inner-red.log` |
| fd10ca1e1 image native JSON/callback and SDK decode/retry/timeout | `pi-ai --test suite_core_1 differential_openrouter_images` | GREEN2:19 source scenarios plus native UTF16 caption/replacement; `/tmp/ai-api-images-green.log`; REDs `images-{red,retry-red,timeout-red}.log` |
| a3ac7b863 image behavior/signal-aware timeout fixture | `pi-ai --test suite_core_2 openrouter_images::` | GREEN13; `/tmp/ai-api-images-behavior-green-2.log`; later callback-format changes require renewed relevant proof |
| 71784594a six direct Simple methods and adapters | `pi-ai --test suite_core_1 differential_simple_acquisition` | GREEN1/12 exact source cases; `/tmp/ai-api-simple-acquisition-green.log`; RED same prefix `-red.log` |
| bc0ff8041 eight API callback catch adapters | `pi-ai --test suite_core_1 differential_api_catches` | GREEN1/32 full-event/result source cases; `/tmp/ai-api-catches-green.log`; REDs same prefix `-red`, `-red-2`, `-images-red` |
| Responses/Azure/shared source scope before phase removal, based on bc0ff8041 | `pi-ai --test suite_core_1 -- differential_openai_responses_shared differential_openai_responses:: differential_azure_openai_responses:: --nocapture` | GREEN15; `/tmp/ai-api-responses-green-1.log`; phase follow-up changes this scope and requires renewal |
| New Responses/Azure field-selection and shared conversion/stream tests against preceding code | focused filters `options_match_frozen`, `differential_openai_responses_shared`, `message_phase_does_not_override` | genuine REDs `/tmp/ai-api-responses-options-red.log`, `/tmp/ai-api-responses-shared-red.log` (six independent failures), `/tmp/ai-api-responses-phase-red.log` |

`cargo fmt --all --check` passed before each implementation commit. Final targeted clippy/readability and all remaining assigned-row proof are still open. No workspace acceptance claim.

## Current audit boundaries

- Closed body audits: simple-options, transform-messages (parent63768e977), lazy, image API, Responses, Azure and shared Responses. Responses/Azure default malformed-SSE console output remains an explicit SDK-shim gap until its process proof lands.
- Remaining assigned audits: Completions; Google shared/Generative/Vertex; Anthropic; Bedrock; Mistral. Their prior trace tables are not promoted by the bounded callback/acquisition work above.
- Mechanical cross-lane edits: shared image callback uses JsValue; existing app lazy-acquisition and registry-compat fixtures; Codex shared-message conversion error propagation changes String→JsString without changing algorithms.
