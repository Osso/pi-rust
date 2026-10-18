# `pi-js-compat`

Shared JavaScript-runtime semantics for the Rust port. The crate is a
refactoring source of truth: it replaces the duplicated helpers that grew inside
`pi-ai`, `pi-agent-core`, `pi-tui`, and `pi-coding-agent`. There is no
TypeScript module behind this crate.

## `String.prototype.localeCompare` (`src/collator.rs`)

| Source behaviour | Rust item | Proof |
|---|---|---|
| `a.localeCompare(b)` in the host locale | `locale_compare`; host locale from `LC_ALL`/`LC_COLLATE`/`LANG` (first non-empty), language part only, `_` → `-`, fallback `und`; ICU4X `Collator` at `Strength::Tertiary` | `tests/differential_locale_compare.rs::matches_node_default_locale_compare` (Node `a.localeCompare(b)` under `LANG=en_US.UTF-8`), `host_locale_compare_uses_the_resolved_environment_locale` |
| `new Intl.Collator(locale).compare(a, b)` | `compare_in_locale(a, b, locale)` | `matches_node_intl_collator_for_explicit_locales` over `en-US`, `und`, `sv`, `de`, `tr`, `ja`, `zh` |
| Invalid locale identifier | Parses to `und`; ICU4X construction failure falls back to the root collator | Construction path shared by both tests |
| Host locale resolution | `host_locale` | `host_locale_compare_uses_the_resolved_environment_locale` |

Differential corpus: 62 pairs covering empty strings, case, accents (precomposed
and decomposed), `ß`/`ss`, Nordic and Turkish letters, digits and numeric-looking
names, punctuation (`-`, `_`, `.`, ` `, `/`, `+`, `@`), CJK, Cyrillic, Greek,
Arabic, Hebrew, and emoji. Node returns an arbitrary-magnitude number, so the
oracle normalizes both sides to the sign of the comparison.

Deviations: none observed for the corpus. ICU4X and Node's ICU4C use the same
CLDR collation data for the tested locales.

## Callers (migrated in the consolidation commit)

| Old path | New path |
|---|---|
| `crates/pi-coding-agent/src/core/detached_job_retention.rs` (`compare_paths`) | `pi_js_compat::locale_compare` |
| `crates/pi-agent-core/src/harness/{skills,prompt_templates}.rs` (ASCII approximation) | `pi_js_compat::locale_compare` |
| `crates/pi-tui/src/autocomplete.rs` (ASCII approximation) | `pi_js_compat::locale_compare` |

## `minimatch` 10.2.5 (`src/minimatch/`)

Source: `node_modules/minimatch` in the pinned source worktree (the npm package
consumed by `coding-agent/src/core/model-resolver.ts` and
`coding-agent/src/core/package-manager.ts`), including its bundled
`brace-expansion` 5.x and `brace-expressions.js`.

| Source behaviour | Rust item | Proof |
|---|---|---|
| `minimatch(path, pattern, options)` | `minimatch::minimatch` | `tests/differential_minimatch.rs` (42k cases) |
| `braceExpand(pattern, options)` | `minimatch::brace_expand` | `matches_minimatch_brace_expansion` (21 patterns) |
| `Minimatch` construction: comments, empty patterns, negation, brace set, slash split, preprocess | `Minimatch::{new,make,parse_negate,preprocess}` | differential variants `default`, `nobrace`, `nonegate`, `nocomment`, `optimizationLevel0`, `preserveMultipleSlashes` |
| `preprocess` optimization levels 0 and 1 | `adjacent_globstar_optimize`, `level_one_optimize` | differential variants `optimizationLevel0` and default |
| `parse`: `**`, empty part, AST regex, fast tests | `Minimatch::parse`, `FastTest` | differential corpus |
| AST parser (`ast.ts`): extglob recursion/adoption/usurpation, negations, POSIX classes, glob escaping | `src/minimatch/ast.rs`, `class.rs`, `escape.rs` | differential corpus incl. `@(a|b)`, `*(a|b)`, `+(a|b)`, `?(a|b)`, `!(a|b)`, `a/**/!(b|c).js`, `[[:alpha:]]*`, `a[]]b`, `a[!]b`, escapes |
| `matchOne` and globstar head/body/tail recursion, dot rules, trailing slash | `match_one_inner`, `match_globstar`, `match_globstar_body_sections` | differential corpus incl. `**`, `a/**`, `a/**/b`, `**/**/a`, `a/**/../b`, `a/**//b` |
| `slashSplit`, `preserveMultipleSlashes` | `slash_split` | differential variants `preserveMultipleSlashes`, `default` |
| Brace expansion: sequences, padding, alphabetic, nested, comma parts, escaped braces, `max`/`maxLength` caps | `src/minimatch/brace.rs` | `matches_minimatch_brace_expansion` |
| Fast paths: `*`, `*.ext`, `?`, `?*.ext`, `*.*`, `.*` | `FastTest` | differential corpus (`*.`, `.*`, `*.*`, `a/*`, dot/nocase variants) |
| `nocase`, `dot`, `matchBase`, `nobrace`, `noglobstar`, `noext`, `nonegate`, `nocomment`, `partial`, `flipNegate`, `nocaseMagicOnly`, `braceExpandMax`, `maxGlobstarRecursion`, `maxExtglobRecursion` | `MinimatchOptions` | differential variants |

Corpus: 80 patterns × 52 paths × 4 default variants, plus 45 patterns × 52 paths
× 11 option variants, plus 21 brace-expansion patterns. Paths include empty,
`.`, `..`, dotfiles, trailing slashes, duplicate slashes, absolute paths,
Unicode, glob metacharacters, and brace characters.

Gaps (typed Rust API exclusions, documented):
- Windows platform branches (`process.platform === "win32"`, drive/UNC
  normalization, `windowsPathsNoEscape`, backslash-as-separator) are not
  ported; the target platform is posix.
- `optimizationLevel >= 2` preprocessing (`firstPhasePreProcess`,
  `secondPhasePreProcess`, `levelTwoFileOptimize`) is not ported; levels 0 and 1
  are. The consumers use the default level 1.
- `escape`, `unescape`, `makeRe`, `match`, `filter`, `defaults`, and the public
  `AST` class are not part of the typed API; only matching and brace expansion
  are exposed.
- A generated regular expression that fails to compile yields
  `Part::Invalid` (no match) instead of throwing; unreachable for the tested
  corpus.
- Patterns longer than 65536 UTF-16 code units return `false` instead of
  throwing `TypeError`.

Discrepancies (source bugs preserved or classified):
- `parseGlob` does not escape an explicitly escaped `|` (`a\|*`), so the
  generated regular expression contains a raw alternation and the pattern
  matches more paths than the literal reading suggests. The port reproduces the
  source behaviour (differential case `a\|*`).
- `brace-expansion` uses `Math.random()` sentinels; the port uses fixed
  non-printing sentinels. Observable output is identical for patterns that do
  not contain those sentinel code points.
