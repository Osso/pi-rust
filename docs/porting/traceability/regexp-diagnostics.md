# RegExp diagnostics and limits — RED corpus

Authority: exact oracle Node `v26.11.1`, V8 `14.6.202.34-node.37`; no network/source download. Printable diagnostic strings extracted from the executable around `Nothing to repeat` / `Unterminated group` / `Invalid regular expression`. Binary hash and extracted neighbors: `/tmp/pi-rust-repin-lossless-interactive/v8-regexp-binary-reasons.json`.

`crates/pi-js-compat/tests/regexp_diagnostics.rs` sends 66 fixtures to actual Node RegExp through `oracle/regexp-diagnostics-oracle.mjs`, then compares acceptance and complete UTF-16 error messages with native JsRegex. **Intentionally RED, not ignored**: 65 mismatches before any diagnostic mapping. The test prints its complete lossless corpus for artifact capture. No production behavior changed, no assertions weakened, no mapping guessed.

## Coverage / stop condition

- Triggered 30 of regress parser's 32 static error categories and dynamic invalid-named-reference errors. Untriggered: `Incomplete class set character`, `Invalid property escape`. Dynamic `Unexpected char: ...` has no reachable fixture yet.
- A single regress category does not identify a V8 reason: parenthesis (unterminated/unmatched), group modifier (invalid/repeated flag/multiple dashes), quantifier (incomplete/out-of-order), character escape (invalid/Unicode/property/negated-string), and class-set bracket/range/character categories require additional parse context. No partial lookup is represented as full V8 parity.
- Node accepts 257 nested groups (514 characters, flag i), while regress rejects with `Regular expression is too deeply nested` (limit256).
- Node accepts 65,536 `a*` loops (131,072 characters, flag i), while regress rejects with `Loop count limit exceeded` (limit65,535).
- Capture overflow fixture (65,536 empty capture groups) is rejected by both, but regress diagnostic differs from Node `Too many captures`.

Full corpus and exact pinned messages: `/tmp/pi-rust-repin-lossless-interactive/regexp-diagnostic-corpus.json`; compact summary `regexp-diagnostic-summary.json`; command output `test-regexp-diagnostics-red.log`.

Parent paused diagnostic implementation for a user-owned wording/limit decision. Until resolved, session-selector-search remains open: filtered-ID differentials do not prove its exported parseSearchQuery.error text. A wording deviation cannot by itself establish parity for the two accepted-by-Node/rejected-by-regress limit cases.
