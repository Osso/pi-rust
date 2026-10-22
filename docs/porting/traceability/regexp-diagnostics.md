# RegExp diagnostics and approved limits

Authority: exact oracle Node `v26.11.1`, V8 `14.6.202.34-node.37`; no network/source download. Printable diagnostic strings extracted from the executable around `Nothing to repeat` / `Unterminated group` / `Invalid regular expression`. Binary hash and extracted neighbors: `/tmp/pi-rust-repin-lossless-interactive/v8-regexp-binary-reasons.json`.

`crates/pi-js-compat/tests/regexp_diagnostics.rs` sends 66 fixtures to actual Node RegExp through `oracle/regexp-diagnostics-oracle.mjs`, then compares acceptance and complete UTF-16 error messages with native JsRegex. Historical **RED, not ignored** checkpoint295988b4: 65 mismatches before any diagnostic mapping. The test prints its complete lossless corpus for artifact capture. No production behavior changed, no assertions weakened, no mapping guessed.

## Coverage / stop condition

- Triggered 30 of regress parser's 32 static error categories and dynamic invalid-named-reference errors. Untriggered: `Incomplete class set character`, `Invalid property escape`. Dynamic `Unexpected char: ...` has no reachable fixture yet.
- A single regress category does not identify a V8 reason: parenthesis (unterminated/unmatched), group modifier (invalid/repeated flag/multiple dashes), quantifier (incomplete/out-of-order), character escape (invalid/Unicode/property/negated-string), and class-set bracket/range/character categories require additional parse context. No partial lookup is represented as full V8 parity.
- Node accepts 257 nested groups (514 characters, flag i), while regress rejects with `Regular expression is too deeply nested` (limit256).
- Node accepts 65,536 `a*` loops (131,072 characters, flag i), while regress rejects with `Loop count limit exceeded` (limit65,535).
- Capture overflow fixture (65,536 empty capture groups) is rejected by both, but regress diagnostic differs from Node `Too many captures`.

Full corpus and exact pinned messages: `/tmp/pi-rust-repin-lossless-interactive/regexp-diagnostic-corpus.json`; compact summary `regexp-diagnostic-summary.json`; command output `test-regexp-diagnostics-red.log`.

## Approved contract and GREEN proof (2026-10-22)

AGENTS.md records user approval: invalid-pattern reason text may differ from V8, classification must match; regress depth/loop limits are an explicit gap. The corpus test now asserts valid/invalid classification plus the frozen diagnostic prefix, omitting only approved reason wording. The two named resource-limit fixtures explicitly assert their Node-accepted/native-rejected gap, rather than being ignored or silently filtered.

`JsRegexError` owns a raw JsString message. Shared `js_regex/error.rs` is the only constructor of `Invalid regular expression: /<src>/<flags>: <reason>`; invalid flags receive the frozen constructor-flags prefix. Duplicate/unknown/u+v flags are rejected; both u and v select Unicode input mode. The former bwrap scanner and established exact reason mappings moved here; context-sensitive cases outside the proved subset retain the approved engine wording. Node-ignore and bwrap no longer maintain private diagnostic builders; session search reads the raw shared message without Display projection.

`test-regexp-classification-prefix-red.log` RED68 prefix/classification failures; `test-regexp-native-error-red.log` missing raw message API. `test-regexp-classification-green-1.log` GREEN2/2:72 scalar cases (including invalid flags and two explicit limit gaps), plus raw high/low/pair pattern-prefix cases. Original 66-case reason corpus remains preserved above as historical evidence, not a current failing gate.

Existing exact bwrap assertions are unchanged: `test-bwrap-worker-shared-regex-green.log`8/8 includes 13 invalid patterns ×2 flag modes with complete error-unit equality, proving retained mapped reasons and raw prefix fidelity. `test-differential-bwrap-shared-regex-green.log`4/4; `test-node-ignore-shared-regex-green.log`2/2. All logs live in the artifact directory. No bwrap_extension integration target exists in this checkout; actual worker and differential_bwrap suites were run.
