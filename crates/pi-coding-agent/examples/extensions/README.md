# Opt-in Rust-native extension examples

`commands.rs` is embeddable source, not a standalone Cargo executable. Nothing
loads it by default. It demonstrates `ExtensionApi::get_commands()` through
`/commands`; completions suggest `extension`, `prompt`, and `skill` (singular).
Other filters are accepted literally, matching the pinned TypeScript example.

Copy `commands.rs` into your embedding application's `src/extensions/`, then
include it from `src/lib.rs` (or adjust `#[path]` to its actual location):

```rust
#[path = "extensions/commands.rs"]
mod commands;

use std::sync::Arc;
use pi_coding_agent::core::extensions::{ExtensionFactory, ExtensionRegistry};

pub fn register_examples(registry: &mut ExtensionRegistry) {
    let factory: ExtensionFactory = Arc::new(|api| {
        let result = commands::commands_extension(api);
        Box::pin(async move { result })
    });
    registry.register("<example:commands>", factory);
}
```

The host explicitly creates the registry and calls `register_examples`. To load
only that chosen factory using its own cwd, event bus, and extension runtime:

```rust,ignore
use pi_coding_agent::core::extensions::load_extension_from_factory;

let extension = load_extension_from_factory(
    registry.get("<example:commands>").unwrap().clone(),
    cwd,
    event_bus,
    runtime.clone(),
    "<example:commands>",
).await?;
```

Alternatively, `ExtensionRegistry::load_registered` loads the host's explicitly
registered factories. Pass the resulting extension to the host's
`ExtensionRunner`, bind its real `ExtensionActions` (including `get_commands`),
and supply its UI context. `/commands` queries that action on each invocation;
it can select a command and optionally display its source path.

This is native embedding, **not** a dynamic TypeScript loader. Do not copy `.rs`
files into Pi's live extension directories expecting discovery or compilation.
The behavioral test compiles the actual nested example via `#[path]`; no
library re-export or built-in registry entry is necessary.

## Built-in tool renderer

`built_in_tool_renderer.rs` exports `built_in_tool_renderer_extension(&ExtensionApi)`.
Embed via `#[path]` and explicitly register its factory using the pattern above.
It replaces read/bash/edit/write renderers, delegates execution to original tools,
and captures process cwd when the factory loads. Edit renders its own shell.
Nothing loads by default. Differential test: `example_built_in_tool_renderer`.

## Provider payload logging

`provider_payload.rs` exports `provider_payload_extension(&ExtensionApi)`. Embed it
and register its factory explicitly using the same pattern as `commands.rs` above.
It appends request payloads and response headers to
`cwd/CONFIG_DIR_NAME/provider-payload.log`; the config directory must already
exist. Append failures surface through the runner's error listeners. Logs can
contain credentials and private prompts: enable deliberately, never by default.
Tests use private temporary directories only.

## Dynamic tools

`dynamic_tools.rs` exports `dynamic_tools_extension(&ExtensionApi)`. Embed and
register its factory explicitly using the pattern above. It registers
`echo_session` during `session_start`; `/add-echo-tool <name>` registers another
echo tool at runtime. New permitted tool names become active immediately through
the host AgentSession's refresh action. Re-registering an inactive existing name
does not reactivate it. Names allow lowercase letters, numbers, and underscores.

## Batch E: prompts, trust and resources

Embed `prompt_customizer.rs`, `claude_rules.rs`, or `project_trust.rs` via
`#[path]` and register the corresponding `<name>_extension` factory explicitly
using the registry pattern above. Nothing loads by default.

`dynamic-resources/index.rs` exports `dynamic_resources_extension(api, base_dir)`.
Pass the installed directory containing its companion `SKILL.md`, `dynamic.md`,
and `dynamic.json`; it returns these paths during resource discovery. This
explicit native directory replaces TS module-relative `import.meta.url`, not
the session's cwd. Assets are unchanged pinned copies.

`trigger_compact.rs` exports `trigger_compact_extension`; compaction triggers on
upward crossings of 100,000 known tokens or `/trigger-compact [instructions]`.
The host supplies its normal compaction callbacks.

