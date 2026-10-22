# Q18 lossless shared message boundary

Contract: [lossless content](../../specs/lossless-content.md). Frozen source: `4f9e6f8a4`, read-only.

| Capability | Implemented | Missing / blocked | Proof |
|---|---|---|---|
| `TextContent.text` / `ToolResultMessage.details` | `JsString` / `Option<JsValue>`, explicit-null adapter | downstream consumers pending | `pi-ai/lossless_content`: exact surrogate units, nested keys, omission/null, Rust→TS→Rust-visible JSON |
| Shared serde and object fields | raw-fragment writer/parser; object-only shared adapters | non-JSON serde formats unsupported | `pi-js-compat/js_json_serde`: five focused sandbox cases |
| Content extraction and text streaming final/delta fields | lossless `JsString`, including user/thinking/core custom text | coding-agent custom consumers pending | pi-ai/core focused targets compile; general provider regressions pending |
| Provider JSON bodies and `OnPayload` | lossless callback type, scalar builder move into AST; pi-messages direct lossless Context serialization | callback fixtures downstream pending | captured actual HTTP request text/details/callback unmatched-unit escapes equal Node `JSON.stringify` |
| Explicit provider sanitation | frozen per-call-site drop behavior | broad provider regressions pending | pinned `sanitize-unicode.ts` compared against Rust on the same message units |
| Node UTF-8 stream/path/process boundary | shared `encode_node_utf8` | consumer migration pending | distinct high/low-unit replacement test; original authority unchanged |
| Agent loop / core custom messages and summaries | lossless execution/hook arguments, result/details, custom AST and summary strings | coding-agent consumers and persisted JSONL integration pending | `lossless_agent`: two sandbox cases, including pinned TS JSON reread |
| Questions / mailbox / session name | raw contracts and consumers integrated; see checkpoints below | compilation and runtime acceptance pending | compiler-blocked; no end-to-end PASS |
| Provider ingress | [SSE and Codex WebSocket](lossless-provider-ingress.md) merged at `5909f149` | human errors, scalar metadata and non-stream images remain gaps | six sandbox tests PASS at lane executable revision; not parent integration acceptance |

## Chalk styling checkpoint

Chalk styling accepts/returns `JsString`; embedded close-code reopening and CRLF/LF encasing operate on UTF-16 units without encoding content. The raw differential invokes pinned Chalk. The original coding target remains compiler-blocked (`chalk-red.jsonl`); a standalone manifest imports the actual Chalk module and actual differential target without coding consumers. Its unchanged mandatory runner uses explicit private `PI_TEST_SANDBOX_DIR`; `chalk-standalone/selected-sandbox-result.log` records 3/3 tests, including raw styles, scalar style cases and supports-color detection. The first hidden-`/tmp` attempt failed before execution. This proves only the module; coding style propagation and independent verification remain pending.

## Session-name lookup checkpoint

Resume name selection, in-memory comparisons and not-found labels retain `JsString`. Metadata ID/name lookup accepts native strings, appends the source ID upper bound natively and encodes only query bindings. `metadata_name_lookup_encodes_only_at_sql_binding` covers two distinct raw names colliding at the source SQL boundary; the resume-name unit case covers native query/error units. `metadata-lookup-red.jsonl` and `resume-name-red.jsonl` remain compiler-blocked, with no runtime PASS. Resume ID/path normalization still has earlier encoding holes; this checkpoint does not approve them.

## Shared-channel input checkpoint

The channel tool and `PostSharedChannelMessageInput.body` retain `JsString` through source trimming. Only the SQLite binding encodes Node UTF-8; reading that SQL TEXT returns the source-replaced string. `shared_channel_encodes_raw_body_only_at_sql_binding` checks unchanged input plus stored output; `shared-channel-red.jsonl` is compiler-blocked, with no runtime PASS.

## Steering input checkpoint

Cancellation reasons and steering `fileRefs` use native `JsString`/`JsValue` through lifecycle requests; file-reference extras and unmatched keys are not projected through typed scalar vectors. New `steering_and_cancellation_params_keep_raw_human_content` is compiler-blocked (`steering-params-red.jsonl`), not a runtime PASS. Agent target IDs still use generated scalar snapshot contracts; arbitrary persisted IDs and their not-found presentation remain an explicit open boundary, not an approved restriction.

