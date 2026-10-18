# Map iteration-order audit (`pi-ai`, `pi-agent-core`, `pi-tui`)

Scope: every non-test use of `BTreeMap`, `BTreeSet`, `HashMap`, `HashSet` in
`crates/{pi-ai,pi-agent-core,pi-tui}/src`, plus the `IndexMap`/`IndexSet`
replacements introduced here. `pi-coding-agent` is out of scope except for
call-site type fixes required by the public type changes.

## JS ordering rules applied

- `Map`/`Set` iterate in insertion order. Deleting and re-inserting a key moves
  it to the end (`shift_remove` + `insert`, never `swap_remove`).
- Plain objects iterate integer-like keys ascending first, then the remaining
  string keys in insertion order. None of the converted maps in scope are keyed
  by integer-like strings (`"0"`, `"1"`, …), so insertion order is correct.
- `Headers` is not a plain object: the Fetch implementation sorts header names
  lexicographically and lowercases them. `headersToRecord(new Headers(...))`
  therefore yields **sorted** keys, not insertion order. Those sites stay
  `BTreeMap`.
- Explicit `sort(...)`/`bySequence(...)`/`localeCompare` in TS keeps its
  comparator; no Rust map type can change it.

## Decision legend

| Action | Meaning |
| --- | --- |
| `IndexMap` / `IndexSet` | Source order is observable; switched and behavior tested. |
| `BTreeMap` (sorted) | TS counterpart sorts explicitly or is a `Headers` record. |
| lookup-only | Only `get`/`contains`/`insert`/`remove`; iteration never reaches output, error text, callbacks, or serialized JSON. Type kept. |

## `pi-ai`

### Core types and helpers

| Site (file:line) | TS counterpart | Observable? | Action | Test |
| --- | --- | --- | --- | --- |
| `src/types.rs:113` `ThinkingLevelMap` | `types.ts` `Model.thinkingLevelMap` (`Partial<Record<...>>`), serialized in model JSON | yes (serialized model) | `IndexMap` | `models::thinking_levels_clamp_to_supported_levels`, serialized model tests |
| `src/types.rs:193` `ProviderEnv` | `types.ts` `ProviderEnv` (`Record<string,string>`) | yes (public auth result, env merge order) | `IndexMap` | `images_models::merges_provider_resolved_env_into_image_options` |
| `src/types.rs:195` `ProviderHeaders` | `types.ts` `ProviderHeaders` (`Record<string,string|null>`) | yes (public auth result, transforms, extensions) | `IndexMap` | `differential_headers::header_record_order_matches_the_pinned_typescript_module` |
| `src/types.rs:237` `ProviderResponse.headers` | `types.ts` `ProviderResponse.headers` produced by `headersToRecord` | yes (callback payload) | `BTreeMap` (sorted) | `differential_headers::header_records_match_the_pinned_typescript_module` |
| `src/types.rs:669` `GrammarVariants` | `types.ts` `GrammarVariants` (`Partial<Record<...>>`), serialized in tool definitions | yes (request body) | `IndexMap` | `differential_constrained_sampling` |
| `src/types.rs:923,925` compat `chatTemplateKwargs`/`chatTemplateArgs` | `types.ts` `OpenAICompletionsCompat` records, serialized into request body | yes (request body) | `IndexMap` | `differential_openai_completions` scenario 40 |
| `src/types.rs:1222,1243,1296` `Model.headers`, wire `ModelWire.headers`, `ImagesModel.headers` | `types.ts` `Model.headers` record, serialized in model JSON | yes (serialized model) | `IndexMap` | `models` header merge tests; `differential_model_catalog` model round-trip |
| `src/utils/headers.rs:10,11` `headers_to_record` | `utils/headers.ts` `headersToRecord(new Headers(...))` | yes (callback record) | `BTreeMap` (Headers sorts) | `differential_headers::header_record_order_matches_the_pinned_typescript_module` |
| `src/utils/headers.rs:31,33` `provider_headers_to_record` | `utils/headers.ts` `providerHeadersToRecord` (`Object.entries`) | yes (request headers, public record) | `IndexMap` | same test |
| `src/models.rs:105,764,858,862` `ModelsRefreshResult.errors` | `models.ts` `errors = new Map<string, Error>()`; joined in `package-manager-cli.ts` | yes (joined error text, callback consumers) | `IndexMap` | `models::refresh_reports_errors_in_completion_order`; `pi-coding-agent` `package_command_execution` |
| `src/models.rs:909,919` auth env | `models.ts` `env` spread (`Record`) | yes (returned auth env) | `IndexMap` | `models::applies_resolved_env_to_stream_options` |
| `src/models.rs:1392` `ProviderApi::Map` | `providers/*.ts` `api: { ... }` record | yes (public variant; iteration reaches stream selection) | `IndexMap` | `models::create_provider_dispatches_on_model_api_and_reports_missing_implementations` |
| `src/models.rs:289,290,344,376,385,386,398,405,418` generations/tails/controllers | `models.ts` internal `Map`s | no (lookup only) | keep `HashMap` | — |
| `src/models.rs:865,1373` selected provider ids, provider id list | `models.ts` `Set` membership / ordered provider list | no (membership; provider order comes from the provider vector) | keep `HashSet` | — |
| `src/model_catalog.rs:6,9,15` `ModelGroups`, `ModelCatalog`, `flatten_model_catalog` | `model-catalog.ts` `flattenModelCatalog` (`Object.assign({}, ...Object.values(groups))`) | yes (returned catalog order; later duplicates keep position) | `IndexMap` | `differential_model_catalog::model_catalog_preserves_source_key_order` |
| `src/images_models.rs:333` merged env | `images-models.ts` `{ ...resolution.env, ...options.env }` | yes (env handed to provider) | `IndexMap` | `images_models::merges_provider_resolved_env_into_image_options` |

