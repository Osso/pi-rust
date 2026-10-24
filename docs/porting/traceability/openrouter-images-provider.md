# OpenRouter images provider — common pin

Authority: `packages/ai/src/providers/openrouter-images.ts:1–14` at4f9e6f8a4. Native `crates/pi-ai/src/providers/openrouter_images.rs`.

| Frozen branch | Native / proof |
|---|---|
| id/name | openrouter/OpenRouter; actual imported factory descriptor comparison. |
| auth | API-key only: envApiKeyAuth("OpenRouter API key", ["OPENROUTER_API_KEY"]). Actual factory RED exposed source-absent native OAuth advertisement; deleted it and its failing-loader stub. Current authKeys/name/oauth result matches frozen exactly. |
| models | Complete ordered embedded IMAGE_MODELS.openrouter records; image-provider native test and [literal proof](image-models-generated.md). Owned-copy semantics approved. |
| API | openrouter_images_api with no refresh_models override; existing image auth/dispatch tests retain model values and requests. |

`oauth_load_contract::openrouter_image_provider_auth_matches_real_frozen_factory` plus four openrouter_images_provider tests GREEN in `q15c-oauth-retained-flows-1.log`. Actual three-loader contract comparison uses the same oracle. No OpenRouter OAuth flow exists at this pin; [old-only deletion ledger](load.md) replaces the retired gap/stub narrative. Other image API/registry/factory modules retain their own proof rows.
