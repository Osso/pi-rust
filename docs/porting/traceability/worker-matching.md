# Worker matching / raw minimatch traceability

Common source: `4f9e6f8a4`, full pin in [source lock](../source-lock.json). Contract: [worker matching](../../specs/worker-matching.md).

| Capability | Rust | Behavioral proof |
|---|---|---|
| Legacy RegExp and explicit Unicode execution | `pi-js-compat::js_regex::JsRegex` | `worker_matching::raw_regex_and_literal_match_actual_frozen_worker`: 280 comparisons with actual immutable worker functions evaluated in VM; mode mutants distinguish pair vs unit consumption |
| Literal lowercase/includes | raw `JsString` operations | same frozen-function matrix: unmatched units, pairs, case folding |
| AST/extglob/class/brace matching on raw units | whole `pi-js-compat::minimatch` engine | `raw_minimatch_matches_pinned_ast_braces_extglobs_and_classes`: 5,544 comparisons against pinned minimatch, including high/low ranges and nocase |
| Scalar matcher regression | same engine | existing `differential_minimatch` three targeted tests passed before final formatting fixes; current revision rerun pending |
| Observed scalar bug | AST legacy regex | RED: `😀` against `@(?)`: frozen minimatch false, old scalar matcher true; GREEN after native UCS-2 execution |
| Worker glob dispatch / compile errors / operation propagation | worker integration pending | not claimed |

The VM oracle loads immutable `fs-worker.cjs`, removes only its final `main()` invocation, and calls original functions. Separate explicit source-fallback context is available. No pinned source is edited. The frozen worker uses `path.matchesGlob`; Node's installed implementation sets `nonegate`, `nocomment`, `windowsPathsNoEscape`, `optimizationLevel: 2`, `nocaseMagicOnly`, with Linux case sensitivity. Final worker parity must test these rather than assume default minimatch.

Initial proof logs: `/tmp/pi-rust-repin-lossless-worker-matching/`. Shared regex factoring retains the former pi-ai constructor/matcher behavior; pi-ai integration is not yet compiled because parent library is blocked by other slices.
