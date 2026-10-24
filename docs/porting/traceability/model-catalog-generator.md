# Model catalog generator

## Q15c active common-pin re-port (in progress)

Current authority is frozen4f9e6f8a4,2232 physical lines (locked coverage2231). Historical implementation/proofs below describe the retired generator and are **not current-pin parity evidence**. Parent ruling2026-10-24: retain native JSON/include artifacts only under the2026-10-20 pre-repin approval, **to re-validate against4f9e6f8a4**; remove old CLI flags/modes/strict/public/json-only/data-only, transaction/rollback/manifest validation, cost/Baseten/Fireworks helper interfaces and reasoning_options side store. Preserve frozen fetch/transforms/metadata/stats and delete-old-before-write-new order.

DISCREPANCY: frozen emitModel interpolates unescaped strings and arbitrary quoted IDs into TS. JSON escaping is inherent to approved native artifacts; malformed TS output is not emulated. Native artifacts must roundtrip quotes/backslashes/lone-surrogate strings exactly. Tests for this boundary remain pending.

Initial metadata slice: actual unchanged frozen functions are exposed through TypeScript AST in generator-repin-oracle.mts. Three concrete models reproduced old DeepSeek compat/thinking, Codex-team/Astra and Sonnet5.5 differences (q15c-generator-metadata-red.log). Native UTF-16 metadata replacement follows frozen thinking-then-completions order; source-absent strict/grammar/tool-search/cache augmentation and deferred reasoning application removed. FocusedGREEN1/three cases q15c-generator-metadata-green-2.log. The expanded metadata matrix checks5880 ordered JSON results across24providers×7APIs×35IDs (q15c-generator-metadata-matrix-2.log); not exhaustive URL branches. Fetch/modeldev source probes reproduced old HTTP-status rejection and old-only provider emission (q15c-generator-fetch-red-2.log). Replacement modeldev passes source-order24-provider smoke and OpenRouter503 body/pricing differential (q15c-generator-fetch-green-1.log). Raw UTF16 parseFloat now delegates to pi-js-compat, with pinned Node binary64-bit prefix tests (q15c-parse-float-utf16-green.log). Old-only record state removed; full branch/error/log/CLI/artifact work and final gates remain open. Catalogue-stage actual-source workflowRED (q15c-generator-catalog-red.log) found old Azure synthesis/missing Codex-team; replacementGREEN (q15c-generator-catalog-green-1.log) compares ordered models from198 fixture inputs plus frozen supplements. Long-context tiers/old GPT5.6 pricing and filtering removed; source exact Codex/GC/Team lists restored. No module closure or generated snapshot mutation.

| Current-pin explicit gap | Representation | Authority/proof |
|---|---|---|
| Inherited Object.prototype lookup in provider/model grouping (`generateModels`,2125–2135) | Native owned maps treat every model ID as data; no prototype-key drops or prototype mutation | Approved2026-10-23 Node/V8/runtime oddity policy; quoted/lone-unit artifact fixtures still pending |


Source: pinned `packages/ai/scripts/generate-models.ts` ([source lock](../source-lock.json)). Native implementation: `crates/pi-ai/src/model_catalog_generator.rs` and its `metadata`, `models_dev`, `catalog`, `artifacts`, `emission_types`, `receiver_types` modules; explicit maintenance entrypoint `crates/pi-ai/src/bin/generate-models.rs`. Status: **done within approved native adaptations**; consumed-field audit and targeted proof below. Baseline provider JSON/manifests remain unchanged.

## 2026-10-24 ingress/CLI/artifact ruling (ongoing)

The2026-10-20 native receiver/emission TypeError deviation is **superseded**: HTTP/JSON ingress is not typed-core input. Frozen uses/defaults/coercions/errors govern; untouched fields require no tests. `receiver_types.rs` and `emission_types.rs`, source-absent helpers/options/modes/record store/manifest/transaction are retired. Shared `CallbackError` now carries native Error branding or arbitrary JSON throws into console events; no generator-specific error carrier. All HTTP statuses consume response bodies. CLI ignores argv and logs top-level rejection without setting an added failure exit.

Actual-source malformed ingressRED:124 differences (q15c-generator-ingress-red.log). First rewrite passes all but3 NVIDIA optional-includes cases (q15c-generator-ingress-green-1.log); those guards are corrected, rerun pending. ArtifactRED q15c-generator-artifact-red-3.log reproduces obsolete manifest emission; new artifact test GREEN in q15c-generator-ingress-green-1.log proves stale deletion before writes, untouched files, quote/backslash/lone-surrogate roundtrip and source stats. JSON/include generated-path announcements are the native artifact path adaptation; statistic strings/order stay exact. Legacy suite migration/retirement and final gates remain open.

## Historical pre-repin capability and proof matrix (not common-pin evidence)

