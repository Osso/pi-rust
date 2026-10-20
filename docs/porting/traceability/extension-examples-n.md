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
| index.ts:444–721 schema/registration, exclusive modes, project trust, sequential substitution/failure, parallel limits/output | subagent_extension / execute / run_chain / run_parallel | 11 full-result differentials; UI confirmations; native refusal without spawn; update counts |
| index.ts:32–263,723–1033 call/result rendering, tools, usage, single/chain/parallel | subagent/render.rs | example_subagent renderer differential: actual native components vs pinned components, ANSI-stripped terminal lines; expanded/collapsed and every tool formatting branch |

`example_subagent_agents` tests the companion module directly (no extension entry
exists there). `example_subagent` loads the shipped entry through explicit native
registry and `ExtensionRunner`; TS oracle loads the unchanged extension through
its real loader/runtime/runner. Only child executable and host UI/model/thinking
are fixture seams. The child is Python, never `pi`/Node CLI; both parsers consume
real process pipes. All invocations occur beneath Cargo's sandbox guard.

Approved isolation delta: partial results own JSON snapshots rather than retaining
pinned mutable object aliases. Tests compare update counts and final results, not
aliased intermediate snapshots. No API additions/dependencies.

Proof gaps: exact ANSI theme bytes, concurrency peak/timing, ignored-SIGTERM child,
per-task 10 KiB multibyte truncation, non-UTF8 stdout chunk splitting, optional cwd
override and every renderer limit/error branch lack independent fixtures. Native
parallel dispatch uses four permits and an eight-task cap; max-task and mixed
success results have differential evidence. Typed frontmatter strings/native
scope input exclude invalid JavaScript runtime types, not valid string values.

## Proof ledger

`/tmp/pi-rust-ext-examples-n/ledger.md`; numbered logs in the same directory.
Only per-example Cargo test targets and formatting; no broad checks or live state.
