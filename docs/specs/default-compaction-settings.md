# DEFAULT_COMPACTION_SETTINGS

This specification records the unresolved boundary for the exported `DEFAULT_COMPACTION_SETTINGS` object from `packages/agent/src/harness/compaction/compaction.ts:162-166`.

## Required source behavior

- Export a single object with mutable fields `enabled`, `reserveTokens`, and `keepRecentTokens`.
- Preserve shared object identity and mutations through the package export.
- Initial values are `true`, `16384`, and `20000`.

## Current discrepancy

No approved safe Rust API representation currently preserves both direct package-level object access and shared mutable identity:

- A Rust `const` value is copied at each use and cannot provide one shared mutable object identity.
- A function returning `CompactionSettings` changes the export shape and identity semantics.
- `static mut` could model shared mutation only through unsafe access, which is outside the approved safe Rust boundary.

The pinned runtime probe `/tmp/pi-rust-default-compaction-settings-probe.out` reports mutation output `reserveTokens: 0`, `keepRecentTokens: null`, and `sameObject: true`. `null` is invalid at the approved typed Rust boundary and intentionally excluded. No production Rust code is added until an approved representation exists.

## Out of scope

Context-token calculation, context estimation, compaction orchestration, and unrelated compaction APIs remain deferred.
