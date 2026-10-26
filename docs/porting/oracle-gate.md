# Frozen oracle and typecheck gate

## Active reference (2026-10-21)

[Source lock](source-lock.json): `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`, read-only `/home/osso/.worktrees/pi-rust-source-4f9e6f8a4`. Frozen runtime and type proofs use these exact unmodified bytes. Explicit approved module deltas use the separate [delta lock](delta-lock.json) below; they do not retarget this common pin. The model catalog is tracked `packages/ai/src/models.generated.ts`; no generated provider JSON snapshot, supplement, network refresh or second source checkout is needed. Old approvals are to re-validate against this pin.

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

## Frozen /v target discrepancy (2026-10-24)

At the common pin, `packages/tui/src/utils.ts:40–42` uses RegExp `/v`, but `tsconfig.base.json` and the TUI/coding-agent build configs select ES2022. Including these unchanged TUI sources in the strict no-emit gate produces TS1501. Runtime selector probes pass. Parent approval: the model-registry/model-selector fixture's temporary no-emit config may select target/lib ES2024; use the same pinned compiler and source bytes. Do not patch source or change the shared builder/default gate. Other fixtures must cite this note when requesting the same scoped adjustment.

## Protected invocation

From the Rust checkout:

```text
python3 scripts/test-sandbox.py -- node oracle/typecheck-reference.mjs check
python3 scripts/test-sandbox.py -- node --test oracle/typecheck-reference.test.mjs oracle/repin-infra.test.mjs oracle/session-name-reference.test.mjs
python3 scripts/test-sandbox.py -- ./run-tests.sh oracle
```

The last command is a gate, not part of this slice's focused proof. Existing public API, diagnostic-count/span, negative fixture and runtime-value assertions remain unchanged; source API differences must fail discovery, not be weakened. `run-tests.sh oracle` validates source selection before compiler checks. All generated fixture/config files stay in private `TMPDIR`; repository source is read-only. Compiler options inherit the frozen source, use no-emit, preserve strictness and existing `skipLibCheck`, and resolve absent package `dist` exports through source mappings. `@pi-oracle-source/*` is a type-only import mapping into the selected tree; missing source modules remain failures.

Cargo runs outside the runtime namespace; compiled tests enter the mandatory runner guard. Never put Cargo itself inside the read-only guard. The guard hides host HOME/config/state/providers, supplies private state and loopback-only networking, and mounts only this checkout, common source, explicitly locked delta checkouts and required runtime inputs. It does not inject `PI_OFFLINE`; offline proofs set it explicitly. Existing private-devpts, capability-drop, parent-watchdog and nested-guard behavior is unchanged.

## Phase 0 proof boundary

Focused infrastructure regressions pass; no acceptance, broad discovery, clippy or readability claim is made. Main owns one discovery after merge: `cargo test --workspace --all-targets --no-fail-fast` through the configured Cargo runner. [Coverage](coverage.md) retains pending proof labels and product deltas. Runtime path/import blockers are recorded in [re-pin infrastructure](repin-infra.md), not repaired through product semantic changes.


## Locked post-pin deltas

[delta-lock.json](delta-lock.json) is a separate list of approved TS deltas, not a
replacement for [source-lock.json](source-lock.json). Each `deltas` entry records
an immutable checkout path, exact commit, approval/base provenance and a `modules`
map of source-relative module paths to SHA256. Additional approved commits can
be appended as separate entries (including a later `/home/osso/.worktrees/pi-delta-26c8050f`
checkout); no filesystem discovery or latest-branch selection occurs.

Before mounting reference inputs, `scripts/test-sandbox.py` verifies every locked
checkout's actual HEAD and every listed module's bytes. Missing checkout/module,
wrong HEAD/hash or Git failure aborts the guard: no skip, fallback or alternate
source. Verified checkouts and linked-worktree Git metadata are exposed read-only
at their actual paths, exactly as the common reference. Parent/nested guards
validate the same lock; writable fixture state stays private. Required
`node_modules` setup may reuse the frozen dependency directory by read-only symlink.

The user-approved Mermaid reference at `532f7d12` is a separate historical lineage, not a descendant of the common pin. Its `base` is the introducing commit `66534fbdc`; the locked module hashes also cover the installed `grok-mermaid` 0.2.2 JavaScript oracle. It does not repin other modules.

Exposure is not selection. Individual differential tests explicitly select a
locked module; unselected oracles continue importing the frozen source. The
resident selector additionally verifies its module immediately before spawning.
Its frozen baseline asserts exactly the approved discrepancy set, while its
delta matrix asserts zero differences, both unignored in ordinary Cargo gates.
See [resident traceability](traceability/resident-session.md).

Focused infrastructure regression: `python3 scripts/test-sandbox.py -- python3
scripts/test_test_sandbox.py`. Tests exercise actual Git repositories and changed/
missing module bytes, read-only delta visibility, private state/network isolation,
required search/Python tools and nested device entropy. This is not the workspace
acceptance gate. The prior ad-hoc resident `/opt` alias wrapper is retired.