| Source symbols/branches | Native representation | Behavioral proof | Remaining proof/gap |
|---|---|---|---|
| `readGeneratorOptions`: strict/data-only/json-only/public directory/pretty; unknown/missing/conflicting arguments | `read_generator_options` | options test: 12 source argument cases; real binary invalid-mode exit/stderr | deleted-CWD failure proved81; non-UTF8 OS argv explicitly excluded |
| `getModelsDevCost`, `roundCost`, `withOpenAiLongContextPricing`: plain rates/context tiers/defaults | cost transform; shared binary64 coercion/rounding; catalog overrides | cost, models.dev, full-workflow fixtures | nonfinite/range restrictions intentionally not added |
| `processBasetenModels`: deprecation, GLM-5.2, toggle/effort | direct transform, shared typed effort converter | cost/raw/receiver fixtures; source-preserving unknown-string/unused-budget short-circuits | fixed branch map below |
| `processFireworksModels`: tool filter, GLM-5p2/Kimi-K3 compatibility | direct transform | cost/all-provider/receiver fixtures | fixed branch map below |
| `fetchOpenRouterModels`, `fetchAiGatewayModels`: tool/image/reasoning/pricing, whole-fetch default discard/strict throw | injected synchronous transport and lossless transformations | HTTP36 raw cases; fetch24 all-fetcher status/transport/JSON cases | identified consumed-field corrections proved74/78 |
| `fetchNvidiaNimModelIds`, `normalizeNvidiaModelId`: direct/normalized aliases, overwrite/insertion order, conditional lookup | lossless UTF16 map and normalization | fetch Map fixtures; every UTF16 unit/every Unicode scalar/contextual casing/lone surrogates | no counterexample in enumerated scope; future Unicode-version mismatch not claimed impossible |
| `loadModelsDevData`: all source provider families, routing/filtering/aliases/pricing | `models_dev.rs`; reasoning-record state persists before later fetch failure | all-provider fixture, six alias/pricing/lossless fixtures; strict Qwen exact-ID fixtures; pre-error ordering | literal provider-region fixture mapping below |
| `mergeThinkingLevelMap`, `getModelKey`, `recordModelsDevReasoningOptions`, `supportsDirectReasoningEffort`, `applyModelsDevReasoningOptionMetadata` | metadata merging and deferred catalog reasoning pass | full workflows; pre-error record ordering; five malformed deferred reasoning cases fail outside fetch catch after all three requests | deferred invalid controls fail in original outer region |
| Together compat/map, OpenAI xhigh/max predicates, Google/Anthropic family predicates; compat detection/delta/merge; strict/grammar/search/cache/thinking metadata; Anthropic compat; Bedrock URL | `metadata.rs`, `apply_model_metadata`, provider routing | 675 model/provider/API combinations against six original mutation functions; 15 raw cache/Google-pattern cases; provider fixtures | URL-only and empty-compat branches proved79; literal helper mapping below |
| `generateModels`: sequential loads, overrides/supplements, Codex/GC, Azure, first-source dedup, priority and sorting | `catalog.rs`; UTF16 lexical provider/model/API order | three full actual-source workflow fixtures including collisions, inherited names, aliases, distinct surrogates and numeric keys | concrete duplicate/inherited-name cases mapped below |
| copied Model fields, consumed provider/model/cost/modalities/reasoning receivers | native TypeErrors at actual use/emission points, not a schema prepass | ID14 cases; emission malformed name/cost/context/token matrix; receivers/defaults/filter/strict regions | complete consumed-field matrix below; copied fields remain emission-checked |
| grouped private/flat public artifacts; manifest/hash/timestamp/pretty; include glue | `artifacts.rs`; shared model-data helpers; native include table | four artifact modes; source bytes; compile+execute generated includes; every baseline embedded value unchanged | native output adaptation approved; TS shard syntax deliberately not emitted |
| data-only import guards and selected subset | native include-table reader; shared validators | empty/duplicate/missing-provider guards after three fetches; existing files unchanged; selected subset ignores malformed un-emitted Copilot value | arbitrary include syntax excluded from generated-file grammar |
| staged native transaction and rollback | actual local IO seam `CatalogFileSystem`; shared Node FS records | eight stage-write/validation/include-write/rename/post-manifest/post-provider/post-include/cleanup fault boundaries, previous valid bytes restored and staging checked | rollback domain is native data/include only; public output is separate |
| public reset/mkdir/three write boundaries | output removed/recreated after native commit | five public failure boundaries preserve name/message/errno/code/syscall, remaining files, native committed bytes, cleanup and log order | remove-boundary fixture injects real directory-read EISDIR, not host permission changes; arbitrary host FS errors not enumerated |
| original automatic CLI catch, immediate logs, stats, error exit | `run_generator_cli`, maintenance binary | original automatic CLI fixture success/strict failure and native log/error ordering; real invalid-argument subprocess | successful native subprocess generation intentionally not run against live catalogs |
| native connection/body failures and status-before-body ordering | shared `default_fetch`, `read_body_text`; structured `CallbackError` fields | actual refused connection and premature body vs Node native fetch; actual generator503/truncated body differential | only native causes and JS-specific frames differ by approval |

## Original-source oracle

`oracle/model-catalog-generator-oracle.ts` transpiles actual pinned source with an AST exposure pass; private function bodies are not cloned. Whole generation uses independent HTTP fixtures and temporary real filesystem roots. The original automatic `generateModels().catch(...)` is retained for CLI cases. Native-network fixtures accept explicitly supplied HTTP127.0.0.1 URLs only. Byte-LF framing preserves literal U+2028. Full source diagnostics are retained in `/tmp/pi-rust-model-generator-proof/ts-diagnostics-<pid>.jsonl` and fault-specific JSON artifacts.

## Development proof ledger

Commands use argv through Pyrun; focused development tests and formatters only. Detailed revision/invalidation ledger: `/tmp/pi-rust-model-generator-proof/ledger.md`. Historical logs below predate the consumed-field audit. Current targeted proof and manual changed-code readability review are recorded at the end; no broad check/Clippy gate is claimed.

| Logs | Covered behavior/result |
|---|---|
|02/03,05/06,08/09|Initial RED/GREEN costs, provider transformations, HTTP/options and raw input. Later native type-policy tests supersede malformed-input acceptance/error-message cases only.|
|12,14,16–18|675 metadata cases, all-provider pipeline and full catalog RED/GREEN. Historical18 checkpoint lacked transactions; superseded by46.|
|20–22,25/26|Artifact modes/include compilation/baseline equality and CLI RED/GREEN. Historical21 rollback gap superseded by46; public failure boundaries added68.|
|28/29,31–33,37/38|HTTP/raw error and pre-error reasoning-state ordering; Mistral coercion/cache and lossless NVIDIA aliases; raw metadata relational/regex/ID behavior. Wrong-type V8 messages superseded by50–61.|
|40–44|Oracle U+2028 framing diagnosis; every UTF16 unit/scalar/contextual NVIDIA normalization GREEN.|
|35/36,46|Native transaction RED; post-swap shared reader prerequisite; GREEN eight IO fault boundaries plus normal modes/CLI. Shared model-data helpers integrated, not pending.|
|48|All four fetchers status/transport/JSON strict/default matrix, NVIDIA Map order, Qwen exact IDs, actual original automatic CLI catch.|
|50/51|Native generated-ID policy RED/GREEN14 cases at original outer use point; valid UTF16/numeric/nonfinite/unused-field regressions.|
|53/54|Copied Model emission policy RED/GREEN; stage validator precedence; malformed un-emitted data-only subset ignored.|
|56/57,59/61|Receiver policy RED/GREEN;59 exposed stale pre-policy assertions;61 GREEN7 after classifying false-provider catches/removing duplicate malformed cases, preserving direct UTF16 proof. Source cost/full workflows also GREEN59.|
|62/63,65/66|Native transport RED/GREEN; actual TCP/body failures and sanitized causes.65 reproduces terminated masking503;66 GREEN2 after status-before-body fix.|
|68/69,71|Artifact suite GREEN7 including public mkdir/writes; data-only fixture required generated `*_MODELS` import grammar, corrected and individually GREEN69.71 GREEN adds remove-boundary injected real directory-read failure, old public bytes unchanged and native data retained. Production unchanged.|

