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
