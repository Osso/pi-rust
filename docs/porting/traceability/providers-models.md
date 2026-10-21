# Frozen chat literal catalogs

Source: [common pin](../source-lock.json), `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d` (read-only). Contract: [chat catalogs](../../specs/chat-catalogs.md).

Rust: `providers/models_data.rs`, `model_catalogs_generated.rs`, and exactly 36 `data/providers/*.json` records. Source `as const` data is embedded; returned native owned copies do not claim mutable JS alias/reference semantics. No factory imports or calls in the loader. `serde_json` already enables preserve_order/float_roundtrip.

## Symbols and branches

| Export/branch | Native representation | Focused proof |
|---|---|---|
| `models.generated.ts::MODELS` | ordered static include table | every_literal_value_order_and_lookup_matches_source: every provider key, model key/value, absent key/provider; development RED recorded, GREEN pending |
| Every literal model field, omission, nested map/array | existing Model, no shared-type changes | every_source_record_roundtrips_native_model_without_field_loss: all 1058 values, initial GREEN |
| Literal GC/Team vs operational Codex aliases | independent literal records vs unchanged default-loader/rewrite factory | codex_factory_aliases_use_default_while_literal_aliases_remain_distinct_and_fresh: actual source values and independent fresh native copies; RED recorded, GREEN pending |

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

All 1058 source keys equal model.id. IDs repeat across providers (774 globally distinct); each provider's full ordered record is compared without global deduplication. Runtime exported records have unique keys by JavaScript object semantics. No old grouped flatten/dedup contract is carried into the direct loader.

Deleted old-only azure-openai-responses, baseten, qwen-token-plan, qwen-token-plan-cn, qwen-token-plan-individual artifacts; added Team. Public data-URL helper deleted (only in-repo test consumers). Timestamp API retained only for `pi-coding-agent/src/core/model_runtime.rs:21,217`; no generatedAt in new provenance, so None. This is a staged dependency, not parity approval.

Old `model_catalog_generator` and `model_data_tools::native_model_data_structure`/checker still expect grouped JSON and schema-3 maintenance manifests; untouched, not ported or accepted here. Existing old provider differential imports all.ts/index and removed timestamp export; not used as this proof. Whole providers/all.ts, provider helpers/factories, global Model APIs, readonly-vs-mutable TS alias identity, image catalogs and maintenance generators retain their own coverage labels.

37 source rows remain partial pending parent integration gates; focused development proof is not workspace/final acceptance.
