# Q15 SDK/provider checkpoint

Verified: 2026-10-23. Frozen source: [4f9e6f8a4](source-lock.json). Code checkpoint: 3dcc56ab, including parent8168abc2. Focused evidence only, not whole-lane acceptance.

## Capability/proof matrix

| Boundary | Exact coverage | Evidence / limits |
|---|---|---|
| SDK02/03/04/07/08/09/12, claude_rules, SSH | Literal console output, actual prompt builder/date/cwd, resource state, four auth sessions, full runner results and nine SSH scenarios | b565d620: nine targets GREEN17, q15-sdk-family-checkpoint.log. SDK04 full discovered records historically ignored readdir order (superseded by the exact-order correction in [package-manager trace](traceability/package-manager.md#exact-discovery-order-2026-10-25)); inline-last/filter and matched-input raw console equality remain. This checkpoint predates parent8168abc2 runtime changes; current workspace compilation does not replace behavioral revalidation. |
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

## Q15c auth/types closure (2026-10-24)

Source182-line interface row is done on codeb38f43e6 (parenta913c452 merged), with [complete symbol mapping and retired-test list](traceability/auth-types.md). Bare store methods, required actual model, optional login callback, native human names and fallible context/notify channels replace old-only interfaces. CallbackError comes from shared pi-js-compat; native callback-domain exclusions cite the existing Rust-extension/typed-core decisions. Actual abort(undefined) branding and literal Codex listener-leak DISCREPANCY are tested, not waived.

Bounded core/auth/provider/image regression93 GREEN (`q15c-auth-current-contract-regression.log`); app phase/process/rotation/storage24 GREEN before the stale read-waits fixture failed, then actual frozen sync-lock proof plus corrected cross-process/runtime6 GREEN (`q15c-auth-lock-and-runtime-green.log`). Anthropic6/GitHub12, native interaction10, Codex8/registry3/root2/remaining SDK3 GREEN across the recorded bounded follow-ups. After shared-carrier integration, current contract5 + modifier1 GREEN (`q15c-auth-carrier-{provider,modifier}-green.log`). Workspace/all-target check0 errors/0 warnings: `q15c-auth-workspace-carrier-integrated.{jsonl,stderr}`. All-target warning clippy1230 artifact events,0 errors/0 owned changed-line findings: `q15c-auth-clippy-carrier-integrated-filtered.json`. Six TS oracle type/lint checks GREEN; source counts remain686/186679 (401 done/19 gap/87 partial/179 todo). Manual changed-line Rust/TS readability audit; metrics executable unavailable. No broad runtime-suite claim; parent's known-RED external-editor test remains separately owned.

Next approved slice: outside-consumer audit then delete source-absent OpenRouter/Radius/Kimi/xAI OAuth/load overrides/exports/tests and OpenRouter-image OAuth advertisement. Other implementation rows (helper/context/store) retain their separate proof gaps; interface closure does not promote them.

## Q15c old-only OAuth/load cleanup (2026-10-24)

Codea1c96bcb, parente252716d merged: removed four source-absent OAuth flow modules/exports, bun OAuth registry/override module and sole coding-agent registration call, plus OpenRouter-image OAuth advertisement/error stub. Outside-consumer audit and exact deleted-test names: [load ledger](traceability/load.md). Preserved Anthropic manual/callback + Codex device and two refresh differentials; deleted only old-only flow/Radius-config scenarios. New actual factory RED→GREEN proves API-key-only image auth; full frozen three-loader results GREEN. Loader21-line and image-factory14-line rows now done.

Targeted GREEN7 (`q15c-oauth-retained-flows-1.log`), retained flow after oracle.mts cleanup GREEN1 (`q15c-oauth-retained-flows-final.log`); TS type/lint GREEN2 files. Workspace/all-target0 warnings/errors and changed-line all-target clippy0 findings/1227 artifacts are recorded in the load ledger; mandatory final current check/inventory remains in the external proof ledger. Radius provider/config deletion is the next approved slice, after outside-consumer audit; its remaining source-absent code is not claimed frozen parity.

## Q15c Radius/KnownProvider cleanup (2026-10-24)

Code64dbb742, parentead3bdbc merged. Deleted remaining Radius config module/export/tests and coding-agent Radius constant/export/stale differential branch; provider factory was already gone. [Complete outside-consumer audit and deleted-test names](traceability/radius-config.md). Reconciled public KnownProvider41→36 exact ordered literals against the actual TypeScript compiler API, including openai-codex-team; custom provider strings and real Azure API/orchestrator Radius remain valid. Removed-id generator implementations are explicitly queued for the next whole generate-models.ts re-port, not hidden as completed cleanup.

Known-provider/source differential1 + HTTP2 + constants4 + actual core-leaves constant differential1 GREEN. Workspace/all-target0 errors/0 warnings; changed-line all-target clippy0 findings/1228 artifacts; inventoryPASS686/186679 unchanged403/19/87/177. New compiler oracle tsgo/Biome GREEN; shared historical core-leaves oracle has six unchanged-line lint warnings. Logs `q15c-radius-*` and exact command/revision ledger under `/tmp/pi-rust-repin-delta-ai/`. Types.ts overall row remains partial; no unrelated whole-module promotion.

## Remaining Q15 inventory

| Item | Current evidence / required next proof |
|---|---|
| SDK13 queue-output failure | Historical carrier/order gap stale: frozen catches listener errors and teardown does not abort. Native callback reports/continues; real explicit queue event matches source stderr and successful replacement. Parent72ecf55b removes extra abort; current program stdout and full RPC frames GREEN. [Current proof](traceability/sdk-examples.md#sdk13-queue-output-revalidation-2026-10-23). |
| Six retired oracle imports | Replaced with actual ModelRegistry/AuthStorage/agent-core APIs. Agent-session GREEN1 after current-state stats correction; runner GREEN1 full scenario matrix after explicit image cloning; H GREEN94×6 frames+7 executions exact UTF-16; RPC GREEN1 after parent teardown fix; snake4/invaders6 GREEN. Historical unused invaders-blocker oracle imports migrated but no Rust consumer/proof exists for that diagnostic artifact. |
| Runtime-oddity cleanup | Removed process.env NUL aliasing, shared prototype authority/all four consumers, and auth-warning first-poll/yield emulation. Explicit own-key/native scheduling gap rows replace them. Edit-preview owner-drain retained by main: render-state correctness, not microtask timing. |
| Runtime/process proofs | Historical idle crash/restart proof remains separately parked; no claim that focused SDK/provider tests close it. |
| Final acceptance | Broad remaining module revalidation, current all-feature workspace clippy, readability and independent acceptance remain open. Shared workspace compilation and these bounded matrices are not substitutes. |
