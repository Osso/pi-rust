# Extension examples F

Opt-in embeddable sources, explicitly registered factories; never default-loaded.
Tests use the real Rust ExtensionRunner and pinned TS loader/runner via extended
batch B support, not replacement/mock extension implementations. Complete tool
results and input handler objects, metadata, UI notifications, errors compared.
Only private temporary cwd/state. No added dependencies.

| Source / lines | Rust | Behavioral proof |
|---|---|---|
| with-deps/index.ts 12–32 | with_deps.rs::with_deps_extension, parse_duration | example_with_deps: 674 valid/invalid durations, all unit aliases/case/spacing/fractions/signs, length bounds, empty-error text, numeric rendering, full tool schema/results/errors |
| inline-bash.ts 19–94 | inline_bash.rs::inline_bash_extension | example_inline_bash: 40 inputs through real runner, three sources, images, duplicate patterns, stdout/stderr/exit selection, JS replacement substitutions, previews, headless/whole-line bypass |

| tool-override.ts 30–66, 68–129 | tool_override.rs::register_with_clock, read_text, slice_index, log_access | example_tool_override: every blocked pattern, allowed/missing/directory/invalid UTF-8, fractional/negative/zero slicing, multibyte truncation, complete results and metadata |
| tool-override.ts 131–144 | tool_override.rs read-log command | example_tool_override: missing log and last 20 entries, actual private audit file |
| todo.ts 105–219 | todo.rs::todo_extension, restore, execute_todo, toggle | example_todo: list/add/toggle/clear/unknown, required text/id errors, state reconstruction from actual session tool results |
| todo.ts 38–103, 221–297 | todo.rs::TodoList, render_result, render_list | example_todo: all result branches, expanded/collapsed lists >5 items, 16/40-column tool output, 24/40-column UI, both close keys and non-TUI refusal |

| tools.ts 21–64, 137–146 | tools.rs::tools_extension, restore | example_tools: active-tool ordering, last valid saved config, absent-tool filtering/dedup, persistence and real branch navigation |
| tools.ts 66–135 | tools.rs::ToolSelector | example_tools: native SettingsList rendering at 24/40 columns, up/down/wrap/toggle/cancel including remapped j/k navigation, immediate setActiveTools and appendEntry effects, empty list and non-TUI refusal |
| truncated-tool.ts 33–135 | truncated_tool.rs::execute, run_search | example_truncated_tool: real rg match/no-match/invalid regex/missing path, glob, line/byte truncation, complete results, complete saved artifact bytes, >100MiB buffer failure |
| truncated-tool.ts 137–195 | truncated_tool.rs call/result renderers | example_truncated_tool: call path/glob, partial, empty, truncated, expanded 20-line limit, full-output link, fixed-width lines |

| custom-compaction.ts 20–75 | custom_compaction.rs::custom_compaction_extension | example_custom_compaction: full conversation + turn prefix, previous-summary prompt, en-US grouped/rounded decimal and exponent token notifications, missing model |
| custom-compaction.ts 77–117 | custom_compaction.rs completion/result handling | example_custom_compaction: private faux completion, full request context, maxTokens/cacheRetention/UUID-v7/signal, complete compaction result including usage, text-only aggregation, empty/abort/error paths |
| qna.ts 14–71 | qna.rs::qna_extension, find_assistant_text | example_qna: exact system prompt, mode/model/missing assistant/incomplete guards, reverse branch search and skipped thought-only messages |
| qna.ts 73–118 | qna.rs::QnaLoader, extraction task | example_qna: native BorderedLoader output, full faux request context, text-only response/editor payloads, empty/abort/provider error/cancel results, complete notifications |

Paths above are relative to crates/pi-coding-agent/examples/extensions.

## Decisions / discrepancies
- `ms` string overload only: the example's Type.String schema never exercises
  numeric short/long formatting; returned numeric milliseconds use shared JS
  number formatting. Native parsing preserves npm ms grammar and its empty-string
  exception. No duration crate or npm dependency is added to Rust.
- Inline bash uses the existing native exec API, 30-second timeout and loaded cwd.
  Its pinned command explicitly invokes bash; no developer shell commands are run.
- JS invalid runtime input types remain excluded per project rules.

Mutation proof and exact command logs: /tmp/pi-rust-ext-examples-f/ledger.md.

- Audited-read source deliberately ignores logging failures (lines 53–59), reports
  read errors as ordinary tool results (no isError), and checks byte length before
  slicing UTF-16 units (111–113). These source behaviors are retained, not fixed.
  Directory-error text follows the pinned runtime's observed fs/promises message.
- Todo returns shallow snapshots in TS. Oracle snapshots results at the observed
  execution boundary, before later mutations; both tool payload and full details
  are compared. Cache omission in native TodoList changes no visible output.

- ToolSelector keeps the selected index, observed navigation key, and thread-safe tool state across
  the extension boundary. Rc-based native SettingsList is reconstructed locally
  with that selection for each render/input; its rendering and keybindings are
  reused rather than reimplemented. No extension API additions.
- Truncated-tool preserves the pinned shell-joined command (including its unsafe
  quoting behavior); do not enable for untrusted inputs. Its 100MiB combined
  stdout/stderr limit is enforced while reading and kills/reaps the child group.
- Artifact differentials validate the actual temporary path shape and read every
  saved byte before deleting the private directory. Only randomized directory
  identity is alpha-renamed in both complete payloads; paths are not omitted.

- Model tests register actual faux providers on isolated ModelRuntime instances;
  no live provider/config/DB access. Native request timestamps are replayed into
  the TS clock. Both independent UUID values are validated as UUID-v7 and their
  random identity is alpha-renamed in captured provider options, not omitted.
- QnaLoader reconstructs/disposes the native stateless BorderedLoader on the UI
  thread, retaining request cancellation across the Send+Sync boundary. The
  loader's pinned empty animation frames require no persistent timer state.
  Async completion runs on the host Tokio runtime; UI done resolves once.

## Blocked Todo source-aliasing behavior

**Todo is partial; no semantic deviation approved.**

Pinned `todo.ts:124–126` assigns `todos = details.todos`; later `push` (165) and
`todo.done = !todo.done` (191) mutate the same arrays/objects held by session
entries. Pinned `session-manager.ts:1991–1996` returns original entry references.
This is a classified source aliasing bug: revisiting the same entry can retain
new todos while restoring the old nextId, producing duplicate IDs.

Rust `crates/pi-coding-agent/src/core/session_manager.rs:3019–3027` clones branch
entries; `get_entry` at 2847 returns only `&SessionEntry`. No exposed shared entry
or arbitrary mutable message-details API exists. A private extension-side overlay
would not mutate observable session state and is not an equivalent port. No core
API changes or such workaround were made.

`example_todo::todo_stored_details_aliases_survive_branch_back_and_reload`
reproduces restore → add → restore different entry → branch back → add → reload.
It remains explicitly ignored with the missing API named; forcing it fails
against the real pinned TS runtime. Normal Todo tool results/UI/renderers remain
proven by the non-ignored differential, not whole-module parity.
