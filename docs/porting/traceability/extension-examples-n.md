# Extension examples N

Pinned source `532f7d12`; native opt-in embedding, no default registration.
Worktree base updated from `5d2454d` to `04f51ba` before extension API implementation.

## Subagent discovery and dispatch

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

## Proof ledger

`/tmp/pi-rust-ext-examples-n/ledger.md`; numbered logs in the same directory.
Only per-example Cargo test targets and formatting; no broad checks or live state.

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
| 72–103 bounded output lines/raw append/UTF-16 slice | OutputLog | exact ANSI-preserving screen differential at widths 20/100; native split-surrogate U+FFFD boundary assertion |
| 109–135 loading interval/dots/start/stop | RpcExtensionUi::tick, run | explicit tick screen differential; real UI event loop |
| 141–249 input, select/input dialogs and help/focus | RpcExtensionUi / Dialog / Focusable | real core components, exact rendered lines and JSON response frames in whole-program differential |
| 255–305 spawn/500ms startup, stdin serialization, exit/kill | run / Invocation | private Python child startup-failure and successful dialog process-lifetime tests; real native rpc-demo harness below |
| 310–465 bottom/loading/dialog UI and all RPC UI request methods | extension_request / receive / complete | select/confirm/input/editor, prefill flattening, notifications/status/widgets/editor text; exact frames and ANSI renders |
| 471–527 four local slash commands | submit / complete | all four local dialogs, confirmation cancel, input cancel, editor prefill, local notification order differential |
| 531–641 JSONL/stdout events, streamed text, tool/error output, prompt/abort/exit | receive / handle_input / run | delta newlines, agent lifecycle, tool/error frames, prompt trim, abort and ESC exit; exact per-operation screen/command differential |

`example_rpc_extension_ui` executes the complete pinned program with real pinned
Input/SelectList/Container; fake terminal scheduling, child transport and clock
allow deterministic snapshots. Only import.meta.url is replaced by its unchanged
source URL before transpilation. It does not spawn the pinned client child.
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

Approved String boundary projection: RPC OutputLog slices and subagent preview
slices project split UTF-16 pairs to U+FFFD before passing text to native String
core components, per compatibility.md:177. Native tests assert this explicitly.
Subagent Text padding therefore differs for this excluded lone-surrogate case;
ordinary scalar render fixtures retain full line differential proof.

Proof gaps: real terminal/raw-mode I/O faults, 1000-line eviction, all unknown/
missing optional RPC field combinations, >120-unit result clipping, interval
wall-clock precision and stderr chunk-boundary invalid UTF-8 lack dedicated
fixtures. Process/UI cleanup and successful transport have concrete integration
proof, not live-provider proof. No public core APIs or dependencies were added.
