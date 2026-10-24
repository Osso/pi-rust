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

## Resolver boundaries

Overrides contain only apiKey/env; no provider-status overload, minimum-validity window, refresh deadline or cancellation option. Chat and image getAuth take the actual requested model. Every read/API-key/OAuth failure wraps with the frozen ModelsError code/message and original cause; only the credential-modify catch preserves an existing ModelsError. Refresh rechecks expiry under the lock, permits concurrent refresh/logout/type changes, and does not demand renewed minimum validity. Empty env overlay values consult the base context; fileExists delegates unchanged. [Coverage and remaining proof](../porting/traceability/auth-resolve.md).