### Provider registry and provider modules

| Site (file:line) | TS counterpart | Observable? | Action | Test |
| --- | --- | --- | --- | --- |
| `src/images_api_registry.rs:15,16,55` image API registry | `images-api-registry.ts` `imagesApiProviderRegistry` | no (only `set`/`get`) | lookup-only `BTreeMap` | `images_api_registry` tests |
| `src/models_store.rs:73` model store | `models-store.ts` `entries` | no (only `get`/`set`/`delete`) | lookup-only `HashMap` | `models_store` tests |
| `src/auth/credential_store.rs:18` refresh chains | `auth/credential-store.ts` internal locks | no (per-provider lock lookup) | lookup-only `HashMap` | `models::serializes_concurrent_oauth_refreshes` |
| `src/providers/xai.rs:16`, `opencode.rs:16`, `opencode_go.rs:15`, `fireworks.rs:14`, `cloudflare_ai_gateway.rs:17`, `github_copilot.rs:43` API maps | `providers/*.ts` `api` record | yes (public `ProviderApi::Map`) | `IndexMap` | provider differentials / `models` tests |
| `src/providers/github_copilot.rs:28` available ids | `providers/github-copilot.ts` `Set` membership | no (membership only) | lookup-only `BTreeSet` | `differential_providers` |
| `src/providers/google_vertex.rs:113` env | `providers/google-vertex.ts` env record | yes (auth env returned) | `indexmap::IndexMap` | `provider_auth` env tests |
| `src/providers/faux.rs:216,217,288,289,572,573,1055` prompt cache / deferred responses | `providers/faux.ts` internal maps | no (lookup only) | keep `HashMap` | `faux_provider` tests |
| `src/providers/faux.rs:370,518,718` response headers | `headersToRecord` output | yes | `BTreeMap` (sorted) | `faux_provider` tests |

### Request construction (`api/`)

