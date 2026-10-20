# Oracle typecheck gate

`./run-tests.sh oracle` runs subprocess regressions, checks registered oracle
entrypoints, then registered public-index/direct-module type proofs. It stops on
failure. `all` requires the same gate before Rust tests. This is not a typecheck
of every oracle or the entire pinned monorepo.

## Two explicitly selected references

[source-lock.json](source-lock.json) retains the original `commit` and `worktree`:
`532f7d12cd19235081bd4d6e425bde185652d86f` at
`/home/osso/.worktrees/pi-rust-source-532f7d12`. This checkout remains read-only.
**Runtime differential oracles always use this original checkout and its existing
model snapshot.** Embedded value probes inside typeproof scripts also use the
original source, `tsx`, and root config; only their temporary import paths change.
No live Pi config, session state, provider calls, or model refresh is involved.

On 2026-10-20 Alessio approved a separate type-only reference correcting two
source/catalog type errors. `typecheckReference` adds its own base, commit, tree,
patch, patch hash and installed-catalog hash without replacing the runtime pin:

| Item | Exact value |
| --- | --- |
| Checkout | `/home/osso/.worktrees/pi-oracle-typecheck-532f7d12` |
| Base/sole parent | `532f7d12cd19235081bd4d6e425bde185652d86f` |
| Reference commit | `16a52a4df1b412e1adcc183b64880a2e625b5418` |
| Reference tree | `2373941ed92e62e2d136f098da19c55c65bbb5c9` |
| Patch SHA-256 | `2436061a2cf24c9b68e8f77e25957835ea6d5f7ed2864f8131d5835c134b7940` |

Patch: [oracle-typecheck-532f7d12.patch](patches/oracle-typecheck-532f7d12.patch).
It changes exactly three type annotations across two files:

- `cloudflare-ai-gateway.ts`: supply `createProvider`'s explicit
  `"anthropic-messages" | "openai-completions" | "openai-responses"` generic.
  Its inferred catalog union otherwise rejects the existing completions entry
  (TS2353 at original line 19).
- `opencode-go.ts`: add `"google-generative-ai"` to the return `Provider` and
  `createProvider` generic unions. The locked catalog already contains that API
  (TS2322 at original line 13). No stream implementation is added, no catalog
  model is removed, and existing missing-stream behavior is unchanged.

The regression test transpiles each original/corrected file with the same
installed TypeScript `transpileModule` options (`target=ES2022`, `module=ESNext`,
`sourceMap=false`, `inlineSourceMap=false`, same filename), then compares emitted
JavaScript **bytes**. A type-only claim does not rest on changed filenames.

## Reproducible setup

Run from the pi-rust checkout:

```text
node oracle/typecheck-reference.mjs setup
node oracle/typecheck-reference.mjs check
```

`setup` creates a separate local shared clone of the original, checks out the
exact base, applies only the tracked patch, and makes the deterministic local
reference commit. Author/committer: `Pi Oracle Reference
<oracle-reference@example.invalid>`; both dates:
`2026-10-20T00:00:00+00:00`; subject:
`Type-only oracle reference for locked provider catalog`. Nothing is committed,
pushed, merged or changed in the original checkout or `/home/osso/Repos/pi`.
The shared clone needs the original object store to remain available. Keep this
reference alive for subsequent verification/integration.

Dependencies are reused through explicit root/package `node_modules` symlinks;
no install/build/refresh runs. Model JSON is copied, not symlinked or regenerated.
The reference's tracked tsconfigs remain independent editable files, not links to
the original. No new dependency is required. The catalog digest is SHA-256 over
all filenames sorted lexically, each followed by NUL, exact file bytes, then NUL
(including `.manifest.json` and the existing Kimi supplement). Both installed
snapshots must match `typecheckReference.modelCatalogSha256`:
`6857c86845b47766339bdf35a5f3e98373b28a31f5eb1c0719a04d679ad44705`.
The existing `modelCatalog` metadata remains unchanged.

`check` and every typeproof use one shared helper,
`oracle/typecheck-reference.mjs`. It validates original HEAD/clean source,
reference HEAD/sole parent/tree/clean source, patch SHA-256, byte-exact
`git diff --binary --full-index --unified=6 <base> HEAD`, dependency link destinations and
both catalog digests. Git reads disable optional locks. Dependency symlinks are
checked separately because upstream directory-only ignores do not ignore them.
Missing references fail with setup instructions; no fallback selects the original.
Stale, dirty or wrong-base checkouts fail rather than being reset automatically.
To rebuild, remove only the disposable reference after checking ownership, then
run setup; never reset or clean the original.

## Strict compiler boundary

Generated configs inherit the selected reference's root/base options and
package-to-source mappings, made absolute for temporary configs. Package exports
point to absent `dist/*.d.ts`; paths target real `src`, not API declaration stubs.
Installed `tsgo` (`7.0.0-dev.20260120.1`), TypeScript and `@types` remain reused.

Checks are strict **no-emit**. Absolute `.ts` fixture imports use
`allowImportingTsExtensions`; `rewriteRelativeImportExtensions` is disabled
because fixtures are never emitted. Existing `skipLibCheck` is inherited, not
expanded to hide source/fixture errors. No diagnostic suppression, `skipErrors`,
`noCheck`, emit-on-error, model filtering, or catalog refresh is used.