Format checks pass at coherent commit checkpoints, latest recorded in the proof directory. Logs prove their listed behavioral scope, not whole-module parity.

## Approved adaptations and discrepancies

- Native JSON catalogs and generated include-table glue replace TS generated shards (2026-10-20). Default generation updates glue; data-only preserves it. Runtime catalog APIs/current values remain unchanged.
- “Reject wrong field types (Recommended)” (2026-10-20): present consumed fields use declared types or actual operations/Model contract for `any`. Required missing fields without a source fallback reject at their use point. Preserve defaults/nullish/outer handling and filters before unused checks. No new finite/range/literal-domain constraints; unknown/unused fields and valid lone-surrogate strings remain valid. The consumed-field matrix below covers the full provider/metadata receiver audit; it is not a new eager schema prepass.
- ID failures first consumed by outer generation propagate under either strict setting. Copied Model checks run at emission, including JSON-only, outside fetch catches; existing staged validators retain priority. Un-emitted data-only provider values are not checked. Deferred reasoning errors remain after all three fetches. Native messages replace accidental V8 wrong-type behavior under the approved policy.
- “Allow native causes (Recommended)” (2026-10-20): consume shared structured name/message/cause, no prefix parsing or transport shim. Node top-level name/message/status/failure/log order remains required; native causes are sanitized. Shared dependency87b946c is integrated.
- “Omit JS frames (Recommended)” (2026-10-20) omits only JavaScript-specific frames, not error names/messages/own FS fields/order/exit. Full raw TS diagnostics retained separately.
- Classified source discrepancy: inherited object property names (`__proto__`, `constructor`, etc.) count as existing truthy models during source dedup and are omitted from the serialized model set. Native grouping preserves that observed set; no prototype mutation or silent source-bug fix.

## Remaining acceptance boundary

Implementation, source branch proofs and consumed-field validation are closed below. Coverage promotion follows the supplied completion brief; no workspace/check/Clippy gate is claimed. Public failures do not roll back native catalogs; remove injection uses real read-directory IO failure, not host permission changes. No live refresh, deployment, push or canonical merge performed by this slice.

## Final bounded consumed-field correction

`73-http-consumed-red.log` reproduces NVIDIA null root logged as successful empty catalog instead of source TypeError, and gateway accepting wrong declared tag/pricing fields. `74-http-consumed.log` GREEN7: null root uses the original NVIDIA catch; tool-tagged gateway models reject non-string tag entries, non-object pricing and non-string/non-number pricing values. Explicit nonarray data/tags defaults, skipped unused pricing, nullish rates, valid UTF16/unknown strings, outer IDs and emission checks stay in their source regions. Already-rejecting NVIDIA/OpenRouter receivers retain their existing TypeError messages; no message-only cleanup or metadata schema pass.

## Literal source-function/branch mapping

Line regions refer to the pinned `generate-models.ts`, not transpiled stack lines. Test aliases below name files under `crates/pi-ai/tests/`; each row states actual fixture evidence, not an inferred coverage percentage. All listed branches have native implementation; proof-only gaps G1–G3 are listed separately.

- **M**: `model_catalog_generator_metadata.rs`: `source_metadata_branches_merge_existing_fields_in_source_order` (675 provider/id/API fixtures), `source_raw_cache_coercion_and_ascii_google_patterns_match`.
- **D**: `model_catalog_generator_models_dev.rs`: `source_models_dev_every_provider_pipeline_matches_in_source_order`, `source_provider_alias_pricing_and_lossless_nvidia_boundaries_match`.
- **W**: `model_catalog_generator_workflow.rs`: full-merge workflow, invalid-ID boundary, valid-UTF16/nonfinite/unused-field tests.
- **R**: `model_catalog_generator_receivers.rs`: named receiver/default/filter/deferred-reasoning/gateway/NVIDIA tests; **H**: `model_catalog_generator_http.rs`; **F**: `model_catalog_generator_fetch.rs`.
- **A**: `model_catalog_generator_artifacts.rs`; **N**: `model_catalog_generator_native_http.rs`; **C**: `model_catalog_generator.rs`; **O**: `model_catalog_generator_options.rs`.