| Site (file:line) | TS counterpart | Observable? | Action | Test |
| --- | --- | --- | --- | --- |
| `src/api/transform_messages.rs:33,46,77,144` tool-call id map | `transform-messages.ts` `toolCallIdMap` | no (lookup only) | keep `BTreeMap` | `differential_transform_messages` |
| `src/api/transform_messages.rs:220,221` pending/result ids | `transform-messages.ts` `Set`s | no (membership) | keep `BTreeSet` | `differential_transform_messages` |
| `src/api/anthropic_messages.rs:304` provider headers | `anthropic-messages.ts` `mergeHeaders` record | yes (public `ProviderHeaders`) | `IndexMap` | `differential_anthropic_messages` |
| `src/api/anthropic_messages.rs:496,497,525,526,575,579,932` deferred/loaded tool names | `anthropic-messages.ts` `Set`s | no (membership; deferred order comes from `Vec<Tool>`) | keep `BTreeSet` | `differential_anthropic_messages` |
| `src/api/azure_openai_responses.rs:63,64` deployment map | `azure-openai-responses.ts` deployment lookup | no (lookup only) | keep `BTreeMap` | `differential_azure_openai_responses` |
| `src/api/azure_openai_responses.rs:198,299,301,302` grammar props + merged headers | `ReadonlyMap` / `ProviderHeaders` | yes | `IndexMap` | `differential_azure_openai_responses` |
| `src/api/bedrock_converse_stream.rs:343` response headers | `headersToRecord` output | yes | `BTreeMap` (sorted) | `differential_bedrock_converse_stream` |
| `src/api/bedrock_credentials.rs:71,92,230,231,250,254,283,285,286,299,300` INI sections | AWS SDK INI objects | no (section lookups) | lookup-only `BTreeMap` | `bedrock_converse_stream::profile` tests |
| `src/api/bedrock_eventstream.rs:12,97,98` decoded event headers | Rust-only Smithy decoder | no (header lookups) | lookup-only `BTreeMap` | `bedrock-runtime-helpers` CRC/event tests |
| `src/api/bedrock_sigv4.rs:144,145` canonical headers | SigV4 canonical request (must be sorted) | yes (signature) | `BTreeMap` (required sorted) | differential signer re-signs oracle requests |
| `src/api/cloudflare_gateway_binding.rs:26,164,165` | `cloudflare-gateway-binding.ts` header record | yes | pre-existing `IndexMap` | `differential_cloudflare_gateway_binding` |
| `src/api/constrained_sampling.rs:221,222` `create_grammar_tool_input_properties` | `constrained-sampling.ts` returns `Map` | yes (returned map) | `IndexMap` | `differential_constrained_sampling` (non-alphabetical tools) |
| `src/api/github_copilot_headers.rs:40,41` | `github-copilot-headers.ts` `buildCopilotDynamicHeaders` record | yes (returned record, request headers) | `IndexMap` | `github_copilot_headers` tests |
| `src/api/google_client.rs:582` merged headers | `providerHeadersToRecord({ ...model.headers, ...optionsHeaders })` | yes (request headers) | `IndexMap` | `differential_google_api` |
| `src/api/mistral_conversations.rs:316,317,978,1259` id maps / partial args | `mistral-conversations.ts` internal maps | no (lookup only) | keep `BTreeMap` | `differential_mistral_conversations` |
| `src/api/mistral_conversations.rs:495,496,514,522,532` request headers | `mistral-conversations.ts` `Headers` built from a record | yes (recorded request headers) | `IndexMap` | `mistral_conversations` header-override test |
| `src/api/openai_codex_responses.rs:503,523,2101,2386` grammar props | `openai-codex-responses.ts` `ReadonlyMap` | yes (public option; `.get` only at use sites) | `IndexMap` | `differential_openai_codex_responses` |
| `src/api/openai_codex_responses.rs:744,748,752,756,766,770,771,807,812,825,830,1020,1077,1153,1811,1930,2096` request headers | `openai-codex-responses.ts` `Headers` | yes (recorded request headers; `Headers.set` keeps position, delete+set moves to end) | `IndexMap`, `shift_remove` in `delete_header` | `differential_openai_codex_responses` |
| `src/api/openai_codex_responses.rs:719,1688,1692,1695,1696,1697` provider id set, websocket caches/stats | `openai-codex-responses.ts` `Set`/`Map`s | no (membership, single-session `get`/`set`/`delete`) | lookup-only | `openai_codex_responses` tests |
| `src/api/openai_completions.rs:72,89,90,202,203,976,994,1305,1306,1308,1309,1480,1582` grammar props, chat template values, merged headers | `openai-completions.ts` `ReadonlyMap`, `Record`, `mergeHeaders` | yes (request body and recorded headers) | `IndexMap` | `differential_openai_completions` scenario 40 |
| `src/api/openai_completions.rs:318,338,641,1044,1465,1476,1477,1478,1481,1587,1588,1589,1590` seen names, tool-name lookup, streaming state | `openai-completions.ts` `Set`s/`Map`s | no (membership/lookup; output order comes from `Vec`/stream index) | keep `BTreeMap`/`BTreeSet` | `differential_openai_completions` |
| `src/api/openai_responses.rs:191,347,348,350,351` grammar props + merged headers | `openai-responses.ts` `ReadonlyMap` / `ProviderHeaders` | yes | `IndexMap` | `differential_openai_responses` |
| `src/api/openai_responses.rs:201` deferred tool map | `openai-responses.ts` `deferredTools` | no (`.get`/`.has` only) | lookup-only `BTreeMap` | `differential_openai_responses` |
| `src/api/openai_responses_shared.rs:45,54` grammar props | `openai-responses-shared.ts` `ReadonlyMap` | yes (public option) | `IndexMap` | `differential_openai_responses_shared` |
| `src/api/openai_responses_shared.rs:55` deferred tools | `openai-responses-shared.ts` `ReadonlyMap` | no (`.get`/`.has` only) | lookup-only `BTreeMap` | `differential_openai_responses_shared` |
| `src/api/openai_responses_shared.rs:448,454,504` allowed providers / loaded / emitted ids | `openai-responses-shared.ts` `Set`s | no (membership) | keep `BTreeSet` | `differential_openai_responses_shared` |
| `src/api/openai_responses_shared.rs:843,918,1035,1050,1131,1283,1284` output slots / reasoning blocks | `openai-responses-shared.ts` `Map`s | no (`get`/`set`/`delete`; stream index order drives output) | lookup-only `BTreeMap` | `differential_openai_responses_shared` |
| `src/api/openai_sdk.rs:31` `default_headers` | SDK `defaultHeaders` record | yes (merged into request headers) | `IndexMap` | provider differentials (`differential_openai_completions`, `..._responses`) |
| `src/api/openai_sdk.rs:369` SSE error headers | SDK internal header record | no (lookup into `ProviderErrorHeaders`) | lookup-only `BTreeMap` | `differential_error_body` |
| `src/api/openrouter_images.rs:200,201` merged headers | `openrouter-images.ts` `{ ...model.headers, ...options.headers }` | yes (request headers) | `IndexMap` | `differential_openrouter_images` |
| `src/api/pi_messages.rs:528,529` request headers | `pi-messages.ts` literal + `providerHeadersToRecord` spread | yes (recorded request headers) | `IndexMap` | `differential_pi_messages` |
| `src/api/pi_messages.rs:668,675` streaming tool JSON | `pi-messages.ts` `toolJson` `Map` | no (index lookup) | lookup-only `BTreeMap` | `pi_messages` tests |
| `src/utils/deferred_tools.rs:47,61,62,63` unique positions / used / deferred names | `utils/deferred-tools.ts` `Map`/`Set`s | no (positions/used are lookups; deferred order is the `Vec`) | lookup-only | `differential_deferred_tools` |
| `src/utils/estimate.rs:122,133` added tool names | `utils/estimate.ts` `Set` membership | no (membership) | lookup-only `BTreeSet` | `differential_estimate` |
| `src/utils/provider_retry.rs:19` error headers | `utils/provider-retry.ts` header lookups | no (`get` only) | lookup-only `BTreeMap` | `differential_retry` |

