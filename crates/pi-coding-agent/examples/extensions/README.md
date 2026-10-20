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