| Source symbol / exact region | Branches → concrete fixture proof |
|---|---|
|`readGeneratorOptions`36–78|O flags/conflicts/missing/unknown arguments; A real binary bad output mode. deleted cwd real binary/source differential closed81.|
|`withOpenAiLongContextPricing`391–405|W `gpt-5.4`, `gpt-5.5`, `gpt-5.6-terra` full generated cost bytes.|
|`mergeThinkingLevelMap`475–477|M existing `high:"custom"` retained/overwritten by matching metadata; W deferred effort merge.|
|`getModelKey`, `recordModelsDevReasoningOptions`481–489|D all-provider effort records; W prior OpenAI record retained before later null-model fetch failure; R deferred wrong options after three requests.|
|`supportsDirectReasoningEffort`, `applyModelsDevReasoningOptionMetadata`491–514|D/W OpenAI Responses, Anthropic and native compatibility paths; W models without records and unsupported direct-effort APIs remain unchanged. R deferred failures outside catches.|
|`getTogetherCompat`, `getTogetherThinkingLevelMap`516–533|D plain/no-tool/deprecated models; DeepSeek-R1, MiniMax-M2.7, GPT-OSS20b/120b, DeepSeek-V4-Pro.|
|`supportsOpenAiXhigh`, `supportsOpenAiMax`535–553|M plain negative, GPT5.4/5.5/5.6 positive; M seven rotating API identities, W Codex/GC/Azure clones.|
|`isGoogleThinkingApi`555–557|M Google generative/Vertex and five non-Google APIs; raw M both Google APIs.|
|`isAnthropicAdaptiveThinkingModel`, `isAnthropicTemperatureUnsupportedModel`559–587|M Claude Opus4.7/Sonnet4.6/Fable5/plain; W dash-spelled Opus/Sonnet4-6; D Copilot Claude API routing.|
|`mergeAnthropicMessagesCompat`619–621|M preserves `compat.extra`, adaptive/strict/temperature fields merged by sequential mutations.|
|`detectOpenAICompletionsCompat`623–716|M named provider branches/default provider metadata; D/W actual provider endpoints. custom-provider baseURL-only recognition closed79.|
|`isPlainEmptyObject`, `openAICompletionsCompatDelta`718–732|M default empty routing maps omitted and provider-specific nondefault fields retained; preexisting `compat.extra` survives.|
|`mergeOpenAICompletionsCompat`, `applyOpenAICompletionsCompatMetadata`734–745|M completions/non-completions and existing compat merge; empty compat omission closed79.|
|`applyStrictToolCompatMetadata`747–757|M OpenAI/Cloudflare Responses, Anthropic messages and nonmatching APIs; D/W generated provider compat.|
|`applyOpenAIGrammarToolCompatMetadata`776–781|M allowed provider/API GPT5 fixtures, plain/nonmatching providers/APIs; W GPT5 generated clone values.|
|`applyOpenAIToolSearchMetadata`783–795|M OpenAI/Codex IDs and nonmatches; W supplements/clone model bytes.|
|`applyOpenAIExplicitPromptCacheMetadata`800–807|M positive cache1; raw M zero, numeric-string, bool, array, coercion error and nonnumeric values; W real pricing corrections.|
|`isGemini3ProModel`, `isGemini3FlashModel`, `isGemma4Model`809–820|M Gemini3.1-pro/latest-flash/Gemma4; raw M uppercase/ASCII-digit vs Devanagari/Arabic-digit patterns.|
|`applyThinkingLevelMetadata`822–968|M GPT5 Responses/Copilot/off/xhigh/max/pro, Anthropic adaptive/temperature, Google, Groq Qwen, Kimi/Moonshot/Gateway/OpenRouter, Mercury2, GLM/Fireworks, OpenCode, AntLing, Copilot. W generated metadata pass order and clones.|
|`getAnthropicMessagesCompat`970–979|D Fireworks/Kimi/Xiaomi families, default no-compat models; M/W generated compat merging.|
|`getBedrockBaseUrl`981–985|D `eu.anthropic.claude` versus plain/US IDs.|
|`normalizeNvidiaModelId`987–989|Existing normalization differential enumerates every UTF16 unit/scalar and contextual casing/lone surrogates; F normalized-key overwrite and insertion order.|
|`roundCost`991–993|C/H decimal per-token prices, trailing numeric strings, bad numeric strings, overflow/nonfinite paths.|
|`getModelsDevCost`995–1017|C plain/context/noncontext/missing-size tiers; R wrong tier collection/entry types and unknown-tier unused values.|
|`fetchNvidiaNimModelIds`1019–1039|F absent/null/empty data, normalized aliases, duplicates, strict/default status/transport/JSON; R null response root fails with source log order; malformed ID/data receivers already throw TypeError.|
|`fetchOpenRouterModels`1041–1101|H tool filter, string/image modalities, pricing parse/rounding/defaults, top-provider fallback, raw UTF16, null/shape/coercion failures; F strict/default matrix; W outer ID and A copied-name emission failure regions; N actual503 broken-body precedence.|
|`fetchAiGatewayModels`1103–1161|H/R explicit nonarray data/tags fallbacks, tool filter, vision/reasoning, pricing finite/numeric/nullish behavior; R declared consumed tag/pricing rejection after filter; W ID and A emission boundaries.|
|`processBasetenModels`1163–1255|C GLM/toggle/effort/deprecated fixtures; R receiver failures, unknown effort strings and unused budget values/short-circuits; raw UTF16 keys/names proof.|
|`processFireworksModels`1257–1333|C/D tool filter, GLM-5p2/Kimi-K3/API/compat; R invalid consumed modalities and skipped unused fields.|
|`loadModelsDevData`1335–1344/2296–2303|D all-provider request order; F strict/default status/transport/JSON; W/R pre-error records and provider/default catch boundaries.|
|Bedrock1345–1385, Anthropic1386–1412|D tool filtering; Bedrock Jamba/Mistral/profile exclusions and EU/default region; plain source cost/limits/modalities.|
|Google1413–1446, Vertex1447–1488|D latest aliases and canonical versions, Vertex non-Gemini/preview exclusion and Gemini2.5 cache correction.|
|OpenAI1489–1517, Groq1518–1544, Cerebras1545–1571|D tool models/defaults, OpenAI GPT5.6 excluded alias; W OpenAI corrections/supplements.|
|Cloudflare Workers1572–1599/Gateway1600–1656, xAI1657–1685|D OpenAI/Anthropic/Workers/unknown/no-slash gateway IDs; xAI Grok4.5 Responses; W excluded xAI builtins.|
|ZAI1686–1729, Mistral1730–1756, HuggingFace1757–1788|D GLM5.2/4.5 variants and streaming exclusions; alias/pricing test trailing/hex/false-cache Mistral; plain HF model.|
|NVIDIA1789–1823, Together1824–1858|D NVIDIA input/output text + live map; F duplicate IDs; D Together family effort/toggle/fixed maps and deprecation; alias test source-key precedence.|
|OpenCode/Go1859–1966|D npm OpenAI/Anthropic/Google/Alibaba/openai-compatible; Grok/Kimi/MiniMax/Qwen/DeepSeek routing; W Spark exclusion.|
|Copilot1967–2020, MiniMax2021–2055|D Claude/GPT/Grok/OSWE/MAI routing and subscription costs/headers; W selected Copilot tier correction/MiniMax allowlist.|
|Kimi2056–2102, Moonshot2103–2158|D canonical/alias/highspeed/thinking/K3, both Moonshot regions; W missing-alias supplements/corrections.|
|Xiaomi2159–2215, Alibaba2216–2295|D all Xiaomi endpoints; D/F Qwen international/CN/individual allowlist, preview exclusion, strict exact-ID failure/success.|
|`generateModels`2305–2397|W sequential load/filter and model pricing/context/cache overrides; pre-error reasoning state; A immediate logs.|
|supplements2398–2584, Codex/GC2585–2685|W empty and mixed full catalogs compare every supplement/clone emitted value, preserving existing model precedence.|
|Mistral/OpenRouter aliases2686–2752, metadata/clones2753–2790|W default supplements, duplicate IDs, deferred reasoning and Azure clone bytes; M six metadata passes.|
|dedup/sorting2791–2815|W first duplicate wins; inherited names omitted; UTF16/numeric-looking keys preserve source enumeration/lexical order.|
|data-only/grouping2816–2841|A empty/duplicate/missing-provider guards after fetches; selected subset; grouped private vs flat public modes.|
|native transaction2843–2945|A actual IO eight stage/validate/include/rename/post-validate/cleanup boundaries, previous-byte restoration and source FS metadata. Native include glue replaces TS shards by approval.|
|public output2947–2957|A reset/mkdir/model-list/provider-list/provider-file faults after native commit, exact remaining files and logs; separate rollback domain.|
|statistics2959–2970, automatic catch2974–2977|A source/native immediate log/statistics order and real invalid-argument subprocess; F original automatic source CLI success/strict failure/exit1.|

