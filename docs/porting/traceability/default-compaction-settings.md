# DEFAULT_COMPACTION_SETTINGS traceability

Pinned source: `packages/agent/src/harness/compaction/compaction.ts:162-166` and package export `packages/agent/src/index.ts:58-75`, commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| Source | Observable contract | Rust status |
|---|---|---|
| `compaction.ts:162-166` | One exported mutable object with `enabled: true`, `reserveTokens: 16384`, `keepRecentTokens: 20000` | Deferred: safe Rust value boundaries cannot preserve shared mutable singleton identity without an approved representation |
| `index.ts:58-75` | Package export is the object itself, not a factory | No Rust fallback or alternate export added |

## Evidence

`/tmp/pi-rust-default-compaction-settings-probe.out` imports the pinned package export, records the initial values, mutates `reserveTokens` and `keepRecentTokens`, and records the changed object in the same process. This proves mutation is observable through the shared export.

## Discrepancy

A Rust `const` or factory would change identity/mutation semantics. `static mut` would require unsafe access. The discrepancy remains recorded rather than silently selecting either behavior.
