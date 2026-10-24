# Removed Cloudflare stream wrapper

Common pin has no packages/ai/src/providers/cloudflare-stream.ts. Native module, exports, wrappers and old-wrapper tests are deleted, not adapted into a compatibility shim. The real [Cloudflare auth](providers-cloudflare-auth.md) owns endpoint substitution; direct factory dispatch does not materialize ambient placeholders.

Historical pre-repin wrapper proof is retired. Current tests compare full model-aware auth results and actual factory request transcripts.