### Closed final proof-only cases

| ID | Exact source branch | Implementation / evidence | Status |
|---|---|---|---|
|G1|623–717 URL-only compat recognition with a custom provider: ZAI2 endpoints, Together2 endpoints, Moonshot, OpenRouter, Cloudflare Workers/Gateway, NVIDIA, AntLing, mixed-case DeepSeek, Cerebras, xAI, Chutes, OpenCode|`custom_provider_url_routes_and_empty_compat_deletion_match_source`: 16 URLs including standard default × 3 IDs × absent/empty/nonempty compat = 144 actual-source cases.|GREEN79; no production change.|
|G2|738–745 standard completions model ends with empty compat, then deletes the field|Same named metadata test checks omitted compat for standard completions with absent/empty initial compat, alongside source bytes.|GREEN79; no production change.|
|G3|64–70 relative json-output resolution after OS current directory has been removed|`deleted_cwd_cli_failure_matches_original_os_error_before_fetching`: original module runs after real chdir+rmdir; actual binary starts in deleted cwd via held FD. Shared `removed_current_directory_error_has_node_message_and_pathless_own_fields` checks record.|RED80; GREEN81 native/source message/errno/code/syscall/no-fetch exit1; shared FS GREEN82.|

NonUTF8 OS argv cannot be represented by TS `process.argv` strings and is an explicit typed/native boundary exclusion, not an unimplemented JavaScript input class. The later consumed-field audit below supersedes the bounded-correction completion claim; the missing boolean/filter/routing/control checks are now implemented and proven. Shared node_fs change invalidates only earlier filesystem foundation proof; targeted82 replaces it. Number/date/JSON code and proof scope remain unchanged.

## Complete consumed-field matrix (2026-10-20)

References: **G** = pinned `packages/ai/scripts/generate-models.ts`; **T** = pinned `packages/ai/src/types.ts`; **Q** = pinned `packages/ai/scripts/models-dev-reasoning-options.ts`. Native paths under `crates/pi-ai/src/`: **N** = `model_catalog_generator.rs`; **R/E/D/M/C/A** = `model_catalog_generator/{receiver_types,emission_types,models_dev,metadata,catalog,artifacts}.rs`. Lines refer to production revision `9077dd8a`.

Validation status is a boundary, not a promise of eager rejection:

- **USE**: native TypeError at the consumed receiver/filter/conversion, inside the original fetch catch where applicable. Optional missing/nullish fields retain source defaults; no new range, finite-number or literal-domain restriction.
- **EMIT**: copied fields use the approved actual-emission checks, after defaults/overrides and existing staged validators, outside fetch catches. Public JSON checks the full output; data-only checks only emitted subsets. Clarified by parent during this task: do not move copied scalar checks into fetch catches, even when defaults/overrides erase wrong types.
- **SOURCE**: explicit source operation/default/coercion retained, not an unfinished validation gap. OpenRouter `any` parseFloat inputs and raw metadata cache comparisons retain their existing coercion contract. Generated intermediate fields are constructor-typed, then emission-checked; provider-supplied unknown fields are not copied into metadata.

The governing approval remains “present consumed fields use declared types or actual operations/Model contract for `any`” and “Required missing fields without a source fallback reject at their use point.” Parent confirmed that the latter applies to consumed reasoning-option `type`/effort `values`; skipped controls remain unvalidated. Optional controls are not converted into required fields. Native error shape is `GeneratorError { name: "TypeError", message: <exact receiver message>, details: None }`; copied output messages retain `Generated model "provider"["id"].field must be a <type>` (object/array use `an`), and generated IDs retain `Generated model.id must be a string`. Existing accidental V8 messages for already-rejecting HTTP/NVIDIA receivers remain unchanged.

### models.dev receivers

