# Auth resolution

Frozen contract: `packages/ai/src/auth/resolve.ts` and `auth/types.ts` at [common pin](../porting/source-lock.json).

- getAuth returns provider auth without copying model.headers into it. Model headers stay on the request model for API-specific application, separate from auth/explicit-option headers.
- Every chat/image request supplies its actual model to auth resolution; providers can derive auth from its identity, provider and base URL. Never substitute the first catalog model or invent a representative model.
- Stored credentials own provider auth; ambient sources run only without stored credentials. Explicit request overrides overlay environment and API-key credentials.
- OAuth refresh runs only when expired, rechecks under the credential-store lock, persists the rotated credential before returning, and preserves source error code/message/cause boundaries.
- Strings and arbitrary credential extras remain native `JsString`/`JsValue` until their documented filesystem/HTTP boundaries.

## Google Vertex provider auth

Vertex auth has no login callback. A stored credential resolves only a nonempty stored key; missing/empty keys do not fall through to ambient key or ADC. Without a stored credential, a nonempty GOOGLE_CLOUD_API_KEY wins; otherwise ADC requires an existing credentials file and nonempty project/location. Path and project fallbacks use presence, not truthiness. Stored credential.env is not an ADC configuration path.

## Cloudflare provider auth

Cloudflare Workers AI requires a nonempty key/account; Gateway additionally requires a nonempty gateway ID. A stored credential owns every field, including missing values. Auth returns the actual model's base URL with every account/gateway placeholder replaced; Workers replaces an unused gateway placeholder with empty text. Gateway supplies its scoped bearer header and null Authorization/x-api-key overrides. Direct provider dispatch has no ambient stream-wrapper substitution.

## Approved staging gap (2026-10-22)

Provider-ID-only status wrappers in the old coding-agent runtime have no request model. They explicitly pass `None` via the separately named status resolver. This is not a request fallback; chat/image request entrypoints require a real `AuthModel`. Q15 removes the status-only path while replacing the old model registry/runtime architecture. See [traceability](../porting/traceability/auth-resolve.md).
