# CURRENT_SESSION_VERSION traceability

Pinned source: `packages/coding-agent/src/core/session-manager.ts:52` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `session-manager.ts:52` | Export constant value `3`; the surrounding session header declares `version?: number` without integer validation. | `pi_coding_agent::core::session_manager::CURRENT_SESSION_VERSION: f64 = 3.0`; focused proof preserves the value without integer narrowing. |
| `index.ts:241` | Package index re-exports `CURRENT_SESSION_VERSION`. | Rust re-exports the constant from the crate root; `core` intentionally does not aggregate-re-export it. |
| Differential request `13179` | Compare the actual direct-module value against Rust. | The dedicated test sends exactly one request with operation `current_session_version`; Rust module/root values match the direct-module oracle. |
| Public-index proof | Preserve the exact exported literal type and runtime value. | The pinned compiler API verifies the real package-index alias resolves to `session-manager.ts` with exact number-literal type `3`; a pinned `tsx` probe imports the real package index and returns runtime value `3`. |

## Boundary

`CURRENT_SESSION_VERSION` is a value-only slice. Rust uses `f64` because the source-facing session header accepts unrestricted TypeScript `number` values and supplies no integer validation; the current literal `3` does not establish an integer-only public contract.

Production commit: `1b78900`. Differential/public-index proof commit: `c039d67`.

## Focused proof status

- Focused RED: `/tmp/pi-rust-current-session-version-red.out` records clean base `837bb0f`, exit `101`, with only the missing module and crate-root exports.
- Exact committed focused GREEN: `/tmp/pi-rust-current-session-version-focused-c039d67.out` records `1 passed, 0 failed` at `c039d67`.
- Differential RED: `/tmp/pi-rust-current-session-version-differential-red.out` records the exact production base `1b78900`, exit `101`, and structured unsupported-operation response for request `13179`.
- Exact committed differential GREEN: `/tmp/pi-rust-current-session-version-differential-c039d67.out` records `1 passed, 0 failed` at `c039d67`.
- Public-index proof: `/tmp/pi-rust-current-session-version-typeproof-c039d67.out` records compiler `5.9.3`, zero scoped diagnostics, exact literal type `3`, and runtime package-index value `3`.
- Direct oracle source typecheck: `/tmp/pi-rust-current-session-version-oracle-typecheck-c039d67.out` exits 0.
- Changed Rust/TypeScript/JavaScript readability passes without suppressions or unrelated behavior.

## Final integration proof

- Code/proof/docs HEAD: `ca1c4ca`; focused production commit: `1b78900`; differential/public-index commit: `c039d67`.
- `/tmp/pi-rust-current-session-version-final-workspace-tests.out` records `cargo test --workspace --all-targets` exiting 0 with `280 passed, 0 failed, 2 ignored`.
- `/tmp/pi-rust-current-session-version-final-workspace-check.out` and `/tmp/pi-rust-current-session-version-final-workspace-clippy.out` record workspace check and all-target/all-feature Clippy with `-D warnings` exiting 0.
- `/tmp/pi-rust-current-session-version-final-oracle.out`, `/tmp/pi-rust-current-session-version-final-typecheck.out`, `/tmp/pi-rust-current-session-version-final-fmt.out`, and `/tmp/pi-rust-current-session-version-final-diff.out` exit 0 at `ca1c4ca`.
- `/tmp/pi-rust-current-session-version-final-readability.out` records the final changed Rust/TypeScript/JavaScript readability audit passing without suppressions or new violations.
- `/tmp/pi-rust-current-session-version-final-audit.out` confirms exact scope/export boundaries, request `13179`, truthful nonzero RED artifacts, documentation paths, no Cargo manifest/lock or deployment change, and clean Rust/source pins.
- Inherited independent verification at clean Rust HEAD `ca1c4ca` returns `OVERALL: PASS`.

No dependency change or deployment applies to this slice.

## Exclusions

Session-manager implementation, session parsing, persistence, migration, storage, lifecycle, orchestration, unrelated constants, constructors, serialization, validation, fallback behavior, protocol changes, dependencies, and unrelated APIs remain unported.
