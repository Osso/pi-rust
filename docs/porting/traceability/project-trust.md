# project-trust.ts traceability (blocked)

Source: `packages/coding-agent/src/core/project-trust.ts` (96 lines, pinned revision in
`docs/porting/source-lock.json`). Rust: not ported in this slice.

## Blocked symbols

| TS symbol | Blocking module | Notes |
| --- | --- | --- |
| `ResolveProjectTrustedOptions` | `extensions/types.ts` (`LoadExtensionsResult`, `ProjectTrustContext`) | the options shape embeds both extension contracts |
| `resolveProjectTrusted` | `extensions/runner.ts` (`emitProjectTrustEvent`) | the extension `project_trust` hook is the first decision source |
| private `selectProjectTrustOption` | `ProjectTrustContext.ui.select` (extension UI contract) | async UI selection; no Rust UI contract yet |
| private `saveProjectTrustPromptResult` | ported `core::trust_manager` store | trivial wrapper over the ported store, but only reachable through the blocked selection path |
| private `formatProjectTrustPrompt` | none (pure string) | held with the module to avoid exporting an unreachable private helper |
| `AppMode` | none | unrelated declaration in the same file; no consumer in this slice |

`trust-manager.ts` (`getProjectTrustOptions`, `hasTrustRequiringProjectResources`,
`ProjectTrustStore`) is already ported in `crates/pi-coding-agent/src/core/trust_manager.rs`.

No trait seams, stand-in extension types, or duplicated `emitProjectTrustEvent` were added; the module
waits for the extensions slice. No differential test is possible until the extension runner exists.
