# api/openai-prompt-cache traceability

Pinned source: `packages/ai/src/api/openai-prompt-cache.ts` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `OPENAI_PROMPT_CACHE_KEY_MAX_LENGTH` | `64`. | `api::openai_prompt_cache::OPENAI_PROMPT_CACHE_KEY_MAX_LENGTH`; `clamps_keys_longer_than_the_limit`. |
| `clampOpenAIPromptCacheKey(key)` | `undefined` stays `undefined`; `Array.from` counts Unicode code points; keys over 64 code points are sliced to the first 64. | `clamp_openai_prompt_cache_key(Option<&str>) -> Option<&str>` slices at the 64th char boundary; `preserves_missing_and_short_keys`, `clamps_keys_longer_than_the_limit`, `counts_code_points_not_utf16_units_or_bytes`. |
| Differential oracle | 20 keys: absent, empty, boundary lengths, astral and accented characters, newlines. | `tests/differential_api_leaf.rs` compares exact values and the exported limit. |

## Exclusions

- JavaScript `undefined` maps to `None`; other invalid runtime key types are excluded by the typed API.
- No source bug found.
