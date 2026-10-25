# index.ts — common-pin audit

Frozen: `packages/coding-agent/examples/extensions/custom-provider-gitlab-duo/index.ts`, `4f9e6f8a4`. Native: `crates/pi-coding-agent/examples/extensions/custom_provider_gitlab_duo.rs and models.json`.

## Export and branch coverage

MODELS and model map; direct-token cache TTL/hit/fetch/403/general error/invalidation; PKCE/login callback URL/code and refresh forms/expiry; stream key/model gates, backend URLs, replacement auth/headers, Anthropic adaptive thinking, event forwarding and Error.message/String catch; exact ProviderConfigInput registration. Native token/header values retain raw units; opaque header-name rejection and uncaught conversion boundaries retain existing classification.

## Proof and limits

`example_gitlab_duo` passes against the current frozen module: `/tmp/triage2-example_gitlab_duo-audit.log`. [Detailed behavioral fixture mapping](extension-examples-n.md). Audit revalidates the module logic and visible output, not live providers or OS-fault behavior. Native embedding/owned snapshot and en-US locale mappings remain within the standing approvals.

Gap: exotic/wrong-typed input, OS faults, numeric extremes and runtime scheduling oddities follow the current parity bar; existing precise discrepancies remain in the linked mapping. No new dependency, API carrier, or semantic deviation.
