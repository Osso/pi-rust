# Worker matching / raw minimatch traceability

Common source: `4f9e6f8a4`, full pin in [source lock](../source-lock.json). Contract: [worker matching](../../specs/worker-matching.md).

| Capability | Rust | Behavioral proof |
|---|---|---|
| Legacy RegExp and explicit Unicode execution | `pi-js-compat::js_regex::JsRegex` | `worker_matching::raw_regex_and_literal_match_actual_frozen_worker`: 616 comparisons with actual immutable worker functions evaluated in VM; mode mutants distinguish pair vs unit consumption |
| Literal lowercase/includes | raw `JsString` operations | same frozen-function matrix: unmatched units, pairs, case folding |
| AST/extglob/class/brace matching on raw units | whole `pi-js-compat::minimatch` engine | `raw_minimatch_matches_pinned_ast_braces_extglobs_and_classes`: 5,544 comparisons against pinned minimatch, including high/low ranges and nocase |
| Scalar matcher regression | same engine | existing `differential_minimatch` three targeted tests passed before final formatting fixes; passed at bd36a1e8 |
| Observed scalar bug | AST legacy regex | RED: `😀` against `@(?)`: frozen minimatch false, old scalar matcher true; GREEN after native UCS-2 execution |
| Active frozen worker glob / Node options | `worker::matches_glob`, level-two raw path preprocessing | `bwrap_worker_matching`: 1,683 actual frozen-function comparisons; raw units, braces/extglobs, traversal, Node nonegate/nocomment/backslash-pattern behavior; real find retains Linux backslash filenames after a separate RED |
| Legacy matcher and operation propagation | `LineMatcher`, compile once before collection | native worker module standalone harness (`worker_matching_tests.rs`) runs actual grep; pair-unit regex, raw ranges/literals, empty-directory invalid-pattern RED/GREEN |
| Raw regex compile errors | `format_regex_error` | 26 exact frozen message comparisons (13 patterns × both ignoreCase flags): unmatched bracket/group/close, leading quantifier, reversed class range, trailing escape, reversed quantifier bounds, quantified anchor/lookbehind, invalid group/capture name, duplicate name, missing named reference |
| Oversized glob throws | `matches_glob` | actual frozen `TypeError: pattern is too long` and actual Rust find-operation error, not silent false |
| Frozen fallback branch | oracle with `path.matchesGlob` absent | `.a`/`*` proves source fallback true differs active branch false. Rust provides the active matching capability unconditionally; no legacy Node-feature-detection branch is introduced |

The VM oracle loads immutable `fs-worker.cjs`, removes only its final `main()` invocation, and calls original functions. Separate explicit source-fallback context is available. No pinned source is edited. The frozen worker uses `path.matchesGlob`; Node's installed implementation sets `nonegate`, `nocomment`, `windowsPathsNoEscape`, `optimizationLevel: 2`, `nocaseMagicOnly`, with Linux case sensitivity. Final worker parity must test these rather than assume default minimatch.

## Open proof/discrepancy limits (not approved deviations)

- Complete V8 regex parser/diagnostic parity is not proved. Engine limits (`Capture group count limit exceeded`, `Loop count limit exceeded`, `Regular expression is too deeply nested`) and malformed Unicode escapes in named capture identifiers/references remain unmapped diagnostic branches. Unknown diagnostics are returned as errors, never interpreted as no-match. The shared validation adapter intentionally retains its existing regress error contract; worker formatting is separate.
- Worker error result preserves message/name prefixes and raw units. Rust does not reproduce Node source-location stack frames.
- Level-two preprocessing implements semantic pattern/file transformations; redundant pattern-set elimination is not needed to determine Boolean matches and remains absent. No general optimization-level-two corpus is claimed beyond the frozen-worker matrix.
- Parent coding-agent library and pi-ai validation integration remain compiler-blocked by independent slices; standalone harness compiles the actual worker module, not a replica.

Proof logs: `/tmp/pi-rust-repin-lossless-worker-matching/`. Shared regex factoring retains the former pi-ai constructor/matcher behavior; pi-ai integration is not yet compiled because parent library is blocked by other slices.