All common-model rows apply to every provider region listed in the literal branch map, including Google canonical aliases, Baseten/Fireworks, Together aliases, subscription endpoints, OpenCode/Go and Qwen. `consumed_reasoning_rejects_strings_in_every_provider_family` exercises all **33** source provider keys/aliases with `reasoning:"yes"`, strict rejection and default-fetch discard. Exceptions are explicit below.

| Input field / receiver | TS declaration | TS consumption | Rust consumption / check | Validation status / proof |
|---|---|---|---|---|
| catalog root | G124 | G1339,1345 | D182 | USE object; existing receiver tests |
| selected catalog provider object | G120,124 | G1345–2295; aliases G1824–1826 | D62,183; R10–18 | USE optional object; source provider selection/defaults retained |
| provider.models | G121 | G1164,1214,1258,1286,1345–2295 | R12–18 | USE object only after source falsy guard; missing/falsy models still empty |
| models entry object | G83 | G1215,1287,1348–2290 | R20–21,31; N181,291 | USE object; null/primitive rejected in receiver suite |
| model ID (record key) | G121 string keys; G84 `model.id` unused | G1214,1286,1347–2290 | R18; D62; N148 | SOURCE lossless JsString keys; raw UTF-16 differential |
| name | G85 | G1241,1294,1366–2285 | N152; D341 | EMIT E58–59; alias names use original model, not canonical-source name |
| tool_call | G86 | G1287,1349–2254 | R30–33; D109–115; N292 | USE boolean before tool filter; new filter tests; Baseten never consumes it |
| structured_output | G87 | G1380 | D301–303 | USE boolean only on selected Bedrock models; skipped Jamba/profile IDs leave it unused |
| reasoning | G88 | G1217,1296,1370–2276 | N147,154; R39–49 | USE boolean; all 33 provider keys, direct Baseten/Fireworks; absent/null remains false |
| reasoning K3 short-circuit | G88 | G2088,2143 | R40–46; D683–685,722 | SOURCE forced true for Kimi `k3` and Moonshot `kimi-k3`; malformed unused reasoning accepted |
| status | G90 | G1215,1830,1876,1972 | R35–37; N182; D467,514,584 | USE string only in Baseten/Together/OpenCode/Go/Copilot; unknown strings accepted |
| limit object | G91 | G1251–1252,1305–1306,1378–2285 | N146 | USE optional object; receiver tests |
| limit.context | G92 | G1251,1305,1378–2284 | N165–166; D603 | EMIT E69–70 after source default/overrides; copied-scalar and artifact tests |
| limit.output | G93 | G1252,1306,1379–2285 | N169–170; D608 | EMIT E69–70 after source defaults/overrides |
| cost object | G95 | G1248–1250,1300–1304,1372–2282 | N145,138–139; D396–408,599,697,726 | USE optional object; copied numbers remain EMIT; Mistral derived cache follows source numeric coercion |
| cost.input | G96 | G1247,1300,1373–2279 | N136; D396,828 | EMIT E36–40; source false/empty defaults retained |
| cost.output | G97 | G1248,1301,1374–2280 | N137; D829 | EMIT E36–40 |
| cost.cache_read | G98 | G1249,1302,1375–2281 | N138; D397,832 | EMIT E36–40; Mistral nullish default and implied subscription rates retained |
| cost.cache_write | G99 | G1250,1303,1376–2282 | N139; D836 | EMIT E36–40 |
| cost.tiers | G100–109 | G996–1008 only via G2001 Copilot | N94–128; D599 | USE optional array of objects; unused for other providers |
| cost.tiers[] object | G100–109 | G997 | N102–107 | USE object; receiver tests |
| cost.tiers[].tier | G105 | G997–998 | N108–109 | USE optional object; receiver tests |
| cost.tiers[].tier.type | G106 | G998 | N110–111 | USE string; new `type:3` rejection; unknown strings skip unused tier rates/size |
| cost.tiers[].tier.size | G107 | G998,1001 | N117–120 | EMIT E80–84 as inputTokensAbove, only for context tiers with a size; no new numeric range |
| cost.tiers[].input | G101 | G1002 | N121 via plain_cost N136 | EMIT E78–79; noncontext/missing-size tier rates unused |
| cost.tiers[].output | G102 | G1003 | N121 via N137 | EMIT E78–79 |
| cost.tiers[].cache_read | G103 | G1004 | N121 via N138 | EMIT E78–79 |
| cost.tiers[].cache_write | G104 | G1005 | N121 via N139 | EMIT E78–79 |
| modalities object | G111 | G1245,1290,1371–2277 | R71–74; N143 | USE optional object; no unused output prepass |
| modalities.input / entries | G112 | G1245,1290,1371–2277; NVIDIA G1794,1809 | R74–84; D431–433 | USE array of strings; NVIDIA validates before text filter, other models after provider/tool filters |
| modalities.output / entries | G113 | NVIDIA G1795 only | D435–438; R71–84 | USE array of strings only after input includes text; wrong output ignored when input excludes text |
| model.provider object | G115 | OpenCode/Go G1878 | D517; R51–53 | USE optional object; unrelated providers leave it unconsumed |
| model.provider.npm | G116 | G1878–1893 | R54–55; D521–530 | USE string; unknown package strings accepted, no enum/domain validation |
| reasoning_options | G89 | G487–489,1218–1222,1237,511–512 | D122–130; N187–207,251; R115–123; C191–195 | USE optional array when consumed, not when merely recorded; unsupported direct-effort APIs leave records unused; deferred failures remain after all 3 fetches |
| reasoning_options[] object | Q3–9 | G1220–1221; Q22 | R86–91 | USE object at actual scan/effort-map traversal; existing invalid-entry tests |
| reasoning_options[].type | Q4,6,9 | G1220–1221; Q22 | R92–98 | USE required string; missing/null/numeric reject exact `models.dev reasoning option.type must be a string`; unknown strings accepted |
| effort.values | Q7 | Q22 | N255–263 | USE required array for effort controls actually mapped; missing/null/scalar rejects `models.dev effort values must be an array` |
| effort.values[] | Q7 | Q25–31 | N264–282 | USE string or null; unknown strings/UTF-16 accepted and ignored by membership map; no literal-domain restriction |

