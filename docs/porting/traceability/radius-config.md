# providers/radius-config traceability

Pinned source: `packages/ai/src/providers/radius-config.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `DEFAULT_RADIUS_GATEWAY` | `https://radius.pi.dev`. | Constant; `normalizes_gateway_urls_with_scheme_detection_and_trailing_slashes`. |
| `isRadiusGatewayModel` | Shallow object/field checks (id/name strings, reasoning bool, input array, cost object, numeric windows). | `is_radius_gateway_model`; `sanitizes_valid_configs_and_filters_shallowly_invalid_models`. |
| `sanitizeRadiusGatewayConfig` | Reject non-objects/missing baseUrl/models; filter shallow-invalid models; clone entries. | `sanitize_radius_gateway_config`; behavioral + differential. |
| `normalizeRadiusGatewayUrl` | Prepend `https://` unless `http(s)://`; strip trailing slashes. | `normalize_radius_gateway_url`; differential includes empty and `https://` edge cases. |
| `getRadiusCredentialConfig` | Read/sanitize `credential.gatewayConfig`. | `get_radius_credential_config`; `reads_the_gateway_config_from_the_credential_extra_fields`. |
| `getRadiusModelsFromConfig` | Spread model fields and set `api: "pi-messages"`, provider, baseUrl. | `get_radius_models_from_config`; behavioral + differential model projection. |
| `getRadiusModels` | Empty without a valid config. | `get_radius_models`; behavioral test. |
| `truncateHttpBody` | Trim, 512 UTF-16 code units, append `…`. | `truncate_http_body`; `loads_gateway_config_with_bearer_auth_and_truncated_errors`. |
| `loadRadiusGatewayConfig` | GET `/v1/config`, optional bearer, status error with truncated body, invalid-config error. | `load_radius_gateway_config`; behavioral test. |

## Exclusions and discrepancies

- The source's shallow model check accepts models whose `cost` object cannot satisfy the typed `ModelCost`; Rust drops those entries. This is the typed-input exclusion for invalid JS shapes.
- Unknown keys on a gateway model are spread into the TS model but cannot be represented by the typed `Model`; Rust drops them.
- Differential calls the exported `getRadiusCredentialConfig` (the sanitizer is module-private in TS).

## Q18 provider request integration (2026-10-22)

Gateway config sanitization reads JsValue directly, preserving raw base URL and excluding only source-shallow-invalid models. Shallow-valid model deserialization errors propagate as contextual CallbackError through Result-returning config/model APIs; no scalar projection or silent parse drop remains. Gateway/key values stay native until HTTP URL/header construction.

[Focused public-provider runtime proof](lossless-sdk-boundaries.md) records exact covered cases, revisions and remaining acceptance limits.