`typecheck-report.mjs` propagates numeric exits, launch errors and signals with
both diagnostic streams intact. Negative proofs retain expected-error directives
and compiler-API assertions on exact diagnostic codes/counts/spans; public-index
export, declaration-origin, property and API-shape assertions are unchanged.
Original source diagnostics are not reclassified as expected negatives.

## Protected-runner integration

Pinned patch comparison explicitly selects six context lines, matching the locked patch bytes; it must not inherit host Git `diff.context`. Protected preflight at `de1cc57` exposed that implicit host dependency: 6 Node tests passed, 5 failed before compiler checks (`/tmp/pi-rust-batch-a-guard-verification/REPORT.md`). The patch/reference/catalog hashes and all diagnostic checks remain unchanged. Regression `locked reference verification ignores inherited diff context` checks actual reference acceptance with context 0 and 20; its initial RED is retained in `/tmp/pi-rust-batch-a-oracle-fix/diff-context-red.log`.

Protected oracle configuration and dynamic compiler fixtures are created only in private `TMPDIR`; the repository-relative negative fixture is tracked at `oracle/fixtures/invalid-typecheck.ts`. Its compiler error/path assertion remains intact, without writing into the read-only checkout. Actual protected RED after the context fix: 19/20 Node tests passed; the repository-fixture write failed with EROFS. A separate exact runner `mktemp` operation also failed EROFS. Logs: `/tmp/pi-rust-batch-a-oracle-fix/{protected-oracle-temp-red,config-location-red}.log`.

Run final integration in two phases: `python3 scripts/test-sandbox.py -- ./run-tests.sh oracle`, then Cargo outside the runtime namespace (`cargo test --workspace --all-targets`), whose compiled tests enter the mandatory Cargo runner guard. Do not place Cargo itself inside the runtime guard: checkout/cache are read-only and Cargo is absent from its restricted PATH. No writable-checkout or unsandboxed fallback is introduced.

## Development proof ledger

| Command | Exact scope | Result |
| --- | --- | --- |
| `node --test oracle/typecheck-report.test.mjs` | Original filtering helper at `432cf07` plus regressions | RED: 7 failed, 1 passed |
| `node --test --test-name-pattern=no-emit oracle/typecheck-report.test.mjs` | Before no-emit config repair | RED: TS2877 |
| `./run-tests.sh oracle` | Strict gate repair `6a0295a`, before this slice | 9/9 regressions; entrypoints pass; public-index exit 2, exactly the two original provider errors; later proofs not reached |
| `node --test oracle/typecheck-reference.test.mjs` | New regression before implementation | RED: missing separately pinned `typecheckReference` metadata |
| `node --test oracle/typecheck-reference.test.mjs` | Shared helper/setup and one boundary regression, before caller wiring/additional cases | GREEN: 1/1; original compiler exit 2 with exactly TS2353/TS2322; corrected reference exit 0; both JS byte comparisons pass; missing/dirty/stale/wrong-base references rejected |

| `node --test oracle/typecheck-reference.test.mjs oracle/typecheck-report.test.mjs` | `95254cf` | New reference regressions 3/3 pass; existing reporter regressions 7/9 pass, two negative fixture failures expose the reporter test caller's obsolete generator argument |
| `node oracle/default-thinking-level-typecheck.mjs` | `95254cf` | PASS, exit 0; selected-reference compiler check followed by original-source runtime fixture |
| `cargo fmt --check` | `95254cf` implementation, before commit | PASS, exit 0 |

| `node --test oracle/typecheck-report.test.mjs` | `a50a719` | GREEN: 9/9, including both retained negative fixture assertions |
| `node oracle/coding-agent-truncation-constants-typecheck.mjs` | `a50a719` | RED: its existing literal original-pin assertion compared the selected reference commit against the original commit |

The reporter caller uses the new generator signature and selected reference,
retaining every diagnostic assertion. The truncation proof's literal pin now
checks `typecheckReference.baseCommit`; the shared helper separately checks both
actual HEADs. Its API/diagnostic/runtime assertions remain unchanged, with a new
subprocess regression.

| Command | Exact scope | Result |
| --- | --- | --- |
| `node --test --test-name-pattern=truncation oracle/typecheck-reference.test.mjs` | `b36bcb8` | GREEN: 1/1; exact types/aliases, retained negative diagnostics and original-source runtime imports pass |
| `cargo fmt --check` | Before each implementation/fix commit (`95254cf`, `a50a719`, `b36bcb8`) | PASS, exit 0 each |

The three earlier reference tests at `95254cf` remain valid: subsequent code
changes touch only the reporter test caller and truncation pin check/new test.
Reporter regressions pass 9/9 at `a50a719`. Documentation-only follow-ups do not
invalidate those results. Formatters: installed Biome on new helper/tests and
updated generator/reporter test; unrelated existing whole-file formatting was
left unchanged.
Full `./run-tests.sh oracle` and `./run-tests.sh all` are reserved for the independent
verifier; no new full-gate, broad Rust, lint, build, deployment or final-acceptance
pass is claimed by this slice.
