# Common-pin OAuth flows

Frozen `packages/ai/src/utils/oauth/{anthropic,github-copilot,openai-codex,index,types}.ts` defines the OAuth utilities and legacy registry. [Module traces](../porting/traceability/oauth-registry.md) describe the native bridge.

## What it must do

- [x] Expose all frozen direct login/refresh helpers, method constants, builtin providers, registry operations and callbacks. Synchronous callback failures use Result and retain their rejection.
- [x] Preserve first-value query parsing, two-part hash parsing, empty-manual second prompts, omitted-manual callback waiting, and manual non-Error wrapping.
- [x] Close browser listeners before waiting for token exchange results; preserve Codex's classified onAuth-before-finally leak. Reject empty callback codes without settling the flow. Anthropic bind failures retain Node listen fields.
- [x] Keep fetch, body-read and JSON rejection catches at their exact source boundaries, including recursive branded Anthropic causes, shared SyntaxErrors and Codex status-text error-body handling. Derive Codex expiry after the token response.
- [x] Poll with exact rejection values. Copilot sends no login signal on HTTP requests, uses its independent models timeout, starts all policies concurrently, never reads policy response bodies and requires picker-enabled models.

## How it works

- [Anthropic trace](../porting/traceability/anthropic.md)
- [Copilot trace](../porting/traceability/auth-oauth-github-copilot.md)
- [Codex trace](../porting/traceability/openai-codex.md)
- [Auth values and storage](auth-types.md)

## Implementation inventory

- `crates/pi-ai/src/auth/oauth/{anthropic,github_copilot,openai_codex,support}.rs`: native flows, HTTP and callback boundaries.
- `crates/pi-ai/src/{oauth,oauth_registry}.rs`: public forwarding, builtin registry and legacy callback bridge.
- `crates/pi-ai/src/{compat/extension_oauth_types,auth/types}.rs`: existing callbacks and shared native error/credential values.

## Tests asserting this spec

- `crates/pi-ai/tests/{oauth_repin_edges,public_oauth_types,oauth_registry,anthropic_oauth,github_copilot_oauth,openai_codex_oauth}.rs`.
- Existing `differential_oauth_flows`, `differential_github_copilot_oauth`, `differential_anthropic_timeout`, `oauth_abort_wrapper`, `auth_types_contract` and app `auth_codex_credential_order` retain frozen HTTP/credential and failure proof.

## Known gaps (current cycle)

No open in-scope module gap. [Successor verification](../porting/ai-core-successor-proof.md) records the actual focused gates and inherited persistence/HTTP proof; no independent agent was used.

## Out of scope

Adversarial/wrong-typed JSON, native runtime/browser/crypto readiness and scheduling oddities follow AGENTS.md. Existing lossless carriers and frozen credential/key-order authority remain unchanged. No live provider calls.
