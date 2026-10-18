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
