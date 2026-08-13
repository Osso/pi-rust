# DEFAULT_COMPACTION_SETTINGS

This specification records the unresolved boundary for the exported `DEFAULT_COMPACTION_SETTINGS` object from `packages/agent/src/harness/compaction/compaction.ts:162-166`.

## Required source behavior

- Export a single object with mutable fields `enabled`, `reserveTokens`, and `keepRecentTokens`.
- Preserve shared object identity and mutations through the package export.
- Initial values are `true`, `16384`, and `20000`.

## Current discrepancy

No Rust representation is approved in the typed core:

- `const CompactionSettings` produces a fresh value rather than the exported shared object.
- A function returning `CompactionSettings` changes the export shape and identity semantics.
- `static mut` could model shared mutation only through unsafe access, which is outside the approved safe Rust boundary.

The pinned runtime probe `/tmp/pi-rust-default-compaction-settings-probe.out` confirms mutation of the exported object persists within the source process. No production Rust code is added until an approved representation exists.

## Out of scope

Context-token calculation, context estimation, compaction orchestration, and unrelated compaction APIs remain deferred.
