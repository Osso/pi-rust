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

## `Number` / `parseInt` / `parseFloat` / `Number::toString` (`src/number.rs`)

| Source behaviour | Rust item | Proof |
|---|---|---|
| `Number(string)` (whitespace, `0x`/`0b`/`0o`, `Infinity`, `NaN` rejection) | `parse_number` | `tests/number.rs`, `tests/differential_number.rs` |
| `Number.parseInt(string, radix)` (`radix` 0/16 prefix handling, longest digit prefix) | `parse_int` | `tests/number.rs`, `tests/differential_number.rs` |
| `Number.parseFloat(string)` (leading numeric prefix, exponent rollback) | `parse_float` | `tests/number.rs`, `tests/differential_number.rs` |
| ECMA-262 `Number::toString` (shortest round-trip digits, exponent thresholds) | `js_number_to_string` | `tests/number.rs`, `tests/differential_number.rs` (`String(Number(s))`) |
| `JSON.stringify`-style `f64` serialization | `serialize`, `serialize_option` | consumed by `pi-ai`/`pi-agent-core` wire records; existing differential suites |
| `JSON.stringify(number)` value (integer notation below `1e21`, `null` for non-finite) | `js_number_value`; the single implementation for every JSON payload builder | `tests/number.rs::js_number_value_matches_json_stringify_numbers`; `tests/differential_number.rs::matches_node_json_stringify_numbers` (27 inputs against `JSON.stringify(Number(s))`) |

Differential: 36 `Number` inputs, 28 `parseFloat` inputs, 21 `parseInt` pairs,
26 `String(Number(s))` inputs, and 27 `JSON.stringify(Number(s))` inputs against
Node 26.

Deviation: integral values in `(u64::MAX, 1e21)` are printed by `serde_json` in
exponent form (`1e+20`) instead of JavaScript's full digits
(`100000000000000000000`); the JSON numeric value is identical, and
`serde_json` cannot represent those integers without `arbitrary_precision`.
Exponent spelling outside that range matches JavaScript (`1e+21`).

## `Date.parse` (`src/date.rs`)

| `Date#getUTC*` fields | `utc_date_fields` (`local_date_fields_in` with `TimeZone::UTC`) | `src/date.rs::tests::utc_fields_match_javascript_get_utc_getters`; codex-usage differential |
| `Date.now()` | `js_now_millis` | `src/date.rs::tests::now_millis_tracks_the_system_clock`; tool elapsed timers and interactive idle timers |

| Source behaviour | Rust item | Proof |
|---|---|---|
| V8 single-number legacy dates: `0`, months `1..12`, invalid `13..31`, two-digit years `32..99`, longer years, sign/whitespace/`Z`; strict ISO years retain precedence | `parse_numeric_date` | `tests/differential_date_parse.rs::numeric_dates_match_node_in_utc_and_chicago` (RED `"0"`: Rust NaN vs Node 946684800000; GREEN pending) |
| HTTP-date forms (`RFC 1123`, `RFC 850`, asctime) | `httpdate` for zoned forms; asctime wall clock is resolved in local time like V8 | `tests/date.rs`, `tests/differential_date_parse.rs` |
| ECMAScript ISO 8601: 4-digit and signed 6-digit years, `YYYY[-M[-D]]`, `T`/`t`/whitespace separators, optional seconds/fraction, `Z`/`z`/`±HH:MM`/`±HHMM` | `parse_iso8601_millis` | `tests/date.rs`, `tests/differential_date_parse.rs` |
| Zone-less date-time = local; strict date-only = UTC; non-padded legacy date-only = local; `Z` on any date-only = UTC | `resolve_local_from_wall`, `finish_date` | `tests/date.rs`, differential under `TZ=UTC` and `TZ=America/Chicago` |
| DST gap/fold disambiguation | `jiff::civil::DateTime::to_zoned` (`compatible`: gap uses the pre-transition offset, fold uses the earlier instant) | `tests/date.rs::uses_compatible_disambiguation_for_dst_transitions` |
| Extended years beyond jiff's timestamp range | offset from the nearest representable year (`9998`/`-9998`), applied to the original wall clock | `tests/date.rs`, differential (`+010000`, `-000001`, `-271821`, `+275760`) |
| `MakeDay` normalization (`2020-02-31` → March 2, day 32 invalid) | `normalize_day` | `tests/date.rs`, differential |
| `24:00` only with zero minutes/seconds/millis | time validation in `parse_iso8601_millis` | `tests/date.rs`, differential |
| `days_from_civil` | `days_from_civil` | `tests/date.rs::days_from_civil_matches_known_dates` |

Differential: 100 inputs under `TZ=UTC`, `TZ=America/Chicago`, and the inherited
process zone.

V8 month-name/slash legacy dates, local/zoned times, two-digit year pivots, pre-1970 HTTP and asctime dates now use the shared legacy date grammar. Proof: `legacy_month_slash_and_pre_epoch_http_dates_match_node_across_zones` (RED `"Jan 1 2020"`: NaN vs Node 1577836800000; GREEN pending), UTC and Chicago including DST gap/fold and adjacent invalid dates/times.