`auto_commit_on_exit.rs` exports `auto_commit_on_exit_extension`; it stages **all**
changes in the session cwd and commits at shutdown. Enable only deliberately.
`git_checkpoint.rs` exports `git_checkpoint_extension`; it creates ephemeral git
stash refs at turns and offers restoration before forks. Both use the host's
`ExtensionApi::exec`; tests run exclusively in private temporary repos with
global/system git config disabled. Neither example loads by default.

## Modes (batch G)

`minimal_mode.rs` exports `minimal_mode_extension(&ExtensionApi)`; register its
factory explicitly using the pattern above. It overrides seven built-in tools'
renderers while executing the built-ins against the invocation context cwd.
`plan_mode/utils.rs` contains pure planning helpers with lossless UTF-16 text.
Full plan-mode registration is not shipped: existing native APIs cannot preserve
the pinned example's mutable custom-entry data aliasing; see
[`extension-examples-g`](../../../../docs/porting/traceability/extension-examples-g.md).

`preset.rs` exports `preset_extension(&ExtensionApi)`. Register its factory
explicitly; it reads global/project `presets.json` on session start. No providers
are called by this example. Config loading failures are printed, matching TS.

## Batch F examples

Embed `with_deps.rs` (`with_deps_extension`) for `parse_duration`, and
`inline_bash.rs` (`inline_bash_extension`) for prompt expansion. Register each
factory explicitly as above. Inline commands execute in the loaded cwd; enable
only for trusted prompts. Native duration parsing matches the example's npm `ms`
string overload without adding dependencies.

`tool_override.rs` (`tool_override_extension`) registers audited `read` and
`/read-log`; audit output goes to the configured agent directory. `todo.rs`
(`todo_extension`) registers `todo` and `/todos`, restoring tool-result state
from the current session branch. Both require explicit factory registration.

`tools.rs` (`tools_extension`) provides `/tools`, with branch-local persisted
selection. `truncated_tool.rs` (`truncated_tool_extension`) provides `rg`, storing
truncated full output in a private temporary directory. Its shell-joined command
matches the pinned example; enable only for trusted tool inputs.

`custom_compaction.rs` (`custom_compaction_extension`) summarizes all prepared
messages using registry model `google/gemini-2.5-flash`. `qna.rs`
(`qna_extension`) extracts questions using the current model into the editor.
Both can make model requests when explicitly enabled; tests use private faux
providers only.

Todo parity is **partial**: native session snapshots cannot reproduce the pinned
example's mutation of stored tool-result details by reference. See
[batch F traceability](../../../../docs/porting/traceability/extension-examples-f.md)
for the blocked API and test.

## Tic-tac-toe (batch K)

Embed `tic_tac_toe.rs` via `#[path]`, keep `tic_tac_toe_instructions.txt` beside
it, and explicitly register `tic_tac_toe_extension(&ExtensionApi)` using the
registry pattern above. `/tic-tac-toe` opens the human/agent game; the two tools
play and inspect the agent cursor. It uses the host's custom UI and session
entries, not live state. Nothing loads by default. Differential test:
`example_tic_tac_toe`; source reconstruction caveat and proof matrix:
[batch K traceability](../../../../docs/porting/traceability/extension-examples-k.md).

## Custom editor and overlay examples

Register these factories explicitly using the native pattern above:

| Module | Factory |
|---|---|
| `border_status_editor.rs` | `border_status_editor_extension` |
| `modal_editor.rs` | `modal_editor_extension` |
| `rainbow_editor.rs` | `rainbow_editor_extension` |
| `overlay_test.rs` | `overlay_test_extension` |

Copy `editor_composition.rs` alongside the three editor modules. It delegates
the real CustomEditor surface while the example overrides input/render behavior;
the host still wires base callbacks and focus. Border-status hides the footer
and working indicator; overlay-test registers `/overlay-test`. No example loads
by default. Behavioral/differential proof: `tests/extension_examples_editors.rs`.