## `pi-agent-core`

| Site (file:line) | TS counterpart | Observable? | Action | Test |
| --- | --- | --- | --- | --- |
| `src/harness/types.rs:91` `AgentHarnessStreamOptions.headers` | `harness/types.ts` `headers?: Record<string,string>` | yes (public option; merged into request headers) | `IndexMap` | `harness_stream_options::stream_options_keep_absent_and_explicit_values_distinct` |
| `src/harness/types.rs:116,117` header/metadata patches | `harness/types.ts` `Record` patches (`undefined` deletes) | yes (public option; patch order) | `IndexMap` | `harness_stream_options::stream_option_patch_distinguishes_unchanged_clear_and_set` |
| `src/harness/types.rs:517` `ShellExecOptions.env` | `harness/types.ts` `env?: Record<string,string>` | no (process environment is unordered) | keep `HashMap` | `bash_execution_message`, `harness_env` |
| `src/harness/session/types.rs:311` `resumeData` | `harness/session/types.ts` `resumeData?: { [id]: JsonValue }` | yes (serialized into session JSONL) | `IndexMap` | `session_jsonl::resume_data_serializes_in_source_key_order` |
| `src/harness/session/memory.rs:216,238` `sessions` | `harness/session/memory.ts` `sessions` Map; `list()` uses `values()` | yes (session listing order) | `IndexMap`, `shift_remove` on delete | `session_memory::lists_sessions_in_creation_order` |
| `src/harness/reducer.rs:275` deferred-handle validation | `harness/reducer.ts` `validateDeferredHandles(entriesById.values())` | yes (first error message) | iterate ordered `&[Entry]` | `harness_reducer` 16-entry non-alphabetical case (also in `differential_harness_core`); reverting to `HashMap` iteration reports `xi-deferred` |
| `src/harness/reducer.rs:1,276,277,278,279,280,286,320,348,378,399,426,508,520,521,535,538,542,574,586,597,630,650,668,725,729,751,769,809,879,915,916,1006,1040,1109,1130,1189,1190,1260,1261,1282,1285,1286,1309` entries-by-id, starts, attempts, queue maps, invocation sets | `harness/reducer.ts` corresponding `Map`/`Set`s | no (lookups; ordered results are built from `Vec`/sequence sorts) | lookup-only `HashMap`/`HashSet` | `harness_reducer`, `differential_harness_core` |
| `src/harness/events.rs:112,150` listener registry | `harness/events.ts` `listeners` Map of Sets | no (emit uses `get`; removal scans all buckets) | lookup-only `HashMap` | `harness_events` |
| `src/harness/session/state.rs:52,54,56,61,74,76,78,83,515` used ids, entries-by-id, open ops by lane, lanes, labels | `harness/session/state.ts` | no (lookups; lane/open-operation order is the ordered `Vec`) | lookup-only `HashMap`/`HashSet` | `session_memory` state tests |
| `src/harness/session/context.rs:42` entry projectors | `harness/session/context.ts` projector map | no (lookup by entry type) | lookup-only `HashMap` | `session_context` |
| `src/harness/session/jsonl/repo.rs:259,281` active create destinations | `harness/session/jsonl/repo.ts` `Set` | no (membership) | lookup-only `HashSet` | `session_jsonl` |
| `src/harness/agent_harness.rs:985` entry projectors | `harness/agent-harness.ts` projector map | no (lookup by entry type) | lookup-only `BTreeMap` | `agent_harness` |
| `src/harness/compaction/branch_summarization.rs:146` old branch path | `harness/compaction/branch-summarization.ts` `Set` | no (membership) | lookup-only `BTreeSet` | `branch_summarization` |
| `src/harness/tools/file_mutation_queue.rs:17` queue entries | `harness/tools/file-mutation-queue.ts` map | no (keyed lock lookup) | lookup-only `HashMap` | file-mutation tests |
| `src/harness/tools/bash.rs:28,239` env | `harness/tools/bash.ts` env record | no (process environment) | keep `HashMap` | `bash_execution_message` |
| `src/harness/utils/shell_output.rs:30` env | `harness/utils/shell-output.ts` env record | no (process environment) | keep `HashMap` | `shell_output_capture` |
| `src/harness/env/nodejs.rs:36,44,45,54,762,763,765,771` shell env / child pids | `harness/env/nodejs.ts` | no (env lookups, pid membership) | keep `HashMap`/`HashSet` | `harness_env` |
| `src/agent/agent_loop.rs:242,250` completed/pending tool ids | `agent/agent-loop.ts` `Set`s | no (membership) | lookup-only `BTreeSet` | `agent_loop` |
| `src/agent/proxy.rs:107,120` partial JSON | `agent/proxy.ts` map | no (index lookup) | lookup-only `HashMap` | `proxy` |
| `src/jsdiff.rs:82,101` best path | jsdiff internal map | no (diagonal lookup) | lookup-only `HashMap` | `differential_*` diff tests |
| `src/search/scanning.rs:332,421` seen session ids | `search/scanning.ts` `Set` | no (membership) | lookup-only `HashSet` | `session_search` |

