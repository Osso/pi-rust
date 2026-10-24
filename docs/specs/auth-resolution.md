# Auth resolution

Frozen contract: `packages/ai/src/auth/resolve.ts` and `auth/types.ts` at [common pin](../porting/source-lock.json).

- Every chat/image request supplies its actual model to auth resolution; providers can derive auth from its identity, provider and base URL. Never substitute the first catalog model or invent a representative model.
- Stored credentials own provider auth; ambient sources run only without stored credentials. Explicit request overrides overlay environment and API-key credentials.
- OAuth refresh runs only when expired, rechecks under the credential-store lock, persists the rotated credential before returning, and preserves source error code/message/cause boundaries.
- Strings and arbitrary credential extras remain native `JsString`/`JsValue` until their documented filesystem/HTTP boundaries.

## Cloudflare provider auth

Cloudflare Workers AI requires a nonempty key/account; Gateway additionally requires a nonempty gateway ID. A stored credential owns every field, including missing values. Auth returns the actual model's base URL with every account/gateway placeholder replaced; Workers replaces an unused gateway placeholder with empty text. Gateway supplies its scoped bearer header and null Authorization/x-api-key overrides. Direct provider dispatch has no ambient stream-wrapper substitution.

## Approved staging gap (2026-10-22)

Provider-ID-only status wrappers in the old coding-agent runtime have no request model. They explicitly pass `None` via the separately named status resolver. This is not a request fallback; chat/image request entrypoints require a real `AuthModel`. Q15 removes the status-only path while replacing the old model registry/runtime architecture. See [traceability](../porting/traceability/auth-resolve.md).
