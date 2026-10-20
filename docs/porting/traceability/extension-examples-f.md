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