Pre-existing `IndexSet` uses in `agent/types.rs:525`, `agent/agent.rs:239,353`,
and `harness/compaction/utils.rs:16,18,20,64` were already ordered and are not
part of this change.

## `pi-tui`

| Site (file:line) | TS counterpart | Observable? | Action | Test |
| --- | --- | --- | --- | --- |
| `src/layout.rs:59,651` render cache | `layout.ts` render cache | no (keyed by height/width) | lookup-only `HashMap` | `layout` / render tests |
| `src/terminal_image.rs:464` kitty metadata registry | `terminal-image.ts` registry | no (id lookup; eviction order kept in the `VecDeque`) | lookup-only `HashMap` | `terminal_image` |
| `src/tui_alt_screen.rs:1225` search ranges by row | `tui-alt-screen.ts` `rangesByRow` Map | no (each row is written independently; per-row ranges are sorted by the same comparator) | `BTreeMap` | `tui_alt_screen` |
| `src/components/markdown/lexer.rs:528,538,1837` link definitions | `marked` lexer (Rust-only port) | no (label lookup) | lookup-only `HashMap` | markdown differential |
| `src/components/editor.rs:174,278,362,2056,2614,2646,2681` paste registry | `components/editor.ts` `pastes` Map keyed by ascending numeric id | no (numeric ids are contiguous and ascending; `BTreeMap` order equals Map insertion order after the shift-down delete+set sequence) | `BTreeMap` | `editor` tests |

