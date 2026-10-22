# model-resolver

Source: `packages/coding-agent/src/core/model-resolver.ts` (780 lines).
Rust: `crates/pi-coding-agent/src/core/model_resolver.rs`.
Tests: `crates/pi-coding-agent/tests/model_resolver.rs` (ports
`test/model-resolver.test.ts`), `tests/differential_model_resolver.rs`
(oracle `oracle/model-resolver-oracle.ts`).

## Exports and branches

| TS export / branch | Rust item | Test |
|---|---|---|
| `defaultModelPerProvider` (41 entries) | `DEFAULT_MODEL_PER_PROVIDER`, `default_model_for_provider` | `defaults_match_the_source_catalog`, differential `defaults` |
| `isAlias` (`-latest` or no `-YYYYMMDD`) | `is_alias`, `has_date_suffix` | `alias_preference_*`, differential parse cases |
| `findExactModelReferenceMatch` canonical/provider-split/bare-id, ambiguity | `find_exact_model_reference_match` | `exact_reference_rejects_ambiguity`, differential `exact` |
| `tryMatchModel` exact → partial id/name → alias preference → latest dated | `try_match_model` | `parse_simple_patterns`, `alias_preference_*` |
| `parseModelPattern` exact match, colon split, valid level recursion, invalid level warning/fallback, strict mode | `parse_model_pattern` | `parse_*` (23 differential cases), `strict_mode_rejects_invalid_suffix` |
| `resolveModelScopeFromModels` glob detection, thinking suffix, exact-before-glob, minimatch on `provider/id` and `id`, diagnostics order, dedupe | `resolve_model_scope_from_models` | `scope_*`, differential `scope` |
| `resolveModelScopeWithDiagnostics` | `resolve_model_scope_with_diagnostics` (trait seam) | compile + trait method; async path is a thin `available_models` call |
| `resolveModelScope` console warnings | `resolve_model_scope` (`eprintln!` per diagnostic) | diagnostics covered by sync tests; stderr side effect not asserted |
| `resolveCliModel` all branches: provider map, inferred provider, exact/ambiguous/auth preference, provider-prefix strip, strict parse, raw-id fallback, custom-model fallback with thinking suffix, error strings | `resolve_cli_model` | 17 differential CLI cases, all `cli_*` tests |
| `findInitialModel` priority order (CLI, scoped, saved default, defaults, first available) | `find_initial_model` | 5 differential initial cases, `initial_model_*` |
| `restoreModelFromSession` restored/fallback/current/default/first | `restore_model_from_session` | `restore_*` |

## Deviations / gaps

- `findInitialModel` calls `process.exit(1)` on a CLI resolution error in the
  source; the Rust function returns `Err(message)` so the CLI owns termination.
- Informational restore messages (`console.log`) go to stderr; the crate forbids
  stdout writes outside the output guard.
- `ModelResolverRuntime` is a trait seam for the duck-typed TS runtime; the
  `ModelRuntime` implementation lands with `model_runtime.rs`.
- `localeCompare` uses `pi_js_compat::locale_compare`; minimatch uses
  `pi_js_compat::minimatch` with `nocase: true`, as approved by the shared-helper
  decision.

## Discrepancies

None found.

## Q18 native pattern continuation (2026-10-22)

Scope/CLI patterns, warnings/errors/fallback messages, colon slicing, fuzzy text
matching, and glob matching use native UTF-16. Model/provider IDs remain approved
scalar metadata. Custom fallback model IDs reject unpaired units explicitly before
construction. Thinking-level recognition checks only its ASCII enum grammar;
unknown suffixes retain the original native pattern in diagnostics. Native raw
no-match fixture written but unexecuted. replacement-adapters-1 found one typo in
a macro name, corrected afterward; no runtime PASS.
