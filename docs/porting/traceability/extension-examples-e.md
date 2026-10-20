# Extension examples E

Opt-in native embedding, no default loading. Pinned source:
`532f7d12`, `packages/coding-agent/examples/extensions/`.
Each test includes actual example source with `#[path]`, explicitly registers
its factory, and uses the real Rust loader/runner and UI. Shared fixture
`tests/support/extension_examples_e.rs` reuses `extensions.rs` and `oracle.rs`,
extending the A/B transcript pattern. `oracle/extension-examples-e-oracle.ts`
loads unchanged pinned factories using the real TS loader/runner, in-memory
session manager and faux model registry. No providers or live state.

Tests compare entire runner results, UI traces, commands, errors (excluding
stack frames), and action traces. Temp-root prefixes alone are normalized.
Resource handler results include every path and extension provenance.

| TS source / branches | Native example | Behavioral proof |
|---|---|---|
| `prompt-customizer.ts:24–70` selected tools, missing/empty tools, skill names/order; `74–97` missing/empty/nonempty user append, factory | `examples/extensions/prompt_customizer.rs` | `example_prompt_customizer::complete_prompt_results_match_all_tool_skill_and_append_branches`: 43 real-runner differential cases; includes complete messages/systemPrompt result |
| `claude-rules.ts:31–54` missing directory, recursive traversal, sorted file order, file-only lowercase `.md`; `57–86` scan/update/cache, notification, empty rules, prompt append | `examples/extensions/claude_rules.rs` | `example_claude_rules::rules_are_scanned_recursively_sorted_and_refreshed_only_at_session_start`; `scandir_errors_are_reported_with_node_messages` compares full Node ENOTDIR error |
| `project-trust.ts:17–64` per-load count, all five choices, cancellation, empty/nonempty/cancelled note, no UI, modes, session-start handler | `examples/extensions/project_trust.rs` | `example_project_trust::all_choices_notes_cancellation_and_modes_match_complete_trust_results`: 28 steps, complete trust decision/remember and UI trace |
| `dynamic-resources/index.ts:1–15` module-relative skill/prompt/theme paths, discovery on startup/reload | `examples/extensions/dynamic-resources/index.rs` | `example_dynamic_resources::startup_and_reload_return_complete_paths_with_real_assets`: full returned paths/provenance and byte-identical companion assets |

## Representation and discrepancies

- Dynamic resources: host supplies the resource directory explicitly, the native
  equivalent of `dirname(import.meta.url)`; never use session cwd. Three original
  resource assets accompany the Rust example unchanged.
- JS invalid runtime types remain outside typed Rust API contracts.
- Claude rules preserves source behavior: symlink entries are not traversed;
  failed refresh leaves prior cached files. Existing non-directory roots report
  the Node `scandir` error. No speculative filesystem recovery.
- No semantic deviations or missing extension APIs identified in these modules.

## Proof ledger

`/tmp/pi-rust-ext-examples-e/ledger.md` records exact targeted commands and logs.
Every module first failed for its missing source file. Mutation checks changed
prompt context, rules heading, trust load count, and skill filename respectively;
each caused a real behavioral comparison failure, then original bytes were
restored. Only targeted `cargo test -p pi-coding-agent --test example_<name>` and
formatting are run by this implementer; no clippy/check/workspace acceptance.
