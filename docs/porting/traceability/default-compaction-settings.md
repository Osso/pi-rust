# DEFAULT_COMPACTION_SETTINGS traceability

Pinned source: `packages/agent/src/harness/compaction/compaction.ts:162-166` and package export `packages/agent/src/index.ts:58-75`, commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| Source | Observable contract | Rust status |
|---|---|---|
| `compaction.ts:162-166` | One exported mutable object with `enabled: true`, `reserveTokens: 16384`, `keepRecentTokens: 20000` | Deferred: no approved safe Rust API representation currently preserves both direct package-level object access and shared mutable identity |
| `index.ts:58-75` | Package export is the object itself, not a factory | No Rust fallback or alternate export added |

## Evidence

`/tmp/pi-rust-default-compaction-settings-probe.out` imports the pinned package export, records the initial values, mutates `reserveTokens` and `keepRecentTokens`, and records `reserveTokens: 0`, `keepRecentTokens: null`, and `sameObject: true` in the same process. This proves mutation is observable through the shared export. `null` is invalid at the approved typed Rust boundary and intentionally excluded.

## Discrepancy

A Rust `const` value is copied at each use and cannot provide one shared mutable object identity; a factory changes export shape and identity semantics. `static mut` would require unsafe access. The discrepancy remains recorded rather than silently selecting either behavior.
