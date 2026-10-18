# resource-loader.ts traceability (partial)

Source: `packages/coding-agent/src/core/resource-loader.ts` (1270 lines, pinned revision in
`docs/porting/source-lock.json`). Rust: `crates/pi-coding-agent/src/core/resource_loader.rs`.

## Ported symbols

| TS symbol | Rust item | Tests |
| --- | --- | --- |
| private `firstPartyExtensionName` | `first_party_extension_name` | focused naming cases |
| private `inlineExtensionName` | `inline_extension_name` | focused naming cases |
| private `resolvePromptInput` | `resolve_prompt_input` | focused file/literal cases |
| `loadRulesFilesFromDir` | `load_rules_files_from_dir` | differential `coding-agent-resource-loader-oracle.ts` (order, trim, empty-file skip, missing dir, non-`.md`) |
| `loadRulesFromDir` | `load_rules_from_dir` | same differential request |
| `RulesScope` | `RulesScope` enum | compile-time |

## Blocked symbols

| TS symbol | Blocking module | Owning slice |
| --- | --- | --- |
| `DefaultResourceLoader` (all members) | `extensions/loader`, `extensions/types`, `package-manager`, `modes/interactive/theme/theme`, `footer-data-provider` | ca-package-manager / ca-models / extensions slice |
| `loadProjectContextFiles`, `loadContextFilesFromDir`, `loadInstructionContextFilesFromDirs`, `listAncestorDirs`, `findShadowedContextFile` | `footer-data-provider.findGitPaths` | ca-models |
| `ResourceExtensionPaths`, `ResourceLoaderReloadOptions`, `ResourceLoader`, `DefaultResourceLoaderOptions` | `LoadExtensionsResult`, `PathMetadata`, `Theme`, `InlineExtension` | above |
| `createSourceInfo` use in `extendResources` | `PathMetadata` | ca-package-manager |

No trait seams or stand-in types were added; those symbols wait for their owning modules.

## Notes

- `localeCompare` ordering reuses the existing ICU collator helper in `core::detached_job_retention`
  (made `pub(crate)`); no second collator is constructed.
- `loadRulesFilesFromDir` propagates read errors like the TypeScript `readFileSync` (the source has no
  `try`/`catch` around the read).
