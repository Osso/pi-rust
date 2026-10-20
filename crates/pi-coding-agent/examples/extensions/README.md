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

## Provider payload logging

`provider_payload.rs` exports `provider_payload_extension(&ExtensionApi)`. Embed it
and register its factory explicitly using the same pattern as `commands.rs` above.
It appends request payloads and response headers to
`cwd/CONFIG_DIR_NAME/provider-payload.log`; the config directory must already
exist. Append failures surface through the runner's error listeners. Logs can
contain credentials and private prompts: enable deliberately, never by default.
Tests use private temporary directories only.

## Modes (batch G)

`minimal_mode.rs` exports `minimal_mode_extension(&ExtensionApi)`; register its
factory explicitly using the pattern above. It overrides seven built-in tools'
renderers while executing the built-ins against the invocation context cwd.
`plan_mode/utils.rs` contains pure planning helpers used by the plan-mode example.

`preset.rs` exports `preset_extension(&ExtensionApi)`. Register its factory
explicitly; it reads global/project `presets.json` on session start. No providers
are called by this example. Config loading failures are printed, matching TS.
