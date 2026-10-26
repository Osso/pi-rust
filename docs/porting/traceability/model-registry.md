# Model registry — common-pin re-port

Source: `packages/coding-agent/src/core/model-registry.ts` at [4f9e6f8a4](../source-lock.json), locked1000 lines. Native: `crates/pi-coding-agent-models/src/core/model_registry.rs`, `model_registry/schema.rs`, `model_registry/registered_providers.rs`. Old import paths re-export this one implementation.

Implementation, full symbol/branch map and scoped behavior matrices are present. Parent36aba9a2 is merged at0639d2d8; constructor dependency renewal and checkpoint gates pass below. The coverage row stays partial pending parent independent acceptance. This is not whole-package/provider acceptance.

## Exports and every private function

| Frozen symbol / lines | Native representation and branch coverage | Proof |
|---|---|---|
| `ResolvedRequestAuth` :252–262 | enum distinguishes caught auth-error union from a rejection of the catch conversion; optional key/header/env omission | `model_registry_auth_drift`, full16-case `model_registry_provider_auth_repin`, request reentry |
| `clearApiKeyCache` :350 | re-export of the shared command-cache authority | `model_registry_clear_cache_repin`: two cached reads execute once; clearing through registry makes the next read execute again, exact values/count1→2; d910fb85 |
| `ModelRegistry`, readonly `authStorage` :355–362 | owned registry state; `auth_storage()` exposes the same storage authority | live storage, registration, lifecycle and SDK consumers |
| `ProviderConfigInput` :973–1001 | optional provider fields plus source-shaped `ProviderModelInput`; callback uses shared acquisition Result | seven input cases, native Infinity/UTF16 copy, ten dynamic-validation cases, four synchronous-throw and six positive-dispatch cases |
| routing/thinking/chat/compat/model/override/provider/root schemas :46–225 | source validation order; unknown properties retained or ignored at the same subsequent read | `model_registry_repin`:35 real files, complete models/errors/serialized order;49 SDK request cases plus Bedrock4selected-value/1strict cases |
| `formatValidationPath` :227–238 | root/property/array-index paths and exact ordered schema errors | null/missing roots, wrong provider type, empty id, multiple wrong model fields in35-file matrix |
| `emptyCustomModelsResult` :274–276 | empty maps/models, optional load error | missing/empty path, invalid JSON/schema/semantic/read cases |
| `mergeCompat` :278–314 | falsey/absent override keeps base; JS own-property spread; special nested fields deep-spread only when either side truthy | complete config/serialized-order matrix and actual SDK request corpus; RED→GREEN6a6be325 |
| `applyModelOverride` :320–347 | defined simple fields, merged thinking map, partial cost defaults, compat merge | model-overrides/nested-compat/thinking-unknown cases, complete builtin records |
| constructor/create/inMemory :364–376 | normalize selected path once before load; empty disables loading; native path errors reject construction | `model_registry_constructor_repin`:10 actual-source private-process cases, including HOME change, default/absolute/empty controls, normalized read/schema/JSON errors and three file-URL failures |
| `refresh` :381–396 | reload auth; clear request/header/error state; reset global API/OAuth; load and iterate live registrations | six-state dispatch lifecycle, disk-refresh control, live-map mutations and modifier failure partial state |
| `getError` :401–403 | load errors remain state; constructor/callback rejections remain Results |35-file and ten-constructor matrices; ten modifier cases |
| `loadModels` :405–432 | builtin/custom merge then credential-bearing OAuth modifiers; publish only after successful modifiers | config corpus, live auth/storage, all-entrypoint modifier throws |
| `loadBuiltInModels` :434–467 | same provider order; OpenRouter shared catalog; provider override then model override |35-file full-model corpus; shared catalog lifetime and actual CLI cache proofs |
| `mergeCustomModels` :470–481 | provider+id replacement retains position; new models append; later custom duplicate wins | duplicate-custom/custom-replaces-builtin/default and complete serialized-array cases |
| `loadCustomModels` :483–536 | missing file; JSON comments/parser; schema and semantic errors; provider/model request records; read-error catch |35 files plus constructor directory/file-URL error cases; no error/path normalization masks; [reciprocal same-file authorship/read proof](#reciprocal-persistence-fixture-2026-10-26) |
| `validateConfig` :538–583 | override-only requirements; custom endpoint/API requirements; builtin inheritance; supplied positive limits | no-override/key-only/custom-endpoint/custom-api/inheritance/context/max/id/multiple-schema cases |
| `parseModels` / `getBuiltInDefaults` :585–637 | provider/model/builtin API+URL precedence; defaults; no-custom-model skip; headers stored separately; exact declared output fields | custom-defaults, builtin-inheritance, provider/model overrides and unknown-property cases; full values/order |
| `getAll`, `getAvailable`, `find` :643–662 | ordered owned models, live configured filtering, exact lookup/miss | `model_registry_frozen`; complete config/lifecycle snapshots; native raw lookup and builtin-preservation controls |
| `hasConfiguredAuth` :665–671 | live AuthStorage OR configured provider key, not key validity |16-case auth/status/key matrix, including empty runtime/stored/configured keys and command witness |
| `getModelRequestKey` :673–675 | provider:id lookup; scalar identities, native header values | real models.json raw-header proof, callback reentry and unmatched lookup controls |
| `storeProviderRequestConfig` :677–694 | no-op without key/headers/true authHeader; overwrite request record otherwise |16 auth cases; positive partial-upsert temporarily removes request key, refresh replays retained config key |
| `storeModelHeaders` :696–706 | nonempty records stored; missing/empty records delete previous entry | registration input/refresh/reentry cases and full resolved-header outputs |
| `getApiKeyAndHeaders` :708–761 | provider config/env before key await, model headers after; storage-defined key wins; static→provider→model→Bearer precedence; empty header/env omitted |16 auth cases, real raw models.json request, reentrant two-request full-value matrix and native missing-key/header-resolution controls |
| `getProviderAuthStatus` :767–790 | sourced storage status wins; falsey configured fallback absent; command/env/literal labels do not execute commands |16 full status/key/request cases with command side effects and empty-key discrepancies |
| `getProviderDisplayName` :795–805 | registered name, registered OAuth name, global OAuth name, builtin map, provider id | source16-case status matrix, six lifecycle states and native registered/OAuth/builtin/fallback controls; prototype gap below |
| `getApiKeyForProvider` :811–821 | any defined storage key wins, even empty; only truthy configured fallback reads env/executes uncached resolution |16-case matrix and six fresh/expired OAuth rejection cases |
| `isUsingOAuth` :826–829 | stored type, not presence of a global OAuth handler | live storage matrix and empty/unknown OAuth cases |
| `registerProvider` :838–842 | validate → apply → upsert; failed apply keeps prior side effects but does not upsert | ten validation cases, seven input cases, modifier partial-state and reentry cases |
| `unregisterProvider` :853–857 | unknown is no-op; delete then refresh remaining entries, even if refresh rejects | six-state lifecycle, builtin preservation, modifier unregister rejection with deleted registration remaining deleted |
| `upsertRegisteredProvider` :865–876 | defined incoming fields replace; absent fields retain; updates keep insertion slot | OAuth-only and callback/header-only updates, refresh replays merged config, live reinsertion appends |
| `validateProviderConfig` :878–900 | callback API validation first; zero models bypass endpoint/key/model requirements; validate all definitions before application | ten actual-source validation cases: missing/empty callback API, empty/absent models, missing URL/key/effective API, model fallback and invalid second model |
| `applyProviderConfig` :902–970 | force OAuth id to provider; raw/simple callback registration; request record; full model replacement/reconstruction or truthy URL/header override; optional modifier | six complete positive callback model/context/options/event/result captures; seven input cases; four callback throws; three live-map mutations and ten modifier rejection cases |

The defaults cache-hit branch inside `getBuiltInDefaults` is structurally unreachable: parseModels calls it once per unique Object.entries provider. Empty/missing builtin defaults and missing effective API/URL skips are defensive guards; real locked catalog providers have defaults, and validated custom configs supply the required values. No source mutation or fabricated catalog is used to manufacture these paths. Opaque/type exclusions below still apply.

## Current behavioral evidence

| Scope | Evidence |
|---|---|
| Configuration and compat |35 real-file full model/error/serialized-order comparisons;49 real SDK shared requests and real Smithy loopback HTTP for4thinking values/1strict control. No request fields or metadata stripped. `q15e-thinking-registry-green-2.log` |
| Registration inputs |7source cases plus native loaded-Model copy proving Infinity and lone-unit unknown compat survive before JSON. Empty model URL uses ??; empty provider-only URL follows the source truthiness gate. `q15e-registration-reentry-green.log` |
| Constructor/auth/modifiers |10constructor,16status/key/auth,6fresh/expired key rejection,10modifier-entrypoint and bounded callback-reentry/live-map cases. Constructor10-case matrix renewed after parent5f14994a's path-helper change (`q15e-post-parent-registry-boundaries`), GREEN. |
| Synchronous dispatch |4raw/simple × TypeError/code/detail/raw UTF16 cases GREEN9256b49b; before a stream exists, callbackCount1. Six positive dispatches and six registration/upsert/OAuth/reset/unregister states GREEN20afdd27, zero HTTP. `q15e-registry-dispatch-lifecycle-2` |
| Required consumers |Default Agent full events/state GREEN3 atbcf84da0; low-level loop/continue original iteration/result rejection GREEN6 at946208f0. Actual frozen lazyApi raw/simple catch + genuinely-uncaught conversion process boundary GREEN atfec20b12. GitLab's four UTF16 cases and source/native uncaught child GREEN8cb4039b; unused scalar import removed9c739f37. Shared lazy/compat/alias/models regressions GREEN47. |
| CLI/catalog |Actual fresh frozen main --list-models has no preloaded memo, requires the cache-only ID and matches the real native binary's stdout/stderr/exit. Cache bytes unchanged; forced offline refresh failure retained. GREEN4828dd79, `q15e-registry-actual-cli-cache-1`. Same-process memo/registry refresh/root-switch graph remains valid; Architect/Supervisor services are type-checked, not launched. |

Artifact/command/revision accounting: `/tmp/pi-rust-repin-delta-ai/q15-proof-ledger.md`. Historical implemented checkpoints: compat2dc7bd9f, thinking311d0f4b, registration44af5afb, fallibility/auth/constructor8df35fc5. Those are bounded proof, not standalone whole-module acceptance. The earlier1679-warning description was not wholly historical:14duplicate identity-conversion emissions at7unchanged constructor callers were owned by the Result migration. Removed4a0d1aac; follow-up had1665total/0owned. Final gate must also audit migration-caused warnings outside changed hunks.

## Required failure boundaries

| Consumer | Frozen boundary | Native |
|---|---|---|
| SDK/services/architect/supervisor/SDK02/09 | sdk:246, services:150, architect:238, supervisor:155/309, SDK examples propagate | same CallbackError; no obsolete ModelRegistryError wrapper or identity conversions |
| AgentSession / immediate ExtensionRunner | agent-session:5445–5451 and runner:461–473 propagate before later state changes | callback/runtime/action/API channels carry the rejection unchanged |
| Queued runner/services registrations | runner:442–455 and services:161–173 catch Error.message/String(error) | convert only there; conversion failure rejects before queue clearing |
| Model selector | infallible construction; asynchronous refresh rejection is uncaught | defer original carrier to shared uncaught authority; actual-source process outcome constructed=true/renders=0, Error/raw values exact |
| Registered raw/simple callbacks | registry:913–923 returns callback result without a catch | shared [acquisition contract](../../specs/provider-stream-errors.md), no invented stream event |
| Default Agent / low-level loop | agent:515–548 catches; agent-loop:54/:85 fails with original rejection | Agent converts at its catch and always finishes the run, including conversion failure; low-level exports retain CallbackError through existing StreamFailure |
| Lazy / GitLab async catch | lazy:49–53; GitLab:347–369 | exact Error.message/String(error); direct native message; only proven process-uncaught conversion rejection uses the relocated authority |

## Removed source-absent behavior and assertion ledger

ModelRuntime/publication/persistence/config/store/composer/remote-catalog facades are deleted, not fallback APIs. Registry owns AuthStorage and its synchronous model list. Arbitrary CredentialStore injection, native-provider/config getters, old auth-check/auth-command/credential-print and background RPC/update-model refresh hooks have no frozen counterpart.

| Retired assertion/API | Surviving behavior |
|---|---|
| raw_error_consumer_components ModelConfig inspection and get_auth_credential extraction | `model_registry_lossless_config::models_json_retains_raw_url_headers_keys_and_bearer_request`: real frozen models.json, full model, storage precedence and native Bearer/header/key values |
| registry native-provider/config getters | observable register/auth/key/unregister and builtin preservation; no fabricated getters |
| lossless_agent_session_paths get_model / FileModelsStore.path | find retains exact raw misses and valid lookups; opaque ingress explicitly rejects lone ids; raw paths use create |
| old AuthEvent::Info | six login fixtures retain prompt/manual/select, persisted credentials, URLs/device/progress/success, browser outcomes, unknown/help and raw error units; no frozen Info event |
| convert_options_grammar_map_produces_custom_tool_calls | removed source-absent grammar conversion API; real old-grammar-tools request retains frozen messages/tool results |

Frozen-absent behavior now remains only unknown ordered metadata: completions supportsFinishReason/chatTemplateArgs/supportsThinkingTokenBudget/supportsOpenAIGrammarTools/deferredToolsMode/sessionAffinityFormat; Responses supportsReasoningEffort/sessionAffinityFormat/supportsStrictMode/supportsOpenAIGrammarTools/supportsAdditionalTools/supportsToolSearch/supportsExplicitPromptCacheMode; Anthropic supportsStrictTools/supportsToolReferences; Bedrock supportsStrictMode. Real source requests cover all16 keys, including session-affinity headers and the frozen missing-finish_reason error. Retired SessionAffinityFormat/DeferredToolsMode types are not alternate paths. Source-present compat/thinking reads remain lossless through the single authorities; see [AI type mapping](ai-types.md) and [compatibility](compat.md).

## Source discrepancies preserved

- Auth flags are not key validity. Frozen AuthStorage can report hasAuth:true with configured:false for runtime empty keys. A stored empty configured provider key can also give hasConfiguredAuth:true while its status is false and no fallback key exists. The16-case matrix preserves these results; status checks do not execute commands.
- getApiKeyAndHeaders' String(error) can itself reject. Fresh/expired OAuth error/raw/object cases preserve that TypeError as Result::Err, distinct from the caught `{ok:false,error}` union. No new wrapper/string projection.
- Refresh iterates the live registered Map. Appends are visited, deletions skipped, reinsertion revisited in its new slot; a cloned pass was wrong (RED2666a0ae→GREEN3afc3a07).

## Approved exclusions / explicit gaps

- Owned returned models and JS object identity/mutation-through-returned-reference follow the2026-10-22 approval; built-in reentry-dependent live state is explicitly ported. Invalid JS runtime input types remain outside typed Rust APIs (2026-08-12).
- Opaque provider/model/API/header names stay Unicode scalars with explicit unpaired rejection; human model names, header values, errors and arbitrary nested JSON retain exact UTF16. No U+FFFD projection before an actual Node filesystem/HTTP/terminal boundary.
- `getProviderDisplayName` :804 reads a plain-object display table. Inherited Object.prototype names can produce non-string values in frozen; native performs own-key lookup and returns the id. This is the approved2026-10-23 runtime-oddity gap, not emulated. AuthStorage's own-key gap remains [owned there](auth-storage.md).
- Cached models with wrongly typed optional known fields fail explicitly under the approved2026-10-22 catalog-cache gap; valid fields/unknown properties/order stay exact. This does not weaken models.json schema acceptance.
- JS/V8 stack text is omitted, never fabricated. Existing native async scheduling/event-snapshot exclusions remain; tested request-auth read ordering and synchronous acquisition boundaries are not relaxed.

## Acceptance isolation (2026-10-25)

Parent independent acceptance found `model_registry_agent_stream_throw` racing: its two parallel tests use distinct API ids, but `unregister_provider` calls `refresh`, which clears the process-global API registry (frozen :381–396). Reproduced on the third parallel run. These fixtures now retain their private registrations until process exit; no global reset can remove the peer's callback. Lifecycle cleanup remains covered by the dedicated single-test `model_registry_dispatch_lifecycle` target, not by incidental cleanup in rejection fixtures. No serialization, retries, or weaker rejection assertions.

Audit of all18 `model_registry*` binaries: only this binary combines active custom API dispatch with peer global reset. `model_registry_registration_repin` has parallel model-only registrations and one validation cleanup, but no API/OAuth callbacks or global lookups of those registrations. `model_registry` and `model_registry_lossless_config` reset only alongside builtin/local model/auth checks; neither peer installs custom global callbacks. The compatibility-request target has three direct-adapter tests and no reset. Reentry/constructor/provider-auth environment cases each run in private child processes; modifier/dispatch/stream-throw/OAuth-throw targets each have one active test. Other targets do not reset API/OAuth state. At dd379df39, ten consecutive two-thread runs pass (20 tests,90 rejection cases); `q15f-registry-race-green.log`. Workspace/all-target check has0errors/0warnings; warning-mode all-target/all-feature Clippy has0errors/1664existing warnings/0findings in the changed test. No production code changed. This closes the parent's concrete independent-acceptance finding and permits module promotion; results live in the proof ledger.

## Completed checkpoint evidence

At0639d2d8 (parent36aba9a2 integrated): workspace/all-target check0errors/0warnings; all-feature/all-target warning Clippy0errors/1664existing warnings/0owned hunks and0new warning signatures against the pre-stream1665 baseline, including unchanged callers. Owned Codex identity conversion was removedcc320248; its target passes strict Clippy. Strict pinned-source/8fixture TS and Biome pass after595d8104; approved fixture-only ES2024 for unchanged TUI /v, no source/shared-builder change. Inventory686rows/186679lines PASS; fmt PASS.

Current scoped regressions: Agent31+loop43, shared lazy/compat/alias/models47, renewed default Agent/low-level cases9 and constructor10, complete GitLab6native targets (including4negative acquisitions and uncaught process proof), Codex2direct/lazy controls, source/native lazy2targets and16existing authority tests after parent's hook-diagnostic merge. The intentionally ignored private child entrypoints are invoked by active parent process tests. Exact commands/revisions/results and invalidation accounting remain in the proof ledger; `q15e-stream-postmerge-*` and `q15e-stream-final-*` contain gate artifacts. No unchanged broad gate was rerun for a milestone alone.

Manual changed-line Rust/TS readability audit: one rejection authority, pure success adapters, explicit catch-only conversion and process boundaries, bounded/reaped child probes; extracted the default Agent oracle from the growing registry fixture and removed its new any annotations. No new blocker/suppression/dependency. Metrics CLI unavailable, so no automated complexity claim. Parent independent acceptance's fixture-isolation finding is closed above. This module's evidence does not claim provider/package-wide parity.

## Reciprocal persistence fixture (2026-10-26)

`differential_model_persistence_reciprocal` adds a same-file TS-authored → native read → TS reread and native-authored → TS read → native reread → TS reread proof, including a second configuration revision. Full projected models and serialized property order, raw file bytes, unknown fields, and human UTF-16 units are asserted. Frozen `model-registry.ts` only reads models.json (`loadCustomModels`); `registerProvider` retains runtime registrations and `refresh` reloads them, with no configuration writer. Both fixture authors use direct filesystem writes, not an invented persistence API; both registries refresh without altering the file. Focused execution passes at `ff3e911e0`: two tests in `/tmp/models-reciprocal-tests-2.log`; targeted check/clippy, oracle strict no-emit typecheck, and source inventory pass. No production discrepancy found. Catalog writer reciprocity is tracked in [model-catalog-cache](model-catalog-cache.md#reciprocal-persistence-fixture-2026-10-26).