The finite corpus does not establish complete V8 legacy grammar parity; untested legacy forms remain unproven, not approved exclusions.

## Base64 (`src/base64.rs`)

| Source behaviour | Rust item | Proof |
|---|---|---|
| `atob(string)` (ASCII whitespace ignored, padding optional, standard alphabet only, `InvalidCharacterError` cases) | `atob` | `tests/base64.rs`, `tests/differential_base64.rs` |
| `btoa(binaryString)` | `btoa` | `tests/base64.rs`, `tests/differential_base64.rs` |
| `Buffer.from(string, "base64")` (standard + URL-safe alphabets, invalid characters skipped, stop at `=`) | `buffer_from_base64` | `tests/base64.rs`, `tests/differential_base64.rs` |
| `Buffer#toString("base64")` | `buffer_to_base64` | `tests/base64.rs`, `tests/differential_base64.rs` |
| Decoded byte length used by terminal-image sizing | `buffer_decoded_len` | `tests/base64.rs` |

Differential: 34 payloads and 11 byte sequences against Node.

Discrepancy: `buffer_decoded_len` preserves the previous `pi-tui` behavior of
counting only standard-alphabet symbols, so it differs from Node's
`Buffer.byteLength(s, "base64")` for inputs containing whitespace or invalid
characters. `terminal_image` only feeds it protocol payloads. The previous
`pi-tui` decoder rejected URL-safe symbols; the shared `buffer_from_base64`
accepts them, matching Node.

## JavaScript string helpers (`src/string.rs`)

`is_js_whitespace_unit`, `is_js_whitespace_char`, `js_trim`, `js_trim_start`,
`js_trim_end`, and `utf16_len` replace the `pi-tui` `js_compat` module. Proof:
the migrated `pi-tui` differential suites (`differential_{fuzzy,latex,keys,stdin_buffer,terminal_colors,utils}`)
and `pi-js-compat` unit tests.

## Migrated callers

| Old path | New path |
|---|---|
| `crates/pi-ai/src/js_number.rs`, `crates/pi-ai/src/utils/js_number.rs` | `pi_js_compat::number`, `pi_js_compat::date` |
| `crates/pi-ai/src/utils/base64.rs` | `pi_js_compat::base64::{atob, btoa}` |
| `crates/pi-agent-core/src/harness/tools/image.rs` (`encode_base64`, `decode_base64`) | `pi_js_compat::base64::{buffer_to_base64, buffer_from_base64}` |
| `crates/pi-agent-core/src/harness/{skills,prompt_templates}.rs` (ASCII approximation) | `pi_js_compat::locale_compare` |
| `crates/pi-tui/src/{js_number,js_compat,base64}.rs` | `pi_js_compat::{number, string, base64}` |
| `crates/pi-tui/src/autocomplete.rs` (ASCII approximation) | `pi_js_compat::locale_compare` |
| `crates/pi-coding-agent/src/core/detached_job_retention.rs` (`compare_paths`) | `pi_js_compat::locale_compare` |
| `crates/pi-coding-agent/src/utils/iso_time.rs` (`parse_javascript_date`) | `pi_js_compat::date::date_parse` |
| `crates/pi-coding-agent/src/utils/footer_status.rs` (byte-order sort) | `pi_js_compat::locale_compare` |
| `crates/pi-ai/src/api/{pi_messages,mistral_conversations,openai_codex_responses,anthropic_messages,bedrock_converse_stream,openai_responses,openai_completions,azure_openai_responses}.rs`, `utils/validation.rs`, `auth/types.rs` (`js_number`/`number_value`/`json_number`) | `pi_js_compat::number::js_number_value` |
| `crates/pi-agent-core/src/harness/messages.rs`, `harness/frontmatter.rs` | `pi_js_compat::number::js_number_value` |
| `crates/pi-coding-agent/src/{utils/js_json.rs,utils/frontmatter.rs,core/settings_manager.rs,core/models_store.rs}` | `pi_js_compat::number::js_number_value` |
| `crates/pi-coding-agent/src/core/tools/{read,ls,find,grep,bash}.rs`, `bundled_extensions/pyrun/eval_tool.rs`, `core/settings_manager.rs` (`format_js_number`) | `pi_js_compat::number::js_number_to_string` |
| `crates/pi-coding-agent/src/core/export_html/{index,ansi_to_html}.rs` (`js_number`, `js_number_to_string`) | `pi_js_compat::number::js_number_to_string` |

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

## `to_fixed` (added for coding-agent `list-models`)

`Number.prototype.toFixed(fractionDigits)` for finite values below `1e21` is
implemented from the exact binary value of the double: the mantissa is scaled
by `10^fractionDigits` and rounded to the nearest integer with an exact tie
rounding away from zero. `fractionDigits` is capped by the `u128` scale
(15 digits), which covers every source call site. Unit tests cover the cases
where `(value * 10).round()` or Rust `{:.1}` diverge (`1.15`, `1.25`, `999.95`,
`1.005`, `1.045`). The `list-models` differential exercises the formatting
end-to-end; `tests/differential_number.rs::matches_node_to_fixed` covers the
`toFixed` edge cases, including `(-0).toFixed(1) == "0.0"`.
