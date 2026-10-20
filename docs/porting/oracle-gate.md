# Oracle typecheck gate

`./run-tests.sh oracle` runs subprocess regression tests, checks the registered
oracle entrypoints, then runs the registered public-index/direct-module type
proofs. It stops on failure. `all` requires the same gate before Rust tests.
The gate is not a typecheck of every oracle or the entire pinned monorepo.

## Supported boundary

- Source and dependencies come from [source-lock.json](source-lock.json).
  The pinned checkout stays read-only; installed `tsgo` and `@types` are used.
- Generated configs inherit the pinned root/base compiler options and root
  package-to-source path mappings. Package exports target absent `dist/*.d.ts`;
  mapping to the real `src` tree checks actual types, not invented declarations.
- These are strict **no-emit** checks. Absolute `.ts` fixture imports use
  `allowImportingTsExtensions`; `rewriteRelativeImportExtensions` is disabled
  because fixtures are never emitted. The source's existing `skipLibCheck`
  setting is inherited, not expanded to suppress source or fixture errors.
- `typecheck-report.mjs` now runs the compiler, rather than filtering its output.
  Numeric compiler exits survive unchanged. Signal termination and launch errors
  fail with context. Both output streams, continuation lines, config diagnostics,
  source diagnostics, and diagnostics outside this repository remain visible.
- Existing negative type proofs require expected errors (`@ts-expect-error`
  directives fail when unused, or compiler-API proofs assert diagnostic codes,
  counts and spans). No source error is classified as an expected negative.

## Pinned-source blocker

Observed 2026-10-20, pinned revision `532f7d12cd19235081bd4d6e425bde185652d86f`:

| Check | Result | Boundary |
| --- | --- | --- |
| Registered oracle entrypoints | Before source-config repair: reached public-index stage | Includes repaired `config-value-reference-oracle.ts` Output union |
| Public-index type proof | FAIL | Imports pinned AI provider source through real package indexes |
| Later registered proofs | Not reached by gate | No pass claim |

Public-index checking exposes these source errors:

- `packages/ai/src/providers/cloudflare-ai-gateway.ts:19:4`, TS2353:
  `"openai-completions"` is absent from the inferred provider API record.
- `packages/ai/src/providers/opencode-go.ts:13:3`, TS2322: model array includes
  `"google-generative-ai"`, outside its declared provider API union.

The supported package build (`packages/ai/tsconfig.build.json`) includes both
provider modules. Consuming absent declarations cannot establish a clean build;
using declarations emitted despite source errors would not repair that failure.
No disposable build, declaration stub, source patch, diagnostic filter, or
skip-on-new-errors path is used. Gate remains failing until the pinned-source
inconsistency receives an explicitly approved resolution.

## Development proof ledger

| Command | Code scope | Result |
| --- | --- | --- |
| `node --test oracle/typecheck-report.test.mjs` | Original filtering helper at base `432cf07` plus regression tests | RED: 7 failed, 1 passed; failures returned false success |
| `./run-tests.sh oracle` | Uncommitted runner repair, before no-emit config repair | Regression tests 8/8 passed; gate exited 2 at public-index proof, showing both source errors and TS2877 absolute-import emit errors |
| `node --test --test-name-pattern=no-emit oracle/typecheck-report.test.mjs` | Added no-emit absolute-import regression, before config repair | RED: 1 failed with TS2877 |

Later source-config edits invalidate the earlier gate result as current-code
proof. Targeted GREEN evidence is recorded after committing the repair.
