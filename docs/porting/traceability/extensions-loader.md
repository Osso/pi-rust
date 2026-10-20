# extensions/loader.ts → `core::extensions::loader`

Source: `packages/coding-agent/src/core/extensions/loader.ts` (790 lines).
Rust: `crates/pi-coding-agent/src/core/extensions/loader.rs`.

Approved deviation: TypeScript module loading (`jiti`, virtual modules,
workspace aliases, factory cache) is not ported. Rust extensions are static
factories registered in `ExtensionRegistry`; discovery, enabled/disabled
filtering, load order, and diagnostics text follow the source.

## Symbols → Rust

| Source | Rust | Tests |
| --- | --- | --- |
| `VIRTUAL_MODULES`, `resolveAliasEntries`, `createAliases`, `getAliases` | not ported (module loading deviation) | — |
| `clearExtensionCache` | `clear_extension_cache` (no-op; no module cache) | compile-time |
| `createExtensionRuntime` | `create_extension_runtime` (re-exported from `extension_runtime`) | runner tests |
| `createExtensionAPI` | `ExtensionApi::new` + typed `on_*`/registration methods | differential |
| `loadExtensionModule`, `loadExtension` | `ExtensionRegistry::load_extension` + `load_extension_from_factory` | discovery tests |
| `createExtension` | `create_extension` (source/baseDir synthetic source info) | discovery tests |
| `loadExtensionFromFactory` | `load_extension_from_factory` | all runner tests |
| `loadExtensionsInternal`, `loadExtensions`, `loadExtensionsCached` | `load_extensions`, `load_extensions_cached` (identical for static factories) | discovery tests |
| `isExtensionFile` | `is_extension_file` | discovery tests |
| `resolveExtensionEntries` | `resolve_extension_entries` | `package_manifest_entries_keep_declared_order` |
| `discoverExtensionsInDir` | `discover_extensions_in_dir` | `discovers_direct_files_and_index_entries` |
| `discoverAndLoadExtensions` | `discover_extension_paths`, `ExtensionRegistry::discover_and_load_extensions`, free `discover_and_load_extensions` | discovery tests |
| — (resource-loader filtering) | `is_extension_path_disabled`, `is_extension_factory_disabled`, `inline_extension_name` | `disabled_extension_names_filter_paths_and_inline_ids` |
| — (static registry) | `ExtensionRegistry::{register, register_hidden, get, ids, load_registered}` | `registered_extensions_honor_hidden_and_disabled_ids` |

Diagnostics text preserved:

- `Extension does not export a valid factory function: {path}`
- `Failed to load extension: {message}`

## Remaining work

None. The Rust-native discovery surface is complete; `core::resource_loader::DefaultResourceLoader`
resolves factories through `ExtensionRegistry` keyed by resolved path or extension id.

## Gaps

- `clearExtensionCache` cannot invalidate a module cache that does not exist.
- `loadExtensionsCached` behaves exactly like `loadExtensions`.
- `discover_and_load_extensions` requires a registry entry keyed by the
  resolved path; bundled extensions use `load_registered` with ids such as
  `<first-party:goal>`.

## Discrepancies

- Directory enumeration order comes from the OS in both implementations;
  discovery tests assert membership and manifest order rather than readdir
  order.

## Live registration identity (2026-10-20)

`Extension` is a cloneable handle to one `Arc<Mutex<ExtensionState>>`.
`load_extension_from_factory` returns that handle, not a cloned registry record.
API, loader metadata enrichment, runner, and session resource clones therefore
observe the same state. Mutex guards must not span callbacks or awaits.

| Registration | TS loader evidence | Live Rust reader / proof |
| --- | --- | --- |
| tools | `registerTool` mutates `extension.tools`, calls `runtime.refreshTools()` | runner `get_all_registered_tools` / `get_tool_definition`; `runtime_registration_from_command_is_visible_to_runner` |
| commands | `registerCommand` mutates `extension.commands` | runner command resolution; runtime registration test executes late command |
| flags | `registerFlag` mutates flags; default only if absent | runner `get_flags`, runtime flag values; runtime registration test |
| shortcuts | `registerShortcut` mutates shortcuts | runner `get_shortcuts`; runtime registration test |
| message / entry renderers | register methods mutate corresponding maps | runner renderer lookups; runtime registration test |
| markdown transformer | replaces `extension.markdownTransformer` | runner `get_markdown_transformers`; runtime registration test |
| event handlers | `on` appends to the same event list | all runner dispatch paths read callbacks from shared state without holding a guard during callbacks |
| approval reviewers / tool gates | registration appends to corresponding arrays | runner presence checks and dispatch read shared lists; existing gate/reviewer behavior tests |
| providers / native providers / unregister | delegates directly to shared runtime | no extension snapshot involved; queued/immediate provider tests unchanged |
| event bus subscriptions | delegates to shared event bus/runtime subscription tracking | no extension snapshot involved; existing event-bus tests |

Normal callback arrays use live index iteration (including append during dispatch),
matching JS `for...of`. `emitRuntimeMailbox` intentionally gathers callbacks before
dispatch, matching TS `flatMap` at runner.ts:1104–1106.
Tool activation policy remains in `AgentSession::refresh_tool_registry`: registration
immediately refreshes definitions; newly seen permitted names become active;
re-registering an inactive existing name does not reactivate it. Source:
agent-session.ts:5096–5196. No extra synchronization or notification path added.