**Deliberately unconsumed:** `ModelsDevModel.id` (G84), Baseten `tool_call`, non-Bedrock `structured_output`, non-routing `provider`, non-NVIDIA `modalities.output`, non-Copilot `cost.tiers`, budget-token `min`/`max` (Q9), unknown option-kind payloads, and arbitrary extra fields. Google aliases read canonical reasoning/modalities/cost/limit but original names; Vertex never reads cache_write and never reads cache_read for `gemini-2.5-flash` (G1467,1479–1480). Baseten toggle/GLM maps bypass effort values; `.some` stops scanning once its match is found. These acceptance paths are exercised by new ignored-field/required-control tests plus existing receiver/all-provider/source-workflow fixtures. None are claimed as missing validation.

### HTTP provider receivers

OpenRouter JSON is `any` (G1046), not an invented TS interface. `Model<any>` fields use T809–830 at output. Gateway root is `any` (G1108) with the explicit `AiGatewayModel[]` array cast/default (G1121). Explicit nonarray gateway data/tags defaults remain accepted.

| Input field / receiver | TS declaration | TS consumption | Rust consumption / check | Validation status / proof |
|---|---|---|---|---|
| OpenRouter root.data / iterable entries | G1046 `any` | G1050 | N613–630 | SOURCE existing null/noniterable TypeErrors; empty string iteration/default behavior retained; HTTP raw/error suite |
| OpenRouter model.supported_parameters | G1046 `any` | G1052,1079 | N631–642 | USE actual includes receiver (string/array/nullish); existing native message retained |
| supported_parameters[] | G1046 `any` | G1052,1079 string membership | R64,57–61 after N642 | USE string entries for selected tool models; new wrong-entry rejection |
| architecture | G1046 `any` | G1062 | R65–66 | USE optional object after tool filter; new malformed-receiver fixture |
| architecture.modality / entries | G1046 `any` | G1062 | N650–658; R67 | USE actual string/array/nullish includes receiver, string array entries; valid strings remain accepted |
| pricing object | G1046 `any` | G1067–1070 | R68 | USE optional object after tool filter; new array receiver fixture |
| pricing.prompt | G1046 `any` | G1067 | N667,671; pricing_number N599–612 | SOURCE parseFloat after falsy default; emitted numeric result E39; existing numeric/coercion tests |
| pricing.completion | G1046 `any` | G1068 | N668,671 | SOURCE same numeric parsing |
| pricing.input_cache_read | G1046 `any` | G1069 | N669,671 | SOURCE same numeric parsing |
| pricing.input_cache_write | G1046 `any` | G1070 | N670–671 | SOURCE same numeric parsing |
| top_provider | G1046 `any` | G1072,1089 | R69 | USE optional object after tool filter; new false receiver fixture |
| top_provider.context_length | G1046 `any`; output T822 | G1072 | N729–730 | EMIT E69–70 after source fallback; fallback model.context_length not consumed when top value truthy |
| model.context_length | G1046 `any`; output T822 | G1072 | N730 | EMIT E69–70 |
| top_provider.max_completion_tokens | G1046 `any`; output T825 | G1089 | N743 | EMIT E69–70 |
| model.id | G1046 `any`; output T809 | G1056–1058,1074 | N675–688; C188,214 | USE outer require_model_id N81–86, not moved into fetch catch; workflow ID matrix |
| model.name | G1046 `any`; output T810 | G1075 | N675–688 | EMIT E58–59, no added fallback; artifact emission matrix |
| Gateway root.data | G1108 `any` | G1121 | N619–624 | SOURCE explicit Array.isArray default; receiver/HTTP suite |
| Gateway model.tags | G135 | G1123–1128,1148 | N632–642 | SOURCE explicit nonarray default before tool filter |
| Gateway tags[] | G135 | G1125,1128,1148 | R100–101 | USE string entries only for selected tool models; existing receiver policy tests |
| Gateway pricing | G136 | G1132–1135 | R102–103 | USE optional object after tool filter; existing receiver tests |
| Gateway pricing.input | G137 | G1132 | R104–112; N667–672 | USE string or number before toNumber, nullish default retained |
| Gateway pricing.output | G138 | G1133 | R104–112; N668–672 | USE string or number, same rule |
| Gateway pricing.input_cache_read | G139 | G1134 | R104–112; N669–672 | USE string or number, same rule |
| Gateway pricing.input_cache_write | G140 | G1135 | R104–112; N670–672 | USE string or number, same rule |
| Gateway model.id | G131 | G1138–1139 | N675–688; C188,214 | USE outer generated-ID rule; not validated inside fetch catch |
| Gateway model.name | G132 | G1139 | N677–683 | EMIT E58–59 after source name-or-ID fallback |
| Gateway context_window | G133 | G1156 | N725–726 | EMIT E69–70 after source default |
| Gateway max_tokens | G134 | G1157 | N738–739 | EMIT E69–70 after source default |
| NVIDIA root.data / entries | G1024 optional NvidiaNimModelListItem[] | G1027,1032 | D136–150 | USE existing null/nonarray rejection; optional empty-array fallback retained |
| NVIDIA entry.id | G127 | G1028–1029; normalization G987–989 | D151–157 | USE required string before normalized-key insertion; existing native TypeError preserved; full UTF-16/scalar oracle |

Unused HTTP fields (including gateway architecture, OpenRouter tags and metadata beyond the listed receivers) remain untouched. Tool-skipped models leave pricing/name/limits/architecture unused. Pricing accepts nonfinite numbers where source did, gateway toNumber normalizes them to zero; no range/finite restriction was added.

### Generated Model and metadata receivers

These fields are generated by the preceding transformations, constant supplements, or native data-only hydration. They are not arbitrary raw provider metadata bags. Metadata receives generated `api`/`provider`/`baseUrl` constants and validated IDs; it never copies a raw provider's compat object. This is why no new metadata schema prepass or nested compatibility-domain restrictions are needed.

