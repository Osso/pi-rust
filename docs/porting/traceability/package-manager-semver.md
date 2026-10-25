# Package-manager node-semver dependency port

## Authority

`semver@7.8.0`, installed in frozen `4f9e6f8a4/node_modules/semver` (`package.json:3`). Read-only upstream reference: `npm/node-semver` tag `v7.8.0`, commit `efa4be6096c1f9b77d9d27d6132f6220c43b4e31`. Parent verified `classes/`, `functions/`, `internal/`, and `ranges/` byte-identical to the frozen installation. ISC [notice](../vendor/node-semver-LICENSE).

The frozen package-manager imports `maxSatisfying`, `rcompare`, `satisfies`, `valid`, and `validRange` at `packages/coding-agent/src/core/package-manager.ts:30`; call sites are `:49–54`, `:1429`, and `:1477`. Native implementation: `crates/pi-coding-agent-packages/src/core/package_manager_semver.rs`, one private module. The reduced Rust-semver parser and matcher were deleted; there is no parallel implementation, Node subprocess dependency, or linked npm dependency. [Contract](../../specs/package-manager-semver.md).

## Source-to-native coverage

| Frozen dependency source | Native behavior | Proof |
|---|---|---|
| `classes/semver.js:10–92`; `functions/{parse,valid}.js` | Strict string grammar, pre-trim UTF-16 length limit256, safe-integer core fields, canonical version without build metadata, native TypeError text | Upstream22 valid/5 invalid string fixtures; actual-source range/pinning matrix |
| `internal/re.js:19–247` | Exact generated `safeSrc` for FULL, COMPARATOR, BUILD, HYPHENRANGE, COMPARATORTRIM, TILDETRIM, CARETTRIM, CARET, TILDE, XRANGE, STAR, GTE0; shared UTF-16 JsRegex engine | Upstream100 strict/default range-parse fixtures; actual-source55-case matrix |
| `classes/range.js:6–174,257–533`; `ranges/valid.js` | JS whitespace folding; build removal before expansion; caret/tilde/X/hyphen bounds; operator whitespace; comparator dedupe; empty/null/OR sets; original first-set order | Same corpus, including wildcard prerelease/build, wildcard hyphen endpoints and incremented numeric limits |
| `classes/comparator.js:10–79`; `functions/{cmp,compare,gt,gte,lt,lte,eq}.js`; `classes/range.js:535–559` | ANY and strict comparator tests; prerelease core-admission rule | Upstream99 include/78 exclude fixtures, also tested through maxSatisfying |
| `classes/semver.js:93–165`; `internal/identifiers.js:3–24` | Core and prerelease precedence; JS binary64 numeric-identifier comparison; build ignored | Upstream19 comparison fixtures; actual-source large-prerelease rounding and build-tie RED/GREEN |
| `ranges/max-satisfying.js:5–27`; `functions/rcompare.js`; package-manager `:1477` | First maximum wins; invalid candidates ignored by range matching; stable descending selection and exact constructor failure order | Upstream membership/comparison corpus; ten fake-npm actual-source scenarios, including invalid singleton, invalid pair and lone-surrogate singleton |

Only strict/default string APIs used by package-manager are ported. Loose/includePrerelease options, version increments, coercion, intersection/subset/simplification and other uncalled dependency APIs are excluded, not package-manager gaps. Invalid JavaScript runtime types remain under the approved typed-API exclusion. Range LRU memoization is omitted; returned values and errors do not depend on cache identity.

`highest_version` uses the shared pinned V8 array sorter. Its first constructor error is retained; subsequent sort comparisons perform no constructors, and the private workarray is discarded on error. No partial result escapes. This preserves the observable callback/throw boundary despite the sort helper's infallible callback signature.

## Fixture provenance

`crates/pi-coding-agent/tests/data/npm-semver-7.8.0.json` copies only used rows from upstream `test/fixtures/{range-parse,range-include,range-exclude,comparisons,valid-versions,invalid-versions}.js` at the commit above. Selection: strict/default options, string-valued range/version inputs. One non-string range-exclude row is excluded by the typed-input rule. ISC notice is retained in this file's provenance and the linked license. Total323 source fixture rows; membership rows each exercise satisfies and maxSatisfying.

Regex constants were generated locally from `require('/home/osso/Repos/node-semver/internal/re').safeSrc`; no grammar was manually approximated. Global versus first-only replacements follow the source flags. BUILD removal at `classes/range.js:258` is significant for partial-version metadata.

## Evidence

- Genuine RED: `q15b-package-versions-red.log` (equal-precedence builds); `q15b-package-prerelease-red.log` (numeric prerelease rounding); `q15b-package-ranges-matrix.log` and `q15b-package-semver-regression-1.log` (normalization and expanded grammar).
- Post-commit GREEN on `2642c1cd` (implementation `17bf3f7f`, parent `c79fcb17`): `/tmp/pi-rust-repin-delta-ai/q15b-npm-semver-upstream-final.log` (4 focused tests,323 fixture rows), `q15b-npm-semver-differentials-final.log` (17 package differentials + ten-scenario update test), `q15b-npm-semver-check-final.log` (coding-agent all targets). Inventory PASS686 rows/186679 lines.
- Follow-up all-target warning-mode clippy on `73ffaba7` completed successfully, including test targets. `/tmp/pi-rust-repin-delta-ai/q15b-package-clippy-shared-oracle-filtered.json` reports zero errors and zero changed-line findings across the semver/capture owned files;689 target artifacts reached. This supersedes the initial strict-mode historical-warning blocker. No suppressions added. Manual changed-file readability audit performed; rust-code-analysis-cli unavailable.
- Package-manager remains partial: process capture/decoding/trim/env/timeout and concurrent failure-lifetime audit is not established by semver proof.
