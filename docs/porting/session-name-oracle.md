# Session-name oracle

Session-name tests now use the [common frozen source](source-lock.json), not a separately prepared c248b0067 checkout. That ancestor's old approval is to re-validate against 4f9e6f8a4; current source identity is not behavior proof.

`run-tests.sh all` and `differential` run `oracle/session-name-reference.mjs` before Cargo. The read-only preflight checks the common locked HEAD, tsx, tsconfig and tracked `packages/ai/src/models.generated.ts`. Missing/mismatched inputs fail immediately; no clone, install, catalog generation or alternate source is attempted. Prepare the frozen source/dependencies explicitly; never recreate the old source. All Node commands run through `python3 scripts/test-sandbox.py -- ...`.

`session-names:packages/...` remains a test module selector, but `TsOracle::spawn` strips the prefix and uses the same common source lock as every other selector. Process-group teardown and parent watchdog remain unchanged. `oracle/session-name-reference.test.mjs` proves missing/wrong-HEAD failures before Cargo and valid/missing-environment preflights.

`cross_implementation_control_db` creates its fixture through real TypeScript, checks schema version before Rust opens it, writes a name through Rust, then reads it through TypeScript. Existing v14 SQL fixtures remain separate. No test opens or copies live state/config. Differential assertions are unchanged; re-pin discovery is pending main-thread integration.
