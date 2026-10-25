# extensions/loader.ts → `core::extensions::loader`

Source: `packages/coding-agent/src/core/extensions/loader.ts` (753 lines at frozen `4f9e6f8a4`).
Rust: `crates/pi-coding-agent-tools/src/core/extensions/loader.rs`.

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

## Relative-path audit (2026-10-25)

Whole frozen loader inspection found three ordinary path defects: configured paths and registry lookup used the process cwd instead of the supplied cwd; filesystem-loaded extensions kept the unresolved path as `resolvedPath`; absolute manifest entries were joined below the package instead of replacing it. These now follow frozen resolution and source metadata. Public discovery/loading returns `Result` and propagates path errors rather than panicking or using the unresolved input. Resource-loader and fixture callers propagate or unwrap this mechanically.

RED: actual frozen relative `entry.ts` loads successfully while native reports an invalid factory (`/tmp/ext-api2-loader-red.log`). New differential covers relative loading, source metadata, configured directory and absolute manifest resolution, plus invalid file-URL errors. Post-commit GREEN pending.

## Pre-bind action audit (2026-10-25)

Frozen runtime rejects requestResumeContinuation before binding, while native silently succeeded. Its callTool/callCommand/setModel rejections use the short `Extension runtime not initialized` message; native incorrectly used the long synchronous-action text. Actual frozen/native RED `/tmp/ext-api3-loader-unbound-red2.log` proves all four differences through the real API/runtime. The continuation callback now returns its existing public Result; core binding and six fixture action tables propagate this mechanically. Post-commit focused proof pending.

The Rust-native discovery surface is implemented; `core::resource_loader::DefaultResourceLoader` resolves factories through `ExtensionRegistry` keyed by resolved path or extension id.

## Gaps

- `clearExtensionCache` cannot invalidate a module cache that does not exist.
- `loadExtensionsCached` behaves exactly like `loadExtensions`.
- `discover_and_load_extensions` requires a registry entry keyed by the
  resolved path; bundled extensions use `load_registered` with ids such as
  `<first-party:goal>`.

## Discrepancies

- Corrected directory order: frozen Node readdir uses raw byte-name sorting. Native now sorts before decoding/filtering, preserving discovery/factory precedence. RED `/tmp/ext-api3-loader-order-red.log` compares ASCII, accented, BMP and astral filenames in their exact loaded order. Package-manager and theme discovery share this correction; rule locale ties retain the initial byte order. Parent approved the cross-lane package-manager helper change; no manifest workaround is used.

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

## Q18 native UTF-16 continuation (2026-10-22)

Registry keys, IDs, paths, cwd, disabled-name candidates, source metadata, and
factory-load errors use `JsString`. Discovery joins decoded directory entry names
to the original native directory, rather than round-tripping the encoded path.
Only filesystem operations encode; timing labels stay native.

`lossless_extension_loader` covers unmatched-unit inline names, disabled path
identity, and distinct raw/replacement registry keys. RED compilation was blocked
by 1877 coding-library errors; behavior remains unexecuted. Diagnostic probe
`replacement-extensions-1` found two local issues, corrected after the probe.
No fresh frozen parity or runtime PASS claimed. Scalar command descriptions stay
with the UI/runner lane; shortcut keys use that lane's native KeyId.
