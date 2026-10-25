# Extension examples N

Current 4f9e6f8a4 module audits: [subagent](example-subagent.md), [GitLab Duo](example-gitlab-duo.md), [RPC client](example-rpc-extension-ui.md), [questionnaire](example-questionnaire.md). They supersede historical inherited-child-flag drift and native-only optionless-questionnaire proof: child flags now match frozen, and the empty-option case compares directly with the frozen guard.

Pinned source `532f7d12`; native opt-in embedding, no default registration.
Worktree base updated from `5d2454d` to `04f51ba` before extension API implementation.

## Subagent discovery and dispatch

2026-10-24 build-only correction: `js_format!` has no brace escapes. The tool description interpolates the literal `{previous}` as an argument rather than using the invalid `{{previous}}` template. `example_subagent::tool_description_matches_the_pinned_extension_literal` compares the complete registered description with the frozen extension through the real runner; no dispatch behavior changes.

Embed `examples/extensions/subagent/index.rs` using `#[path]`; explicitly register
`subagent_extension(api, agent_dir, Invocation { program, prefix })` in an
`ExtensionRegistry` factory. `agent_dir` is the host-resolved agent root;
`program/prefix` is the embedding host's executable invocation, replacing TS's
Node/Bun/current-script detection. No hardcoded `pi` lookup or real Pi child is
used by tests. Native hosts choose their invocation explicitly.

| Pinned source / branches | Native source | Behavioral proof |
|---|---|---|
| agents.ts:29–93 discovery, suffix/type filtering, read failures, YAML, tools/body | subagent/agents.rs::load_agents | example_subagent_agents: same private disk files through actual pinned module; broken symlink/directory/case/incomplete YAML fixtures |
| agents.ts:95–113 nearest ancestor and scope; project overwrite retains user insertion position | discover_agents / merge_agents | three scope differentials; nearest ancestor; empty root; overwrite order |
| agents.ts:116–126 list, zero/negative/oversized limit | format_agent_list | full value differential for 0,1,8,-1 and empty |
| index.ts:265–442 child invocation, private prompt, JSONL, usage/model, stderr, exit, abort, cleanup | subagent/index.rs::run_single, apply_event, write_prompt | example_subagent: Python fixture child pipes through real native runner and unchanged TS extension loaded through real TS runner; malformed line/trailing non-LF, usage, failure, inherited/overridden model, actual prompt bytes/options/deletion; native spawn-error and timed cancellation |
| index.ts:444–721 schema/registration, exclusive modes, project trust, sequential substitution/failure, parallel limits/output | subagent_extension / execute / run_chain / run_parallel | 12 full-result differentials; UI confirmations; native refusal without spawn; update counts and single/chain callback-time snapshots |
| index.ts:32–263,723–1033 call/result rendering, tools, usage, single/chain/parallel | subagent/render.rs | example_subagent renderer differential: actual native components vs pinned components, ANSI-stripped terminal lines; expanded/collapsed and every tool formatting branch |

`example_subagent_agents` tests the companion module directly (no extension entry
exists there). `example_subagent` loads the shipped entry through explicit native
registry and `ExtensionRunner`; TS oracle loads the unchanged extension through
its real loader/runtime/runner. Only child executable and host UI/model/thinking
are fixture seams. The child is Python, never `pi`/Node CLI; both parsers consume
real process pipes. All invocations occur beneath Cargo's sandbox guard.

Approved isolation delta: partial results own JSON snapshots rather than retaining
pinned mutable object aliases. Oracle clones at callback time: single/chain
snapshots compare completely, including stderr arriving before stdout. Parallel
completion order is nondeterministic; compare update counts and final ordered
results. Zero usage/cost serialization compares exactly. No API additions/dependencies.

Proof gaps: exact ANSI theme bytes, concurrency peak/timing, ignored-SIGTERM child,
per-task 10 KiB multibyte truncation, non-UTF8 stdout chunk splitting, optional cwd
override and every renderer limit/error branch lack independent fixtures. Native
parallel dispatch uses four permits and an eight-task cap; max-task and mixed
success results have differential evidence. Typed frontmatter strings/native
scope input exclude invalid JavaScript runtime types, not valid string values.

## Batch N fidelity corrections (F1–F4, U3)

Merged in `69696cba`: subagent fixes `2f83d590`, RPC lifecycle/UTF-8
`7c8cef28`, process-reaping assertions `648a7a5c`, traceability `6a9b435e`.

