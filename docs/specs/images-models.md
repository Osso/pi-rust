# Image model collection

Match common-pin `ai/src/images-models.ts` for provider ordering, catalog reads, model-aware auth, refresh and generation.

- Preserve replacement order and synchronous last-known model reads.
- Resolve auth for the requested image model; explicit key/header/env values win per field. An empty auth baseUrl does not replace the model endpoint.
- Share an in-flight refresh; retain the previous catalog on failure. Preserve an existing ModelsError, otherwise wrap the original rejection as model_source.
- Return image error output for unknown providers, auth failure and authenticated generation failure. Preserve the frozen no-auth return-without-await discrepancy: an async provider rejection propagates as a rejection rather than image error output.

[Module trace and proof](../porting/traceability/images-models.md). No live provider proof is required; use injected callbacks and loopback requests.