No `indexmap` dependency was needed in `pi-tui`.

## Public type changes and call-site fixes

- `pi_ai::types`: `ProviderEnv`, `ProviderHeaders`, `ThinkingLevelMap`,
  `GrammarVariants`, `Model.headers`, `ImagesModel.headers`,
  `OpenAICompletionsCompat.chat_template_kwargs/args` moved to `IndexMap`.
  `ProviderResponse.headers` stays `BTreeMap` because it is a `Headers` record.
- `pi_ai::models`: `ModelsRefreshResult.errors`, `ProviderApi::Map`.
- `pi_ai::model_catalog`: `ModelGroups`, `ModelCatalog`.
- `pi_ai::api` options/headers listed above.
- `pi_agent_core::harness::types`: `AgentHarnessStreamOptions.headers`,
  `AgentHarnessStreamOptionsPatch.headers/metadata`.
- `pi_agent_core::harness::session::types`: `resume_data`.
- `pi_server::protocol` and `pi_coding_agent` test call sites were updated for
  the `IndexMap` key type and source-order error join.

## Tests added

| Test | Proves |
| --- | --- |
| `differential_headers::header_record_order_matches_the_pinned_typescript_module` | `Headers`-derived records stay sorted; `providerHeadersToRecord` keeps object insertion order. |
| `differential_model_catalog::model_catalog_preserves_source_key_order` | Group/model insertion order and duplicate-key position against the TS oracle. |
| `differential_constrained_sampling` (non-alphabetical `properties` request) | `createGrammarToolInputProperties` Map order against the TS oracle. |
| `differential_openai_completions` scenario 40 | `chat_template_kwargs` body key order against the TS oracle. |
| `models::refresh_reports_errors_in_completion_order` | `ModelsRefreshResult.errors` follows completion/insertion order, not provider-id sort. |
| `images_models::merges_provider_resolved_env_into_image_options` | Provider env then request env spread order, including overwrite position. |
| `harness_reducer` 16-entry corruption case + `differential_harness_core` | The first invalid deferred entry in input order names the error (falsified: `HashMap` iteration names `xi-deferred`). |
| `session_memory::lists_sessions_in_creation_order` | `InMemorySessionRepo::list` returns creation order. |
| `session_jsonl::resume_data_serializes_in_source_key_order` | `resumeData` JSONL keys keep source order. |
| `pi-ai/tests/model_catalog.rs` updated expectation | `Object.assign` overwrite keeps the original key position. |
| `pi-coding-agent/tests/package_command_execution.rs` updated expectation | Refresh errors are joined in map insertion order (`openai` before `anthropic`). |

## Proof

- `cargo fmt --check`
- `cargo test -p pi-ai -p pi-agent-core -p pi-tui` — 2093 passed, 0 failed
- `cargo check --workspace --all-targets`
- `cargo clippy --workspace --all-targets -- -D warnings`
