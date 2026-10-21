# Frozen oracle and typecheck gate

## Active reference (2026-10-21)

[Source lock](source-lock.json): `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`, read-only `/home/osso/.worktrees/pi-rust-source-4f9e6f8a4`. Both runtime and type proofs use these exact unmodified bytes. The model catalog is tracked `packages/ai/src/models.generated.ts`; no generated provider JSON snapshot, supplement, network refresh or second source checkout is needed. Old approvals are to re-validate against this pin.

`oracle/typecheck-reference.mjs` verifies frozen HEAD, clean source, compiler/tsconfig availability, tracked catalog identity and exact catalog bytes. `setup` only validates; it never clones, patches, installs or mutates source. Missing or mismatched inputs fail without alternate-source fallback. Runtime fixture preparation verifies that type and runtime selections are identical and leaves the fixture unchanged. No source transformation exists, so emitted-JavaScript identity is trivial: type/runtime import the same files. Historical patched-source transpile comparisons are no longer applicable.

## Retired patch and need assessment

The old two-file type-only [patch](patches/oracle-typecheck-532f7d12.patch) cannot be applied to the new pin. Read-only `git apply --check` returned exit 1:

```text
error: patch failed: packages/ai/src/providers/cloudflare-ai-gateway.ts:6
error: packages/ai/src/providers/cloudflare-ai-gateway.ts: patch does not apply
error: patch failed: packages/ai/src/providers/opencode-go.ts:2
error: packages/ai/src/providers/opencode-go.ts: patch does not apply
```

No replacement patch or semantic source edit was attempted. A targeted strict no-emit compiler probe using `buildSourceTsconfig` on the new coding-agent public index plus its tracked `utils/highlight-js-lib-index.d.ts` passed, exit 0. The old provider errors TS2353/TS2322 are absent. Therefore an equivalent patched reference is not needed for that compiler boundary. The first index-only probe exposed TS7016 because its temporary config omitted the source's ambient declaration; the shared builder now includes source `packages/*/src/**/*.d.ts`, without suppressing diagnostics or changing declarations.

Probe commands/file sets and full outputs: `/tmp/pi-rust-repin-infra/typecheck-new-pin-{probe,with-declarations}.log`; detailed command ledger: `/tmp/pi-rust-repin-infra/ledger.md`. Reproducible regression: `oracle/typecheck-reference.test.mjs` compiles the public index with the same builder, then proves a concrete invalid fixture still yields exactly TS2322. Fixture mutation tests reject wrong HEAD, dirty catalog and absent compiler. `oracle/repin-infra.test.mjs` proves lock-selected type-import resolution with a real compiler and rejects a string-to-number mutation.

## Protected invocation

From the Rust checkout:

```text
python3 scripts/test-sandbox.py -- node oracle/typecheck-reference.mjs check
python3 scripts/test-sandbox.py -- node --test oracle/typecheck-reference.test.mjs oracle/repin-infra.test.mjs oracle/session-name-reference.test.mjs
python3 scripts/test-sandbox.py -- ./run-tests.sh oracle
```

The last command is a gate, not part of this slice's focused proof. Existing public API, diagnostic-count/span, negative fixture and runtime-value assertions remain unchanged; source API differences must fail discovery, not be weakened. `run-tests.sh oracle` validates source selection before compiler checks. All generated fixture/config files stay in private `TMPDIR`; repository source is read-only. Compiler options inherit the frozen source, use no-emit, preserve strictness and existing `skipLibCheck`, and resolve absent package `dist` exports through source mappings. `@pi-oracle-source/*` is a type-only import mapping into the selected tree; missing source modules remain failures.

Cargo runs outside the runtime namespace; compiled tests enter the mandatory runner guard. Never put Cargo itself inside the read-only guard. The guard hides host HOME/config/state/providers, supplies private state and loopback-only networking, and mounts only this checkout, common source and required runtime inputs. It does not inject `PI_OFFLINE`; offline proofs set it explicitly. Existing private-devpts, capability-drop, parent-watchdog and nested-guard behavior is unchanged.

## Phase 0 proof boundary

Focused infrastructure regressions pass; no acceptance, broad discovery, clippy or readability claim is made. Main owns one discovery after merge: `cargo test --workspace --all-targets --no-fail-fast` through the configured Cargo runner. [Coverage](coverage.md) retains pending proof labels and product deltas. Runtime path/import blockers are recorded in [re-pin infrastructure](repin-infra.md), not repaired through product semantic changes.
