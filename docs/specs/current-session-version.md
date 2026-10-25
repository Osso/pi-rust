# CURRENT_SESSION_VERSION

This specification covers `packages/coding-agent/src/core/session-manager.ts:52` at pinned TypeScript commit `532f7d12cd19235081bd4d6e425bde185652d86f`. The coding-agent package re-exports the constant from `packages/coding-agent/src/index.ts:241`. Focused production is committed as `1b78900`; differential work is at `c039d67`.

## What it must do

- [x] Expose `CURRENT_SESSION_VERSION` from `pi_coding_agent::core::session_manager` and re-export it from the crate root.
- [x] Preserve value `3.0` as `f64`.
- [x] Preserve the package export without adding a `core` aggregate re-export.
- [x] Keep the source session-header contract explicit: `version?: number` has no integer validation, so the literal source constant does not justify a narrower Rust type.
- [x] Compare the source value through exactly one differential request, `13179`.
- [x] Prove the exact public-index literal type and runtime value through the real pinned package index.

## How it works

- [CURRENT_SESSION_VERSION traceability](../porting/traceability/current-session-version.md)

## Implementation inventory

- `crates/pi-coding-agent-storage/src/core/session_manager.rs` — constant-only module boundary.
- `crates/pi-coding-agent-core/src/core/mod.rs` — public module exposure without aggregate re-export.
- `crates/pi-coding-agent/src/lib.rs` — crate-root re-export.
- `crates/pi-coding-agent/tests/current_session_version.rs` — focused value/export proof.
- `crates/pi-coding-agent/tests/differential_current_session_version.rs` and `.../mod.rs` — one-request differential proof.
- `oracle/current-session-version-oracle.ts` — direct-module JSONL oracle.
- `oracle/current-session-version-typecheck.mjs` — symbol-scoped real-index compiler and pinned `tsx` runtime proof.
- `run-tests.sh` — focused, differential, oracle, and typeproof registration.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/current_session_version.rs`
- `crates/pi-coding-agent/tests/differential_current_session_version.rs`
- `oracle/current-session-version-typecheck.mjs`

## Bounded proof status

- [x] Production commit `1b78900`; focused missing-API RED and exact committed GREEN pass.
- [x] Differential/public-index proof commit `c039d67` adds exactly request `13179`; structured unsupported-operation RED and `1 passed, 0 failed` GREEN pass.
- [x] The real pinned package index exposes an alias resolving to `session-manager.ts`, with exact literal type `3` and runtime value `3`; direct oracle typecheck and changed-file readability pass.
- [x] Final integration at code/proof/docs HEAD `ca1c4ca`: workspace `280 passed, 0 failed, 2 ignored`; workspace check, all-target/all-feature Clippy with `-D warnings`, aggregate oracle/typeproof, dedicated typeproof, formatting, slice-range diff, readability, documentation, dependency/lock, scope, and clean-pin gates pass.
- [x] Inherited independent verification at `ca1c4ca` confirms focused `1/0`, differential request `13179` `1/0`, exact literal/runtime value `3`, truthful RED artifacts, clean Rust/source worktrees, and `OVERALL: PASS`.

## Out of scope

Session-manager implementation, session parsing, persistence, migration, storage, lifecycle, orchestration, unrelated constants, constructors, serialization, validation, fallback behavior, protocol changes, dependencies, and unrelated APIs remain excluded.
