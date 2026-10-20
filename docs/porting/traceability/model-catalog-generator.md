# Model catalog generator

Source: pinned `packages/ai/scripts/generate-models.ts` ([source lock](../source-lock.json)). Native implementation: `crates/pi-ai/src/model_catalog_generator.rs` and its `metadata`, `models_dev`, `catalog`, `artifacts`, `emission_types`, `receiver_types` modules; explicit maintenance entrypoint `crates/pi-ai/src/bin/generate-models.rs`. Status: **partial; no independent whole-module completion gate**. Baseline provider JSON/manifests remain unchanged.

## Current capability and proof matrix

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
| copied Model fields, consumed provider/model/cost/modalities/reasoning receivers | native TypeErrors at actual use/emission points, not a schema prepass | ID14 cases; emission malformed name/cost/context/token matrix; receivers/defaults/filter/strict regions | bounded consumed-field corrections complete74/78; no new schema audit authorized |
| grouped private/flat public artifacts; manifest/hash/timestamp/pretty; include glue | `artifacts.rs`; shared model-data helpers; native include table | four artifact modes; source bytes; compile+execute generated includes; every baseline embedded value unchanged | native output adaptation approved; TS shard syntax deliberately not emitted |
| data-only import guards and selected subset | native include-table reader; shared validators | empty/duplicate/missing-provider guards after three fetches; existing files unchanged; selected subset ignores malformed un-emitted Copilot value | arbitrary include syntax excluded from generated-file grammar |
| staged native transaction and rollback | actual local IO seam `CatalogFileSystem`; shared Node FS records | eight stage-write/validation/include-write/rename/post-manifest/post-provider/post-include/cleanup fault boundaries, previous valid bytes restored and staging checked | rollback domain is native data/include only; public output is separate |
| public reset/mkdir/three write boundaries | output removed/recreated after native commit | five public failure boundaries preserve name/message/errno/code/syscall, remaining files, native committed bytes, cleanup and log order | remove-boundary fixture injects real directory-read EISDIR, not host permission changes; arbitrary host FS errors not enumerated |
| original automatic CLI catch, immediate logs, stats, error exit | `run_generator_cli`, maintenance binary | original automatic CLI fixture success/strict failure and native log/error ordering; real invalid-argument subprocess | successful native subprocess generation intentionally not run against live catalogs |
| native connection/body failures and status-before-body ordering | shared `default_fetch`, `read_body_text`; structured `CallbackError` fields | actual refused connection and premature body vs Node native fetch; actual generator503/truncated body differential | only native causes and JS-specific frames differ by approval |

## Original-source oracle

`oracle/model-catalog-generator-oracle.ts` transpiles actual pinned source with an AST exposure pass; private function bodies are not cloned. Whole generation uses independent HTTP fixtures and temporary real filesystem roots. The original automatic `generateModels().catch(...)` is retained for CLI cases. Native-network fixtures accept explicitly supplied HTTP127.0.0.1 URLs only. Byte-LF framing preserves literal U+2028. Full source diagnostics are retained in `/tmp/pi-rust-model-generator-proof/ts-diagnostics-<pid>.jsonl` and fault-specific JSON artifacts.

## Development proof ledger

Commands use argv through Pyrun; focused development tests and formatters only. Detailed revision/invalidation ledger: `/tmp/pi-rust-model-generator-proof/ledger.md`. No broad check/clippy/readability/coverage/independent acceptance run by this implementer.

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
- “Reject wrong field types (Recommended)” (2026-10-20): present consumed fields use declared types or actual operations/Model contract for `any`. Required missing fields without a source fallback reject at their use point. Preserve defaults/nullish/outer handling and filters before unused checks. No new finite/range/literal-domain constraints; unknown/unused fields and valid lone-surrogate strings remain valid. Implemented IDs, emission and receiver guards do not imply every malformed response branch has been audited.
- ID failures first consumed by outer generation propagate under either strict setting. Copied Model checks run at emission, including JSON-only, outside fetch catches; existing staged validators retain priority. Un-emitted data-only provider values are not checked. Deferred reasoning errors remain after all three fetches. Native messages replace accidental V8 wrong-type behavior under the approved policy.
- “Allow native causes (Recommended)” (2026-10-20): consume shared structured name/message/cause, no prefix parsing or transport shim. Node top-level name/message/status/failure/log order remains required; native causes are sanitized. Shared dependency87b946c is integrated.
- “Omit JS frames (Recommended)” (2026-10-20) omits only JavaScript-specific frames, not error names/messages/own FS fields/order/exit. Full raw TS diagnostics retained separately.
- Classified source discrepancy: inherited object property names (`__proto__`, `constructor`, etc.) count as existing truthy models during source dedup and are omitted from the serialized model set. Native grouping preserves that observed set; no prototype mutation or silent source-bug fix.

## Remaining acceptance boundary

Fixed module implementation and identified proof boundaries are closed below. Independent verification owns completion/coverage promotion. Public failures do not roll back native catalogs; remove injection uses real read-directory IO failure, not host permission changes. No live refresh, deployment, push or canonical merge performed by this slice.

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

NonUTF8 OS argv cannot be represented by TS `process.argv` strings and is an explicit typed/native boundary exclusion, not an unimplemented JavaScript input class. No additional response/metadata schema requirement or implementation defect was demonstrated after bounded corrections. Only independent acceptance/coverage promotion remains; no new audit is implied. Shared node_fs change invalidates only earlier filesystem foundation proof; targeted82 replaces it. Number/date/JSON code and proof scope remain unchanged.