| Pinned behavior | Native correction | Behavioral proof |
|---|---|---|
| subagent index.ts:549–551 global replacement | `run_chain` calls [shared replacement helper](js-compat.md#global-nonempty-literal-replacement) | `example_subagent::modes_dispatch_and_streaming_match_pinned_real_extension_runtime`: first child emits all four escapes plus `$1`; second task has two placeholders; complete results and streaming snapshots compare to pinned TS |
| subagent index.ts:1008–1014 running parallel row uses warning hourglass before failure check | `render::result` running icon branch | `running_parallel_renderer_matches_pinned`: running partial result with exitCode -1, empty messages; pinned rendered lines compare |
| subagent index.ts:38–42,63 decimal ties use `toFixed` at all three sites | token k/M and cost call existing `number::to_fixed` | `usage_decimal_ties_match_pinned`: input 1250 → ↑1.3k, output 1250000 → ↓1.3M, cost 0.03125 → $0.0313; expanded/collapsed single/chain/parallel corpus |
| RPC idle Escape stops UI, sends SIGTERM, returns without waiting; child exit ends client independently of descendant pipe EOF | `run` selects child status and reads stderr incrementally; no pipe-reader task or EOF join | native `idle_escape_returns_without_waiting_for_sigterm_ignoring_child` and `child_exit_returns_while_grandchild_keeps_both_pipes_open`: two-second bound; fixture children/grandchild killed, reaped and PID absence asserted, including failure cleanup |
| RPC readline decodes invalid UTF-8; non-JSON lines ignored | byte-line reader with `String::from_utf8_lossy` | `invalid_utf8_stdout_line_does_not_hide_following_notification`: [255,10] then notification; pinned whole-program oracle now uses real Node readline over private PassThrough bytes; native real process renders notification |

F1 preserves surprising source replacement-string behavior; it is not silently
changed to literal substitution. All three subagent `toFixed` sites audited;
GitLab Duo and RPC batch-N examples contain none. Existing native factory
signatures and `ctx.ui.custom` call sites are unchanged.

RED, GREEN and revert-mutation logs: `/tmp/pi-rust-batch-n-fixes/ledger.md`.
`subagent-RED.log` / `subagent-mutation.log` each report the three distinct
F1/F2/F3 behavioral failures; `rpc-RED.log` / `rpc-mutation.log` each report
both lifecycle timeouts plus U3 UTF-8 error. Restored final tests: subagent
5/5, RPC 7/7. Coverage rows remain done; no new dependencies or deviations.

## Proof ledger

`/tmp/pi-rust-ext-examples-n/ledger.md`; numbered logs in the same directory.
Only per-example Cargo test targets and formatting; no broad checks or live state.

## Shared acquisition Result migration (2026-10-24)

Custom Anthropic and GitLab registered callbacks wrap their existing synchronously returned outer streams in Ok; neither callback adds a catch. GitLab inner backend acquisition now propagates Result into its existing async catch at frozen index.ts:347–369. Parent approved correcting only that catch to Error.message/String(error) and native AssistantMessage construction, with a negative proof; conversion-rejection routing must be established from actual source before using the uncaught authority. Actual-source negative RED q15e-gitlab-acquisition-catch-red: four full request/event cases preserve TypeError/raw lone units in frozen but not native; a separate source child exits1 with no completion/error-event output for String-conversion rejection. Its TypeError diagnostic is checked before the failing native exit0 assertion. That proves this conversion boundary is process-uncaught, authorizing the existing shared authority here. The catch now uses error_or_string_message and direct native AssistantMessage construction, preserving raw UTF16 without scalar serde. Its conversion rejection uses only the relocated shared authority. Focused negatives GREEN8cb4039b, and complete6active GitLab tests renewed after typed fixture cleanup561c6693; process rejection cases renewed after parent hook merge0639d2d8. The1private child entry is invoked by its active parent process test.

## GitLab Duo

Embed `examples/extensions/custom_provider_gitlab_duo.rs` and register
`gitlab_duo_extension(&ExtensionApi)` explicitly. The factory composes a native
provider with `$GITLAB_TOKEN` and OAuth; the host supplies its existing native
provider-registration callback when binding the runner. `gitlab_duo_extension_with_backends`
accepts the existing OAuth runtime and existing ProviderStreams implementations
for isolated embedders/tests. It adds no core public API. Cache state belongs to
that native module instance, shared by its OAuth and provider stream methods.

| Source lines / symbols / branches | Rust | Proof |
|---|---|---|
| 32–176 constants, MODELS/MODEL_MAP; 383–405 factory/catalog/auth | custom_provider_gitlab_duo.rs / models.json / Provider::get_models | example_gitlab_duo: complete nine-model metadata differential; factory registration through native loader, registry, real ExtensionRunner and host callback |
| 182–220 token, 25-minute cache, 403/generic failures | State::direct_token | full transport and stream differentials, repeated mixed-backend requests use one direct-token fetch; native exact TTL boundary and refresh invalidation |
| 218–303 PKCE, URL/form order, callback code, login/refresh, expiry, invalidation | GitlabOAuth / State::exchange | successful login/refresh, missing callback code, exchange/refresh failure; exact auth URLs, form bodies, signal presence, credentials/auth differential |
| 307–376 exported streamGitLabDuo, key/model errors, backend selection, adaptive thinking, options/header replacement, event forwarding, caught errors | Provider::stream_simple | Anthropic + two OpenAI models, exact forwarded model/context/key/headers, actual event forwarding; missing key/unknown model/403/500/offline errors including zero usage and timestamp |

The oracle transpiles the complete unchanged pinned module with transport,
cryptographic randomness/clock and backend imports replaced. It executes its
factory through the real pinned loader and extension runtime. Native fixtures use
real runner binding; no live providers, HTTP servers, credentials or config.
Cache hit and errors compare against the pinned module; exact-expiry and OAuth
cache invalidation have native-only evidence. No stack frames are compared.

Proof gaps: actual remote GitLab/Anthropic/OpenAI interoperability, malformed
HTTP JSON/types, concurrent cache-miss races, user-aborted OAuth and inner stream
exceptions lack dedicated batch-N fixtures. Backend request serialization remains
owned by the existing core API implementations (not reimplemented here).

## RPC extension UI client

Embed `examples/rpc_extension_ui.rs`; call `run(Invocation, terminal)` explicitly.
Supply the native host executable/prefix and its registered rpc-demo extension ID.
This replaces the source's hardcoded Node/dist/TS paths, without keeping an old
path fallback. The client itself is not an extension factory. It owns real native
Input and SelectList components under a real TuiMainScreen. Terminal events are
pumped on its UI thread; no Rc objects cross threads.

| Source lines / behavior | Native | Proof |
|---|---|---|
| 72–103 bounded output lines/raw append/UTF-16 slice | OutputLog | exact ANSI-preserving screen differential at widths 20/100; raw-unit split-surrogate differential at widths 19/20/21 |
| 109–135 loading interval/dots/start/stop | RpcExtensionUi::tick, run | explicit tick screen differential; real UI event loop |
| 141–249 input, select/input dialogs and help/focus | RpcExtensionUi / Dialog / Focusable | real core components, exact rendered lines and JSON response frames in whole-program differential |
| 255–305 spawn/500ms startup, stdin serialization, exit/kill | run / Invocation | private Python child startup-failure and successful dialog process-lifetime tests; real native rpc-demo harness below |
| 310–465 bottom/loading/dialog UI and all RPC UI request methods | extension_request / receive / complete | select/confirm/input/editor, prefill flattening, notifications/status/widgets/editor text; exact frames and ANSI renders |
| 471–527 four local slash commands | submit / complete | all four local dialogs, confirmation cancel, input cancel, editor prefill, local notification order differential |
| 531–641 JSONL/stdout events, streamed text, tool/error output, prompt/abort/exit | receive / handle_input / run | delta newlines, agent lifecycle, tool/error frames, prompt trim, abort and ESC exit; exact per-operation screen/command differential |

`example_rpc_extension_ui` executes the complete pinned program with real pinned
Input/SelectList/Container; fake terminal scheduling, child transport and clock
allow deterministic snapshots. `import.meta.url` is replaced by its unchanged source URL before transpilation;
stdout delivery uses real Node readline over private PassThrough bytes. It does
not spawn the pinned client child.
Separate native tests spawn private Python fixture binaries and the existing
`example-rpc-demo-harness` Cargo binary. The latter runs real native AgentSession,
RPC transport, loader/registry/ExtensionRunner and shipped rpc-demo factory:
`/rpc-input` opens the client dialog, an Ada response crosses actual stdin/stdout,
and `You entered: Ada` reaches actual terminal output before ESC cleanup.
No real `pi` process is spawned.

Source bug, preserved: Ctrl+D quit callbacks are unreachable after focus moves
directly to the wrapped Input (source 290,325,383), bypassing PromptInput and
InputDialog handleInput. Whole-program oracle confirms Ctrl+D does not quit;
ESC while idle does. Source help text still advertises Ctrl+D. Native Input
receives Ctrl+D rather than silently correcting this pinned behavior.

RPC OutputLog and subagent preview slices retain exact frozen UTF-16 units through native `JsString` components, including unmatched units and Text padding. No replacement projection is permitted ([compatibility](../../specs/compatibility.md)). The October 22, 2026 decision supersedes the former String-boundary exception.

Proof gaps: real terminal/raw-mode I/O faults, 1000-line eviction, all unknown/
missing optional RPC field combinations, >120-unit result clipping, interval
wall-clock precision and stderr chunk-boundary invalid UTF-8 lack dedicated
fixtures. Process/UI cleanup and successful transport have concrete integration
proof, not live-provider proof. No public core APIs or dependencies were added.

## Q18 native fixture migration (2026-10-22)

Frozen model-result initialization: `packages/coding-agent/examples/extensions/subagent/index.ts:302-310` initializes `currentResult.model` from explicit `agent.model`; line 366 fills a missing model from the assistant message. Rust now uses the same metadata initialization and retains that conditional message fallback. The prior inherited-host-model prefill prevented the actual child model from reaching result details. The existing real-child `modes_dispatch_and_streaming_match_pinned_real_extension_runtime` case was RED (`fixture/inherited` vs `child/model`) before this change.

Native subagent request/result text, JSON values, rendering and UTF-16 preview slices now use `JsString`/`JsValue`. All three renderer differentials now include the split-surrogate preview and compare complete raw-unit lines and padding against frozen TS; no U+FFFD assertion remains.

| Remaining DRIFT | Frozen evidence | Native evidence / proof |
|---|---|---|
| Inherited model/thinking child invocation flags | Frozen subagent `index.ts:295-298` adds `--model` only for explicit `agent.model`, with no inherited `--thinking` injection | Native `run_single` still adds host defaults; real Python child journal compares native/frozen `model` and `thinking` in `modes_dispatch_and_streaming_match_pinned_real_extension_runtime` (RED). Result-model metadata is independently fixed above. |

### GitLab refresh request cancellation (2026-10-22)

Frozen `custom-provider-gitlab-duo/index.ts:277–286` supplies no fetch signal when refreshing OAuth credentials. Native `refresh` now passes `None` at that request boundary; the required native trait token is intentionally unused. Existing full registration/login/refresh differential reproduced `signal:true` versus frozen `false` before the fix (`test-example_gitlab_duo-direct-green-2.log`); assertions are unchanged.

### GitLab raw compat carrier (2026-10-24)

Frozen `custom-provider-gitlab-duo/index.ts:333` assigns literal `forceAdaptiveThinking:true`. The native assignment now stores that same `JsValue::Bool(true)` in the single-authority compat carrier; no condition, request behavior or existing metadata changes.

### GitLab direct-token native JSON boundary (2026-10-22)

Direct-access response JSON is parsed with `parse_lossless`; direct token and header values retain raw `JsString` units. `native_direct_token_tests` uses actual injected HTTP body bytes to prove unmatched token/value retention and rejection of unmatched header-name units before caching (both RED before migration).

| Approved gap | Frozen reference | Native boundary |
|---|---|---|
| Header-name error occurs earlier | Frozen custom-provider-gitlab-duo `index.ts:205,323` would fail during later request header construction | `ProviderHeaders` names remain `String`; `key.to_utf8()` explicitly rejects unpaired units while parsing direct-token response and propagates the conversion error. Values stay native. Header names classified as protocol tokens under opaque-name approval (2026-10-22 parent decision); no replacement or shared adapter added. |

### Q18 RPC client native state (2026-10-22)

RPC client actions, dialog titles/input values, command JSON, streamed text and log buffers now retain `JsString`/`JsValue`. Child argv and terminal/stdout/stderr encode only at actual I/O boundaries. `example_rpc_extension_ui` exercises the same real component/client process boundaries listed above.

`clipped` slices `JsString` directly. `utf16_clipping_preserves_split_surrogate_like_frozen` compares complete screens at widths 19/20/21, before/inside/after a rocket surrogate pair. RED at width 20 recorded native U+FFFD (65533) versus frozen high surrogate (55357); native input/log/JSON paths remain lossless.

### Exact form oracle units (2026-10-22)

`example-forms-oracle.ts` no longer normalizes input or output with `toWellFormed()`. `example_question` and `example_questionnaire` submit D800 through the actual Editor, compare complete raw-unit frames/results against frozen TS, and assert D800 in answer details. Native forms already used `JsString`; only stale projected assertions/oracle normalization required removal.

| Classification | Frozen behavior | Proof |
|---|---|---|
| DISCREPANCY, preserved | An unfinished questionnaire custom answer leaves null; `execute` reads `cancelled` and throws TypeError (`Cannot read properties of null (reading 'cancelled')`) | Final width-1 rocket case in `example_questionnaire`; exact oracle error, replacing obsolete pre-repin RangeError expectation |
| Re-pin correction | Width-1 rocket in question Editor no longer overflows the call stack; unfinished custom answer cancels | Final width-1 case in `example_question` now compares complete frozen output, rather than expecting obsolete RangeError |
