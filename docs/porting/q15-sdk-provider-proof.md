# Q15 SDK/provider checkpoint

Verified: 2026-10-23. Frozen source: [4f9e6f8a4](source-lock.json). Code checkpoint: 3dcc56ab, including parent8168abc2. Focused evidence only, not whole-lane acceptance.

## Capability/proof matrix

| Boundary | Exact coverage | Evidence / limits |
|---|---|---|
| SDK02/03/04/07/08/09/12, claude_rules, SSH | Literal console output, actual prompt builder/date/cwd, resource state, four auth sessions, full runner results and nine SSH scenarios | b565d620: nine targets GREEN17, q15-sdk-family-checkpoint.log. SDK04 full discovered records ignore only approved readdir order; inline-last/filter and matched-input raw console equality remain. This checkpoint predates parent8168abc2 runtime changes; current workspace compilation does not replace behavioral revalidation. |
| Provider factory/catalog/auth/login/dispatch | All36 factory descriptors/order/picker IDs, complete generated catalogs, model-aware getAuth results, complete login events/prompts/results, captured HTTP requests/errors with existing host-header/number normalization, and explicit unsupported OpenCode Go Responses API | differential_providers GREEN6, q15-provider-family-green-2.log. Vendor-SDK protocol construction has separate oracles; this fetch differential excludes Bedrock/Google SDK request paths. No whole-factory mutation/alias claim. |
| Cloudflare auth | Ten complete result cases: missing/empty required values, stored ownership vs ambient env, scoped headers/baseUrl, repeated placeholder replacement and Workers unused gateway | Actual-source RED a12f44de → GREEN in six-case suite. Deleted source-absent stream wrapper/module/export/tests. |
| Native provider regressions | Provider auth20; provider metadata/catalog dispatch7; Codex aliases3 full record equality; model/auth/options header separation1; compat default1 | q15-provider-family-green-2.log, q15-provider-native-regressions-2.log, q15-codex-alias-current-green.log, q15-model-header-options-regression.log, q15-compat-token-default-green.log. Earlier failed commands/logs remain in external ledger; filenames alone are not evidence. |
| Model headers on wire | Models→actual API→fake HTTP for nine chat adapters; image API→fake HTTP. Model sentinel survives Anthropic/Completions/Responses/Codex-SSE/Azure/Gemini/Vertex/Mistral/images; Bedrock ignores model-only sentinel and retains explicit option header | f782f15a GREEN1/ten adapters, q15-api-wire-headers-2.log. Bedrock matches frozen bedrock-converse-stream.ts:214. Codex fixture explicitly selects SSE; this is not WebSocket proof. |

Logs: /tmp/pi-rust-repin-delta-ai/. Full revision/command/result ledger: q15-proof-ledger.md in that directory. No live providers/SSH hosts used.

## Integration checks

At3dcc56ab, cargo fmt --all --check passes. Targeted pi-ai clippy --lib plus differential_providers/provider_auth/providers/providers_openai_codex/models/api_model_headers with -D warnings passes, zero warnings. cargo check -j4 --workspace --all-targets passes, zero warnings (q15-provider-targeted-clippy.log, q15-sdk-provider-workspace-check.log). Later documentation-only commits do not invalidate these code proofs.

Changed Rust/TS received manual readability inspection; rust-code-analysis-cli is unavailable. Whole-lane readability/independent acceptance and workspace all-feature clippy remain parent gates. Cloudflare's literal resolver remains a longer read/validate/assemble function; any readability-only decomposition is separate from this parity fix.

## Remaining Q15 inventory

| Item | Current evidence / required next proof |
|---|---|
| SDK13 queue-output failure | [Documented carrier/order gap](traceability/sdk-examples.md#blocked-13-queue-output-failure-before-disposal); manual oracle imports migrated, not a new failure-order proof. Parent runtime changed afterward, so current failure must be revalidated before remediation. |
| Retired imports in six other oracles | Confirmed locally: oracle/agent-session-oracle.ts:13; extensions-runner-oracle.ts:30; extension-examples-h-oracle.ts:14; extension-examples-j-rpc-oracle.ts:30; extension-examples-l-games-oracle.ts:8; extension-examples-l-invaders-blocker-oracle.ts:8. Replace actual model-runtime.ts/test-utils imports, not with facades. |
| Runtime-oddity cleanup | Existing inherited-property emulation remains in login_options.rs:15, idle_compaction.rs:10, interactive_mode.rs:10012; warning first-poll/yield machinery remains near12502–12522. Parent owns classification/removal under latest policy; no new compatibility carrier. |
| Runtime/process proofs | Historical idle crash/restart proof remains separately parked; no claim that focused SDK/provider tests close it. |
| Final acceptance | Broad remaining module revalidation, current all-feature workspace clippy, readability and independent acceptance remain open. Shared workspace compilation and these bounded matrices are not substitutes. |
