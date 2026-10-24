# Frozen chat literal catalogs

Source: [common pin](../source-lock.json), `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d` (read-only). Contract: [chat catalogs](../../specs/chat-catalogs.md).

Rust: `providers/models_data.rs`, `model_catalogs_generated.rs`, and exactly 36 `data/providers/*.json` records. Source `as const` data is embedded; returned native owned copies do not claim mutable JS alias/reference semantics. No factory imports or calls in the loader. The reader now uses lossless `JsValue` parsing and Model deserialization: human names retain exact UTF16, opaque IDs reject unpaired units explicitly. The generator/reader boundary is separately proved in [generator traceability](model-catalog-generator.md).

## Symbols and branches

| Export/branch | Native representation | Focused proof |
|---|---|---|
| `models.generated.ts::MODELS` | ordered static include table | every_literal_value_order_and_lookup_matches_source: every provider key, model key/value, absent key/provider; development RED → GREEN (all providers/records) |
| Every literal model field, omission, nested map/array | existing Model, no shared-type changes | every_source_record_roundtrips_native_model_without_field_loss: all 1058 values, GREEN |
| Literal GC/Team vs operational Codex aliases | independent literal records vs unchanged default-loader/rewrite factory | codex_factory_aliases_use_default_while_literal_aliases_remain_distinct_and_fresh: actual source values and independent fresh native copies; RED → GREEN |

All shard exports map to the same direct loader and both full-value tests above, not separate provider factory proofs:

| Source shard | Export | Records |
|---|---|---:|
| `amazon-bedrock.models.ts` | `AMAZON_BEDROCK_MODELS` | 106 |
| `ant-ling.models.ts` | `ANT_LING_MODELS` | 3 |
| `anthropic.models.ts` | `ANTHROPIC_MODELS` | 19 |
| `cerebras.models.ts` | `CEREBRAS_MODELS` | 3 |
| `cloudflare-ai-gateway.models.ts` | `CLOUDFLARE_AI_GATEWAY_MODELS` | 38 |
| `cloudflare-workers-ai.models.ts` | `CLOUDFLARE_WORKERS_AI_MODELS` | 13 |
| `deepseek.models.ts` | `DEEPSEEK_MODELS` | 2 |
| `fireworks.models.ts` | `FIREWORKS_MODELS` | 16 |
| `github-copilot.models.ts` | `GITHUB_COPILOT_MODELS` | 28 |
| `google.models.ts` | `GOOGLE_MODELS` | 17 |
| `google-vertex.models.ts` | `GOOGLE_VERTEX_MODELS` | 11 |
| `groq.models.ts` | `GROQ_MODELS` | 7 |
| `huggingface.models.ts` | `HUGGINGFACE_MODELS` | 49 |
| `kimi-coding.models.ts` | `KIMI_CODING_MODELS` | 3 |
| `minimax.models.ts` | `MINIMAX_MODELS` | 3 |
| `minimax-cn.models.ts` | `MINIMAX_CN_MODELS` | 3 |
| `mistral.models.ts` | `MISTRAL_MODELS` | 30 |
| `moonshotai.models.ts` | `MOONSHOTAI_MODELS` | 9 |
| `moonshotai-cn.models.ts` | `MOONSHOTAI_CN_MODELS` | 9 |
| `nvidia.models.ts` | `NVIDIA_MODELS` | 20 |
| `openai.models.ts` | `OPENAI_MODELS` | 47 |
| `openai-codex.models.ts` | `OPENAI_CODEX_MODELS` | 10 |
| `openai-codex-gc.models.ts` | `OPENAI_CODEX_GC_MODELS` | 10 |
| `openai-codex-team.models.ts` | `OPENAI_CODEX_TEAM_MODELS` | 10 |
| `opencode.models.ts` | `OPENCODE_MODELS` | 54 |
| `opencode-go.models.ts` | `OPENCODE_GO_MODELS` | 13 |
| `openrouter.models.ts` | `OPENROUTER_MODELS` | 270 |
| `together.models.ts` | `TOGETHER_MODELS` | 20 |
| `vercel-ai-gateway.models.ts` | `VERCEL_AI_GATEWAY_MODELS` | 194 |
| `xai.models.ts` | `XAI_MODELS` | 8 |
| `xiaomi.models.ts` | `XIAOMI_MODELS` | 6 |
| `xiaomi-token-plan-ams.models.ts` | `XIAOMI_TOKEN_PLAN_AMS_MODELS` | 5 |
| `xiaomi-token-plan-cn.models.ts` | `XIAOMI_TOKEN_PLAN_CN_MODELS` | 5 |
| `xiaomi-token-plan-sgp.models.ts` | `XIAOMI_TOKEN_PLAN_SGP_MODELS` | 5 |
| `zai.models.ts` | `ZAI_MODELS` | 6 |
| `zai-coding-cn.models.ts` | `ZAI_CODING_CN_MODELS` | 6 |

