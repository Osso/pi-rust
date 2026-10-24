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

## Q15c package-manager closure (2026-10-24)

Code59489965 (parentf9167bc9 merged). Frozen2610-line module row is done with [explicit runtime gaps](traceability/package-manager.md). Environment/stdio and concurrency fixes have real-process actual-source RED/GREEN proofs. Manager-owned workers retain Arc-shared callbacks/cache; no public signature changes. Removed source-absent autoload/delta logic, reduced semver parser, and obsolete implicit-maxBuffer emulation (the last explicitly excluded by parent ruling).

Bounded four-target package regression GREEN56 (`q15c-package-regression.log`); process matrix GREEN8 (`q15c-concurrency-green-2.log`, including two child-entry guards); final lint-only stdin fixture GREEN1 (`q15c-stdio-fixture-final.log`). Workspace/all-target check exits0 with zero errors/warnings (`q15c-workspace-check-final.{jsonl,stderr}`); warning-mode coding-agent/all-target clippy exits0,1068 artifacts, zero changed-line findings (`q15c-clippy-final-filtered.json`). Oracle tsgo/Biome lint GREEN (`q15c-oracle-{typecheck,lint}.log`). Inventory PASS686/186679 (`q15c-source-inventory.log`). Manual Rust/TS changed-line readability audit; metrics executable unavailable. These focused proofs do not replace parent final milestone/independent verification.

## Remaining Q15 inventory

| Item | Current evidence / required next proof |
|---|---|
| SDK13 queue-output failure | Historical carrier/order gap stale: frozen catches listener errors and teardown does not abort. Native callback reports/continues; real explicit queue event matches source stderr and successful replacement. Parent72ecf55b removes extra abort; current program stdout and full RPC frames GREEN. [Current proof](traceability/sdk-examples.md#sdk13-queue-output-revalidation-2026-10-23). |
| Six retired oracle imports | Replaced with actual ModelRegistry/AuthStorage/agent-core APIs. Agent-session GREEN1 after current-state stats correction; runner GREEN1 full scenario matrix after explicit image cloning; H GREEN94×6 frames+7 executions exact UTF-16; RPC GREEN1 after parent teardown fix; snake4/invaders6 GREEN. Historical unused invaders-blocker oracle imports migrated but no Rust consumer/proof exists for that diagnostic artifact. |
| Runtime-oddity cleanup | Removed process.env NUL aliasing, shared prototype authority/all four consumers, and auth-warning first-poll/yield emulation. Explicit own-key/native scheduling gap rows replace them. Edit-preview owner-drain retained by main: render-state correctness, not microtask timing. |
| Runtime/process proofs | Historical idle crash/restart proof remains separately parked; no claim that focused SDK/provider tests close it. |
| Final acceptance | Broad remaining module revalidation, current all-feature workspace clippy, readability and independent acceptance remain open. Shared workspace compilation and these bounded matrices are not substitutes. |