| Model field / receiver | TS declaration | TS consumption | Rust consumption / check | Validation status / proof |
|---|---|---|---|---|
| id | T809 | G481–482,535–553,623–716,822–968,2305–2815 | N81–86; M15,57,179; C188,214,478 | USE required string at original outer generation point; UTF-16 preserved |
| name | T810 | clone/spread G2585–2685,2770; serialize G2814–2957 | C clone/supplement pipeline; E58–59 | EMIT string; copied-name artifact fixtures |
| api | T811 | G491–505,555–557,623–968,2766,2783–2789,2827 | M145,177,350; C350–365; A168,180; E58–59 | SOURCE constructor constants / EMIT string; existing metadata differential |
| provider | T812 | G482,623–968,2317–2397,2766,2795–2800 | M56,178,351; C187,213; E58–59 | SOURCE constructor strings / EMIT string; provider sorting/dedup/workflow tests |
| baseUrl | T813 | G623–716 | M57; E58–59 | SOURCE generated constants / EMIT string; URL-only routing differential |
| reasoning | T814 | G947,2963 | M321; C209; E61 | USE validated raw reasoning above, then SOURCE bool constants/truthiness; EMIT boolean |
| input / input[] | T820 | generated/spread/serialize G2585–2957 | N155–163; E62–67 | SOURCE modality-derived strings / EMIT array of strings |
| cost / input / output / cacheRead / cacheWrite | T790–795,821 | G391–405,800–807,2358–2367,2772–2775 | C47–65,163–172,452; M393; E36–40,72–73 | SOURCE existing arithmetic/cache comparison coercions retained; EMIT object/numbers; raw-cache metadata differential |
| cost.tiers / entries / inputTokensAbove / rates | T797–805 | G391–405; serialization G2843–2957 | C47–67; E74–85 | SOURCE constructed context tier / EMIT arrays, objects, numeric threshold/rates |
| contextWindow | T822 | G2324–2376,2777 | C176–178,389–489; E69–70 | EMIT number after overrides; full workflow/artifact fixtures |
| maxTokens | T825 | G2348–2383 | C389–489; E69–70 | EMIT number after overrides |
| headers / string values | T828 | static headers G144–153,1807,1983–1994,2083; serialize | D325,610–616,675–678; E88–89 | SOURCE constants / EMIT object of strings; all-provider/full workflow proof |
| thinkingLevelMap / nullable string values | T819 | G475–477,512–513,822–968 | C197–204; M4–11,176–338; E91–92 | USE validated effort controls; SOURCE generated maps merged in source order / EMIT nullable string map |
| compat object / spread own values | T830–838 | G475–505,619–621,734–745,2391,2783–2789 | C26–34,362–364; M4–11,163–173; E94–95 | SOURCE generated compat only; preserve existing keys, EMIT optional object; 675-case metadata differential |
| compat.forceAdaptiveThinking | T684 | G492 | C350 | SOURCE generated boolean constructor (`true`) or absent; no untrusted raw copy |
| compat.thinkingFormat | T581–590 | G506 | C365 | SOURCE detected/generated string or absent; unknown string domains not restricted |
| compat.supportsReasoningEffort | T565 | G506 | C365 | SOURCE detected/generated boolean truthiness; existing metadata/source-workflow proof |
| autoCompactionThreshold | T824 | serialized Model contract G2814–2957 | E97–98 | EMIT optional number; no provider receiver consumes this raw field |

The metadata helpers' other compatibility members are **writes**, not consumed input fields: strict/tool-search/grammar/cache/temperature/session-affinity flags and routing maps come from native equivalents of source literals, and are covered by the existing source-metadata differential. Merge iterates existing generated object entries without assigning new meaning or validating unknown values. No provider-supplied compatibility field is promoted into a separate input schema.

### Proof ledger and result

Detailed commands, revisions, invalidation reasons and complete stdout/stderr: `/tmp/pi-rust-models-consumed-validation/ledger.md`, logs `01`–`14`.

| Scope | Evidence / result |
|---|---|
| Initial consumed validation RED | `02-red-fixtures.log`: 4 failed, 2 passed; rejected-filter/routing/collection/boolean guards missing. `01` was a fixture calibration failure (mandatory strict Qwen IDs absent), not counted as behavioral RED. |
| Source-order RED | `05-order-red.log`: 1 failed, 1 passed; Baseten reported options before earlier reasoning use. |
| Required controls RED | `09-required-controls-red.log`: 2 failed, 1 passed; absent required controls accepted in direct and deferred paths. |
| GREEN | `08-all-generator-targets.log`: all 11 pre-existing integration targets once, plus 2 new targets, 44 passed. Required-control change invalidated only option-consuming targets: `13-option-path-regression.log` replaces those with 33 passes across 9 targets (including the third new target); unaffected HTTP/fetch/options/metadata/native-HTTP proof from `08` retained. Unique current integration proof: **47 tests across 14 targets**. |
| Internal normalization | `14-normalization.log`: 1 passed, 23 unrelated lib tests filtered; every UTF-16 unit/scalar/context normalization against pinned TS. Total unique current proof: **48 tests**. |
| Mutation | `07-mutation.log`: combined scalar/modality/router-entry guard removal caused 4 expected failures; `12-required-controls-mutation.log`: restoring permissive absent controls caused 2 expected failures. **2/2 mutations killed**, restored exactly before final proof; neither committed. |
| Formatting/readability | `03`, `06`, `10`: cargo fmt and fmt --check passed before implementation commits. Manual review of changed Rust: new helpers remain short, explicit, pure, with named receiver/type errors; no new warning suppressions, dependencies, fallback paths or placeholders. No check/Clippy/workspace suites, live catalog refresh, network provider calls, delegation, merge or push. |

No outstanding consumed-field validation row remains under the governing USE/EMIT/SOURCE split. Historical [partial-row probe](../partial-rows-verification.md#7-generate-modelsts--gap) is superseded by the all-provider `reasoning:"yes"` fixture and the complete matrix above, not by test-count inference alone.