## Provenance and limits

`data/providers/.manifest.json` records source export, common commit, SHA-256 of all 37 actual imported source files, and every native JSON payload. `oracle/chat-catalog-oracle.ts` imports `models.generated.ts` directly via locked TsOracle under mandatory Cargo isolation; explicit export uses `python3 scripts/test-sandbox.py -- <pinned-tsx> ... --export`. No generator, provider implementation, network source, or invented catalog fixture participates.

At this pin GC/Team literal values equal default+provider rewrite; the literals remain separately keyed/cataloged. No unequal-pricing claim is made.

All 1058 source keys equal model.id. IDs repeat across providers (774 globally distinct); each provider's full ordered record is compared without global deduplication. Runtime exported records have unique keys by JavaScript object semantics. No old grouped flatten/dedup contract is carried into the direct loader.

Deleted old-only azure-openai-responses, baseten, qwen-token-plan, qwen-token-plan-cn, qwen-token-plan-individual artifacts; added Team. Public data-URL helper deleted (only in-repo test consumers). Timestamp API was staged for model_runtime; that consumer is now removed and the timestamp helpers/tests are deleted. See [current barrel revalidation](providers-all.md#current-pin-six-case-revalidation-2026-10-23).

The generator's old grouped/manifest writer is retired by its [whole-module re-port](model-catalog-generator.md). The frozen-absent model_data_tools/checker are now [removed](model-data-tools.md); no checker API, mode or grouped maintenance format remains. The committed `.manifest.json` here is test-only frozen-source/payload provenance, not a runtime refresh output. The historical provider differential imported a removed timestamp export; current Q15 revalidation removes that call and exercises actual model-aware auth boundaries. Whole providers/all.ts, provider helpers/factories, global Model APIs, readonly-vs-mutable TS alias identity, image catalogs and maintenance generators retain their own coverage labels.

All37 source rows close after2026-10-24 revalidation through the current lossless reader: the full4-test actual-source differential passes, every1058record/order/membership/lookup and all37provenance files remain exact. Current workspace/all-target check has0errors/0warnings, and warning-mode clippy has0owned changed-line findings. These gates do not establish factory/all.ts, provider protocol/auth, image catalog or checker parity.

## Current revalidation and historical development evidence

`cargo test -j4 -p pi-ai --test differential_chat_catalog -- --nocapture` GREEN4 at31af779f; `/tmp/pi-rust-repin-delta-ai/q15d-chat-literals-revalidation.log`. This exercises the exact current lossless reader and unchanged source snapshot; no artifact edits or regenerated hashes. Prior mutant evidence below remains source-artifact-specific, not new mutation runs.

`cargo test -p pi-ai --test differential_chat_catalog -- --nocapture`: 4/4 pass at e778072a; every 1058 source value/lookup and all ordered provider/model keys, Model losslessness, actual 37-file provenance/native-byte hashes, both alias paths/fresh copies. `providers` target exact staged-timestamp test: 1/1. All under mandatory Cargo runner, worktree-local target, jobs=4.

Five production-artifact mutants killed by the full-record lookup test: changed cost, swapped model order, omitted optional thinkingLevelMap, duplicate model.id under extra key, removed Team include entry. Mutants restored byte-for-byte to e778072a; existing GREEN remains valid. No broad suites/final gates run.

Initial bounded legacy probe exposed stale 17-vs-19 Anthropics count and builtin_providers → old baseten factory → deleted catalog. Authorized follow-up below resolves this exact inventory/deletion coupling; old generator/checker and broader differentials remain integration work.

## Authorized inventory coupling follow-up

Parent authorized only builtin provider factory ID/order correction and stale concrete catalog assertions after artifact removal. New `differential_provider_inventory` loads actual frozen providers/all.ts in a separate sandboxed oracle (not the literal catalog exporter), calls builtinProviders only to read IDs, and reproduces old baseten panic before inventory edits. Native inventory removes old-only baseten/qwen/radius registrations and adds Team through existing generic Codex factory. Source-derived Anthropics count updates to 19; obsolete baseten-positive lookup assertion becomes absence. Whole all.ts/factory/helper mutable-alias coverage remains TODO; no provider protocol/auth implementation changes.

Follow-up at 64e511d3: actual-source inventory 1/1 GREEN; concrete inventory 1/1 GREEN; catalog dispatch 1/1 GREEN. A swapped-provider-order mutant is killed by the actual-source ID/order test, then restored byte-for-byte. The stale gpt-4o.compat assertion was subsequently reconciled to frozen absence (eac35d47/55efa42b); `catalog_readers_preserve_optional_metadata` passes at this lane baseline db79a1a6. The old-only public factories and remaining provider fixtures are removed/reconciled in the [factory removal follow-up](providers.md#removed-old-only-factories).

The b85d0179 timestamp staging was temporary, not a parity waiver; Q15 deletes both helpers after model_runtime retirement. Historical staged-timestamp test evidence above is retired.