## Canonical relay integration checkpoint

Canonical host-relay ingestion now constructs the shared raw mailbox address/ref/message fields directly. Literal kind classification no longer encodes text before comparing tags; the original literal remains in the routed object. Existing `host_relay_store` raw-payload and `host_relay_protocol` surrogate-address retry regressions remain required after parent compilation; merging canonical fixes is not a new runtime PASS.

## Custom glob input checkpoint

`FindOperations::glob` receives `&JsString`; the tool and sandbox payload no longer encode pattern units before matching. `custom_find_operation_receives_original_pattern_units` covers the actual custom-operation callback. `find-pattern-red.jsonl` remains compiler-blocked, not a behavioral RED or PASS. Matcher runtime proof is a separate dependency.

## Custom IO error checkpoint

Main commits `a380bc0a`, `48d9bbe7`, `7830910b`, `5c22e53e` integrate raw `HandlerError` recovery across read/write/edit/find/ls, native `JsString` SessionManagerError, and coordinated callbacks/steering. Sandbox operation traits retain the original error inside `io::Error`; tools recover its raw message instead of Display-encoding it. Edit/ls prefixes concatenate native units. `custom_write_operation_error_keeps_raw_units` targets the actual operation boundary; `raw-io-red.jsonl` is compiler-blocked, not behavioral RED or PASS. Shared native OS error formatting remains unchanged.

## Session callback integration checkpoint

The main commits above preserve `SessionInfoChangedEvent.name` and `ExtensionUiContext.notify` as `JsString` through RPC payloads and the interactive notification queue. Detached completion errors retain their snapshot's raw authority. All new parent `lossless_session` tests remain compiler-blocked; `parent-integration-red.log` records an earlier 423-error checkpoint, not current acceptance. No runtime PASS or full lossless UI claim.

### Merged lane checkpoints

