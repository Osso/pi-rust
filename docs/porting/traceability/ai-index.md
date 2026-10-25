# ai index / base traceability

Sources: `packages/ai/src/index.ts` and `packages/ai/src/base.ts` at
`4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`.
Rust: crate-root re-exports in `crates/pi-ai/src/lib.rs`, `crates/pi-ai/src/base.rs`.
Tests: `crates/pi-ai/tests/public_index.rs` (compiles and exercises every
re-exported path), plus the module-level tests of each re-exported item.

`index.ts` is side-effect free and re-exports only existing modules; the Rust
crate root mirrors it with `pub use` globs. `base.ts` re-exports `compat`, so
`pi_ai::base` is a glob re-export of `pi_ai::compat`.

## `index.ts` → Rust

| TypeScript export | Rust path |
|---|---|
| `Type`, `Static`, `TSchema` (typebox) | typed exclusion: JSON Schema is `serde_json::Value`; builders live in `utils::typebox_helpers` (`string_enum`, `StringEnumOptions`) and `TypeBoxSchema` |
| `AnthropicEffort`, `AnthropicOptions`, `AnthropicThinkingDisplay` | `api::anthropic_messages::{AnthropicEffort, AnthropicOptions, AnthropicThinkingDisplay}` |
| `AzureOpenAIResponsesOptions` | `api::azure_openai_responses::AzureOpenAIResponsesOptions` |
| `BedrockOptions`, `BedrockThinkingDisplay` | `api::bedrock_converse_stream::{BedrockOptions, BedrockThinkingDisplay}` |
| `GoogleOptions` | `api::google_generative_ai::GoogleOptions` |
| `GoogleThinkingLevel` | `api::google_shared::GoogleThinkingLevel` |
| `GoogleVertexOptions` | `api::google_vertex::GoogleVertexOptions` |
| `export * from api/lazy.ts` | `api::lazy::*` (`LazyApi`, `LazyApiCapabilities`, `lazy_api`, `lazy_stream`, `ApiLoader`, `StreamSetup`) |
| `MistralOptions` | `api::mistral_conversations::MistralOptions` |
| `OpenAICodexResponsesOptions`, `OpenAICodexWebSocketDebugStats` | `api::openai_codex_responses::{OpenAICodexResponsesOptions, OpenAICodexWebSocketDebugStats}` |
| `OpenAICompletionsOptions` | `api::openai_completions::OpenAICompletionsOptions` |
| `OpenAIResponsesOptions` | `api::openai_responses::OpenAIResponsesOptions` |
| `export * from auth/context.ts` | `auth::context::*` |
| `export * from auth/credential-store.ts` | `auth::credential_store::*` |
| `export * from auth/helpers.ts` | `auth::helpers::*` |
| `export * from auth/types.ts` | `auth::types::*` |
| `OAuthAuthInfo`, `OAuthCredentials`, `OAuthDeviceCodeInfo`, `OAuthLoginCallbacks`, `OAuthPrompt`, `OAuthSelectOption`, `OAuthSelectPrompt` | `compat::extension_oauth_types::{...}` |
| `export * from images-models.ts` | `images_models::*` |
| `export * from models.ts` | `models::*` |
| `export * from providers/faux.ts` | `providers::faux::*` |
| `export * from session-resources.ts` | `session_resources::*` |
| `export * from types.ts` | `types::*` + `request::*` (request-side interfaces live in `request.rs`) |
| `export * from utils/diagnostics.ts` | `utils::diagnostics::*` |
| `export * from utils/event-stream.ts` | `utils::event_stream::*` |
| `export * from utils/json-parse.ts` | `utils::json_parse::*` |
| `export * from utils/overflow.ts` | `utils::overflow::*` |
| `export * from utils/retry.ts` | `utils::retry::*` |
| `export * from utils/typebox-helpers.ts` | `utils::typebox_helpers::*` |
| `export * from utils/validation.ts` | `utils::validation::*` |

## `base.ts`

| TypeScript | Rust |
|---|---|
| `export * from "./compat.ts"` | `pub use crate::compat::*;` + `pub use crate::*;` in `base.rs` (compat.ts itself re-exports `index.ts`) |

`compat.ts` also re-exports the ten lazy api factories, `env-api-keys`,
`image-models`, `images`, `images-api-registry`, `legacy-api-aliases`,
`providers/images/register-builtins` (images slice), and `utils/retry`; those
are re-exported from `pi_ai::compat` alongside the registry/dispatch API.

Structural deviation: `compat.ts` additionally re-exports `index.ts`, so the
TypeScript `compat` entrypoint exposes the whole index surface. In Rust that
surface lives at the crate root (and therefore in `base`, which mirrors the
union); `pi_ai::compat` exposes the compat-specific API plus its direct module
re-exports. Porting a `@earendil-works/pi-ai/compat` import of an index symbol
means using the crate-root path.

## Typed exclusions and gaps

- TypeBox `Type`/`Static`/`TSchema`: TypeScript's schema builder is a compile-time
  generic system. Rust represents schemas as JSON (`TypeBoxSchema`/`Value`) and
  provides only the `string_enum` helper needed by providers; the hidden
  TypeBox `~kind` marker is recorded as `Tool::typebox_kind`.
- The crate root also exposes `pi_ai::compat` as a module (TypeScript reaches it
  through the `./compat` package export, not `index.ts`).
- `models.rs` intentionally exports a `Models` trait; `index.ts` also exports the
  same name, so the mapping is one-to-one.

## Current-pin closure (2026-10-25)

Added the five missing root OAuth type names: OAuthCredentials, OAuthProviderId, OAuthProvider, OAuthProviderInfo and OAuthProviderInterface. `public_oauth_types` fails to compile before the root re-exports and exercises the legacy getApiKey boundary after them. Strict source witness `oracle/ai-root-oauth-types-fixture.mts` imports these names from the unmodified frozen index. Existing public_index runtime tests remain unchanged. Removed old-only PiMessages, models-store, text and UUID root exports in their owning slices; TypeBox runtime builders remain the existing typed exclusion. Linking the Rust crate does not execute provider factories or register compat providers. Imported modules retain their independent coverage status.