- Provider `5909f149`: `final-ingress-green.log` records **6/6 sandbox PASS** at executable revision `1b01dc97` (later lane changes docs only), exercising actual one-byte SSE and Codex WebSocket. Frozen provider differentials cover SSE; Mistral retains its explicit source sanitizer sites. [Ingress traceability](lossless-provider-ingress.md) records the then-open human-error/metadata/image boundaries; current [metadata approval](../../specs/lossless-content.md#what-it-must-do) supersedes its pending classification, not its proof limits.
- Runtime `e3ce34c3`: raw `AgentDetails` error/result, terminal/cancellation `JsObject` mutation/replay and `WritableSessionMetadata.name` reach explicit Node UTF-8 encoding at SQL binds. Library/test probes report zero owned errors or warnings but exit 101 on external errors; tests never execute. This is compiler-only evidence, not runtime acceptance. [Runtime/mailbox traceability](lossless-runtime-mailbox.md) records behavioral targets; final report: `/tmp/pi-rust-repin-lossless-core-runtime/REPORT.md`.
- Bundled `553ed154`: raw notification callbacks and parser/llama error chains integrated. Final lane library probe remains blocked (83 errors, seven owned primary locations); added tests are compiler-blocked. Report: `/tmp/pi-rust-repin-lossless-bundled/REPORT.md`. These lane-local counts do not describe the current merged tree.

### Unapproved TUI boundary gap

Frozen `Component.render(width): string[]` retains UTF-16 through composition until `terminal.write(buffer)`. Rust `Component::render -> Vec<String>` and scalar Text storage/cache/render cannot retain lone units. Encoding at Text/notification construction is premature, **not** the actual source terminal boundary; it is not approved. Raw input/cursor, width/wrap/slice, style and frame propagation also remain open. Exact investigation: `/tmp/pi-rust-repin-raw-tui-boundary.md`. Opaque scalar provider metadata now has [explicit user approval](../../specs/lossless-content.md#what-it-must-do); human text/errors and arbitrary argument JSON do not. Input must widen to UTF-16, not become a representation gap. These checkpoints establish neither Q18 nor full-port acceptance.

## Development proof ledger

Artifacts under `/tmp/pi-rust-repin-lossless-content/`:

- `serde-red.log`: shared serde API missing (compiler RED); `serde-green.log`: original three cases pass.
- `content-red.log`: original ToolResultMessage rejected a lone surrogate at deserialize (behavioral RED).
- `helpers-red.log`: new explicit boundary/object helper APIs missing (compiler RED).
- `js_json_serde-green.log`: five focused helper/serde cases pass.
- `content-provider-diff.log`: oracle fixture failed because TSX selected CJS and rejected top-level await; fixed fixture, not counted as behavioral RED.
- `content-provider-diff-green.log`: two sandbox differentials pass, including actual pi-messages HTTP body capture and callback replacement.

Checkpoint proof is bounded. It does not establish workspace compilation, unchanged provider test fixtures, all-source parity, or JSONL integration.

User/thinking checkpoint: `UserContent::Text`, `ThinkingContent.thinking`, thinking delta/end events use JsString. OpenAI Completions conversion/cache/request builders now use JsValue, preserving the source-unsanitized reasoning-signature field. `user-thinking-red.log` reproduces rejection of a user lone surrogate; `user-thinking-green-2.log`: four pi-ai sandbox tests PASS, including captured OpenAI HTTP reasoning escapes. Initial thinking-only HTTP fixture was dropped by the existing no-visible-content rule; adding visible text reproduces the actual source boundary. Custom text, argument API and downstream consumers remain pending.

## Argument/outbound checkpoint

`ToolCall.arguments` is a JsObject field using shared object serde. Complete/partial/repair JSON parsers return JsValue; grammar input extraction/buffering retains JsString. Native Anthropic/Google/Bedrock request trees use JsValue; OpenAI/Mistral argument JSON text uses the lossless writer rather than serializing bare map keys. Google raw SSE decoding selects typed records from lossless JSON, including JsValue arguments and JsString text.

Proof: `arguments-red.log` reproduces the old ToolCall argument-key rejection. `arguments-checkpoint-green.log`: seven focused tests PASS, zero warnings. One test captures real HTTP bodies from five direct providers (Anthropic, Google, OpenAI completions/responses, Mistral), confirms exact argument units and byte equality to Node JSON.stringify. Another differential loads frozen `utils/json-parse.ts` for complete and partial unmatched-unit cases. Bedrock request migration has compilation evidence only at this checkpoint.

A generic dispatcher fixture hung after a Google provider task panicked on a bare JsObject macro leaf. The Cargo group was terminated and all children verified exited; partial proof is `arguments-provider-green-2.log` (five prior cases passed, new case unproven). Bounded direct-provider reproduction (`arguments-provider-direct.log`) identified the key-serialization panic; construction now wraps the object in JsValue, with no guard/fallback. `arguments-provider-direct-green.log` passes.

### Checked-boundary status

No whole-value argument/event adapters remain. Validator integration at `1120b034` uses the lane's JsValue engine directly; its entrypoint now constructs JsValue::Object(call.arguments.clone()). Anthropic events are JsValue throughout.

`anthropic_messages::metadata_text` remains a checked UTF-8 restriction for scalar provider metadata (ids/names/type discriminators, stop reasons/explanations, signatures). User approved scalar opaque provider IDs/names, type/stop tags and signatures on 2026-10-21 (“Approve scalar metadata gap”), requiring explicit errors rather than replacement. Human explanations/errors and arbitrary argument JSON remain excluded from that approval. Unmatched units produce an error, never replacement. Text, thinking and argument content never call this helper.

### Native fragment / Anthropic ingress checkpoint

`ToolCall.partial_json` and ToolCallDelta text use JsString; provider argument buffers and complete/repair/partial parsers operate on native UTF-16. `anthropic-ingress-red.log` reproduces the removed event adapter rejecting a valid text delta. `anthropic-ingress-green-attempt.log`: eight sandbox cases PASS, zero warnings, including actual ordered SSE message/text/thinking/tool-fragment/delta/stop handling. The tool fragment carries a native unmatched unit after outer JSON parsing; its final argument retains that exact unit. Frozen partial-parser differential additionally covers raw unmatched units in complete and unterminated strings.

Workspace compilation, consumer fixtures and end-to-end agent/JSONL/question/mailbox/name proof remain pending.

## Provider-native JSON checkpoint

`ProviderNativeContext.value` uses JsValue. Responses shared conversion/tools and OpenAI/Azure/Codex request trees now retain native items without projection into serde_json::Value; Codex continuation response-item cache uses the same AST. The core custom-message extractor can decode the typed native context from its lossless authority rather than dropping valid unmatched units.

`native-context-red.log` reproduces the old native value deserialize rejection. `native-context-green.log`: ten pi-ai sandbox tests PASS, zero warnings, including a captured actual Responses HTTP body with unmatched-unit native text and key, no scalar fallback, and exact Node JSON.stringify bytes. Other existing provider fixtures remain pending migration/regression execution.

## Core agent checkpoint

Core execution/prepare/hooks use JsValue, result/details retain missing versus explicit null, and custom messages use a single JsObject authority. Session summary/custom-data fields and compaction serialization retain JsString/JsValue. Proxy deltas/argument buffers and request context are lossless. Node tools encode only at OS boundaries; the temporary checked edit adapter remains an explicit branch-only blocker pending edit-lane integration.

`core-migrate-5.jsonl`: focused core target compiles, zero warnings. `core-lossless-green.log`: two sandbox cases PASS, including tool argument execution, raw result text/details and Rust→TS→Rust JSON reload. This is not a persisted session JSONL proof. `coding-agent-first.jsonl`: downstream library compilation fails with 313 diagnostics; counts recorded in `coding-agent-error-counts.json`. No workspace readiness claim.

Shared extension contract checkpoint: execute/prepare/render arguments and before-provider payload use JsValue; tool events use JsObject with lossless serde; custom-message content reuses core authority; schemas and renderer-private state remain serde JSON. Coding-agent integration intentionally remains compiler-RED (`coding-agent-2.jsonl`, 423 diagnostics after revealing downstream contract consumers). `session-red.jsonl` records the persisted-session test compilation blocker.

Core consumer propagation checkpoint (compiler-RED): session summary/details/custom data and compaction prompts now retain shared authorities. Raw JSON enum dispatch replaces serde tagged/untagged buffering for known session records; summary extras are decoded from the same AST rather than a scalar-only flattened map. Prompt templates preserve UTF-16 arguments through quote parsing and substitution. Input hooks, prompt/steering queues, command arguments, fork editor text, and AgentResult summaries use JsString. New bounded `lossless_session` tests await downstream library compilation; `prompt-red.jsonl` records compiler RED. `coding-agent-4.jsonl` records 373 remaining errors across current downstream integration. Remaining internal notification/policy contracts and untyped legacy session records are not yet proven.

Permission/error propagation checkpoint: MCP permission responses, updated input and raw bash allow-rule content retain JsValue/JsString. Rule persistence uses lossless JSON; schemas/settings-manager callers retain their existing typed boundary. ToolError/AgentError and native extension HandlerError messages retain JsString; Display alone uses Node UTF-8 encoding. `tool-errors-red.jsonl` reproduces the scalar-only error constructor compile failure; `core-error-migrate-1.jsonl` compiles the new focused core target. Tool migration remains WIP, including the prior temporary checked edit adapter.

Question/display contract checkpoint (compiler-RED): question labels/text and selected answers retain JsString; panel output and metadata retain JsValue. This does not prove terminal-only encoding: the unapproved scalar TUI rendering gap above remains open. ExtensionUiDone/custom and ToolRenderState now use JsValue; loader/session error payloads retain JsString. Truncation keeps whole-line units, slices grep lines by UTF-16 units, and decodes byte-limited suffixes only at the frozen Buffer boundary. `questions-red.jsonl` and `truncation-red.jsonl` record blocked focused-target compilation. The new source-faithful coding truncate implementation no longer re-exports the scalar-only harness copy. Core tool implementations and parent mailbox/supervisor integration remain WIP; no workspace-success claim.

Search helper checkpoint: JsString lowercase preserves unmatched UTF-16 units as Unicode context boundaries, supporting raw-unit history matching without a scalar projection. `lowercase-red.log` records missing API compilation; `lowercase-green.log`: two sandbox cases pass (raw units/Greek final sigma and unsigned-number bounds). Full search-tool integration remains compiler-RED.

Presence follow-up: the shared present-property adapter now supports boxed values, preserving explicit null in session compaction/custom fields and extension result details. `boxed-null-red.log` reproduces the former boxed API mismatch; `boxed-null-green.log`: eight shared serde cases PASS, zero warnings. Pending coding-agent fixtures include custom data omission versus null. Extension load/compaction errors and compact error callbacks retain JsString until display.

Core tool caller checkpoint (not a compiling coding-agent crate): read/write use lossless argument values, preserve original write-success text/path and encode only OS bytes; result wrappers retain raw tail text and arbitrary detail keys when spilling. Bash/search/find/grep/ls/code-index/resume callers and terminal render views use the shared authorities. History search retains raw keys/text through lowercase matching and JSON results. New `lossless_tools` fixtures cover actual write bytes and spill bytes versus retained raw content, but cannot execute until edit and parent integration compile. `coding-agent-12.jsonl` records remaining diagnostics; raw edit implementation and mailbox/supervisor contracts remain open.

Current bounded baseline: `shared-agent-provider-green.log` has core 3/3 and provider 10/10 PASS, zero warnings. Mutants `mutant-error-replacement.log` and `mutant-null-collapse.log` each fail their behavioral assertion (not compilation); source restored byte-for-byte to SHA-256 `8cc3f506411b26b6b7082018b5bbe189cdd787a568fc79686eb52f78be4b2600` for core `agent/types.rs`. This does not replace coding-agent or workspace acceptance.

Edit-substrate RED: `edit-differential-red.jsonl` demonstrates missing UTF-16 edit APIs. A lone oldText unit can match inside a valid surrogate pair; rejecting every unmatched oldText is not source-faithful. Shared unit search/slicing/replacement/trim helpers now retain that boundary, with linear-time prefix-table search. `text-ops-red.log` is the missing-API RED; `text-ops-green.log`: two sandbox behavior tests PASS, zero warnings. Core edit/diff migration and pinned edit differential remain open.

Core edit checkpoint: shared edit/diff/patch text and unit offsets now retain JsString, including matching a lone low unit inside a valid pair. The branch-only checked edit bridge is removed. Core execution retains original path/result/diff text and encodes only the bytes passed to the file environment. `edit-substrate-green-3.log`: five sandbox cases PASS, including pinned edit/patch and word-diff references plus actual Node-like file bytes. Subsequent tokenizer readability extraction requires focused confirmation after commit. Coding edit/preview consumers and fixture migration remain pending.

Coding edit consumer checkpoint: input, preview cache payloads, applied content, duplicate-context slicing and result details now retain JsString/JsValue. The UTF-16-to-byte conversion is removed; file writes alone encode Node UTF-8. Renderer calls intentionally pass raw diff to the modes-owned renderer, whose signature still requires downstream adaptation. New coding tests cover frozen duplicate context (including a split pair at the context limit) and actual low-unit file edits, but cannot execute while the coding-agent library remains blocked.

Proxy ingress follow-up: `proxy-raw-red-diagnostic.log` reproduces serde tagged-enum buffering rejecting a valid lone-surrogate delta (`unexpected end of hex escape`). Dispatch now reads a JsValue authority and decodes only selected fields, without a whole-event scalar projection. The regression sends real proxy SSE one byte at a time, including native text/thinking units, tool fragments with raw keys and a lossless HTTP request context. `proxy-raw-green.log`: focused case PASS. Scalar proxy id/name/signature/error metadata retain their native String restriction; they are not raw-content parity evidence.

Core fixture propagation: execution/prepare/custom/detail fixtures now construct JsValue directly; schemas remain serde JSON. Existing scalar-only fixture formatting uses checked conversion, not replacement. `core-fixtures-3.jsonl` records the last compiler RED. `core-fixtures-green.log` then runs 41 agent-loop cases: 40 PASS, one stale expected U+FFFD FAIL. Frozen `agent-loop.ts:960–969` slices native UTF-16, so the cap fixture now expects the retained low unit `0xDE00`; harness/edit targets have not yet run in that stopped invocation. `core-fixtures-green-2.log`: all 41 agent-loop cases PASS; edit-diff cases 7 PASS/1 FAIL expose a migration regression in the plural no-change error suffix. Frozen `edit-diff.ts:377` does not include the single-edit explanatory suffix; removed it from the plural branch. `core-fixtures-green-3.log`: edit-diff 8/8 and harness tools 22/22 PASS, zero warnings. Agent-loop proof remains valid for unchanged execution code at `58fc2a29`.

Current integration blocker: `coding-agent-14-valid.jsonl` fails the two focused coding-agent target builds with 339 library errors and one warning, dominated by bundled-extension/modes consumers. One direct loader diagnostic now converts a scalar-generated error into JsString. Approval payload/reviewer, broadcast and multi-agent summary contracts await the parent-owned runtime lane; edit render calls await modes. No coding-agent JSONL/tool or workspace acceptance claim.

## Parent contract follow-up

Extension-injected system prompts now use JsString through Context, Agent state and before-agent-start overrides; base prompt files still enter through their actual UTF-8 decode. Manual compaction/tree instructions and interrupt input also retain JsString. `lossless_content::system_prompt_round_trip_keeps_extension_injected_utf16` first failed with a real serde hex-escape error before the field change (`system-prompt-red.log`). Consumer integration remains compiler-blocked; no new workspace/persistence pass is claimed here.

Parent runtime consumers: mailbox/shared-channel formatting retains JsString through host headers, body/file-reference formatting and prompt construction; malformed file references return errors instead of disappearing. Shared oracle support now has an explicit request_lossless path (scalar callers fail closed on unrepresentable responses). Human agent/session names, abort outcomes, approval text and system-prompt hooks retain raw units. New coordination/name/restart/artifact/outcome tests remain compiler-blocked until consumer integration; no runtime pass claimed for them.

## Q18 model/settings path checkpoint
Auth backend/read-only storage, ModelRuntime options/state, ModelConfig and FileModelsStore path identities retain JsString. SettingsManager creation and file-storage paths retain native units, encoding only filesystem/lock operations; resource loader option cwd/agent_dir widened (implementation belongs to parent lane). `lossless_agent_session_paths::raw_storage_paths_keep_identity_until_the_filesystem_boundary` checks backend/catalog identity and raw models.json error suffix. `replacement-red.jsonl` is compiler-blocked by library consumers (1783 diagnostics); no behavioral PASS claimed.

Runtime transitions checkpoint: factory errors, fallback message, cwd/agentDir/session paths and fork IDs retain JsString; relocation details use JsObject/JsValue. Runtime services diagnostics retain JsString. Session cwd/import errors construct native messages; self restart handoff and extension switch/fork/shutdown contracts are native. Factory raw-unit test is compiler-blocked in `runtime-red.jsonl`; no runtime PASS. External constructor consumers and runtime remaining compilation still pending.

Consumer follow-up: native permission locations, system prompt cwd/output, AgentSession multi-agent IDs/parent IDs, bash chunk callback, export output path and mailbox acceptance DB path. Extension runner cwd/resource discovery no longer asks callers for scalar identities. Remaining excluded helper consumers are tracked in lane report; no focused coding runtime PASS.

Stream identity checkpoint: StreamOptions/Agent session ID, session resource cleanup and provider cache IDs retain JsString. Prompt cache key caps use UTF-16 slice (including a split surrogate). Header values encode at their HTTP boundary. New raw cache-key HTTP test first rejected JsString (`session-id-red.log`); GREEN and pi-ai all-targets checks pending.

Provider cache-key HTTP proof at 82ff3a53: `session-id-green.log` runs the actual sandboxed OpenAI Responses stream and captures JSON containing a split-pair prompt_cache_key; 1 PASS, no warnings. pi-ai all-targets check remains blocked by stale fixtures, being adapted. Extension live cwd, discovery cwd and pending relocation targets + mailbox transcript session paths now use native identities.
