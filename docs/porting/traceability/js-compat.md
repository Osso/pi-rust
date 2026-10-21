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
| Lossless host-locale UTF-16 comparison, including lone surrogates | `locale_compare_utf16(left: &[u16], right: &[u16])`; ICU4C backend, existing UTF-8 ICU4X API unchanged | `tests/image_generator_collation.rs::lone_surrogate_collation_matches_node_pair_order`; [independent bounded Batch A proof](../batch-a-proof.md) |

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
| ECMA-262 `Number::toString`: shortest digits, even-digit ties, exponent thresholds, signed zero, non-finite | `js_number_to_string` via `ryu_js::Buffer::format` | `tests/number.rs`, `tests/differential_number.rs`, `tests/differential_number_bits.rs::shared_formatter_and_json_consumers_match_node_same_bits` |
| Generic serde finite numeric meaning without i128 range errors; Some/None and non-finite null | `serialize`, `serialize_option`; exact integer conversion only in i64/u64 range | `generic_serde_preserves_finite_numeric_meaning_without_range_errors` exercises actual attributed fields through `serde_json::to_value` |
| Numeric value storage, not shortest-decimal integer substitution | `js_number_value` | Same test: as_f64 bits/range and exact unsafe-integer storage; historical `matches_node_json_stringify_numbers` is semantic-only |
| Literal compact/pretty JSON and String coercion | `js_json` shared writer / `to_js_string` | Same-bit test compares Node text literally for lossless writer, serde writer, parse/write and scalar coercion |

Differential: 36 `Number` inputs, 28 `parseFloat` inputs, 21 `parseInt` pairs,
26 `String(Number(s))` inputs, and 27 `JSON.stringify(Number(s))` inputs against
Node 26.

Bounded number-format spec: [contract](../../specs/js-number-format.md).
New corpus: 4,346 binary64 bit patterns, transmitted as big-endian hex strings and
read with Node DataView, including unsafe integers ±ULPs at 1e18, 2^53, 2^64,
1e20/1e21 and 1e-6/1e-7, exact shortest-digit tie, signed zero, non-finite,
subnormals, extrema and 4,096 unfiltered deterministic xorshift patterns.
ASCII-LF oracle framing and existing TsOracle process watchdog are reused.
Node v26.11.1 is an observed development runtime, not a new source baseline.

RED: the original helper already formatted 1e18+128 correctly; its actual defect
was bits `3e60000000000000`: Rust `...5313e-8`, Node `...5312e-8`.
Generic serde separately rejected bits `c3effffffffffff8` as number out of range.
Development commands/results: `/tmp/pi-rust-js-number-format-proof/ledger.md`.

Boundary: direct generic serde JSON is numeric-semantic, not byte-exact
JSON.stringify. It may print exact integral digits `1000000000000000128`
rather than shortest `1000000000000000100`, and exponent `1e+20` rather than
full decimal. Both retain the same binary64 meaning. The shared js_json writer
is byte-exact for the bounded corpus. Internal integer storage is never replaced
by the shortest decimal integer. No arbitrary_precision, wire-body/tool serde
migration, provider changes or new parse/toFixed algorithms are claimed.
The finite corpus does not establish exhaustive binary64 parity; the primitive
uses the published ECMAScript formatter rather than corpus-specific cases.

## `Date.parse` (`src/date.rs`, `src/date_parser.rs`)

Implementation reference: [Node v26.11.1 V8 file hashes](../v8-date-parser-source-lock.json),
[BSD license](../vendor/V8-LICENSE). This is not a new TS runtime baseline.

| Source behavior | Rust item | Behavioral proof |
|---|---|---|
| UTF-16 scanner: ASCII numbers (nine significant digits after leading zeros), word-prefix table, comments, NUL termination, Unicode whitespace/punctuation | `Scanner`, `keyword` | `v8_scanner_boundaries_and_bounded_combinations_match_node`, `v8_lossless_utf16_matches_node` |
| Strict ISO parsing with legacy continuation only before time separator; signed years, negative-zero consumption, date-only UTC | `parse_iso` | original 662 comparisons plus boundary corpus |
| Legacy named/numeric day ordering, 0..49/50..99 year pivot, time/AM-PM composition, named-month out-of-range-second quirk | `Day`, `Time`, `parse` | numeric/legacy original suites plus boundary combinations |
| Abbreviation zones; legacy unconstrained hour fields, uint32 overflow and Smi limit vs strict ISO constraints | `Zone` | boundary corpus including 32-bit Smi boundaries `GMT+300000:`, `GMT+596523:`, `GMT+596524:` and `GMT+1193047:` and `GMT-1193047:59` exact second offsets |
| Gregorian normalization, overflow-safe time construction, inclusive ±8.64e15 TimeClip | `days_from_civil`, `finish_date` | `tests/date.rs`, clip/large-year boundary cases |
| Local DST gaps/folds, extended future calendar-equivalent year | `resolve_local_from_wall` | Chicago ordinary/extended DST corpus; original local/ancient/year-9999 cases |
| Lossless UTF-16 native API; `str` remains encoding wrapper | `date_parse_utf16`, `date_parse_utf16_with_time_zone`, existing `date_parse*` | UTF-16 cases against raw Node `Date.parse`, UTC/Chicago/inherited |
| Unchanged clock/getter/ISO formatting helpers | existing Date APIs | existing date integration tests; no grammar changes to those helpers |

Four initial counterexamples reproduced actual RED (not wrong-type inputs):
`Jan 1 2020 EST`, `2020.1.1`, `Jan1 2020`, `Jan 1 2020 (ignored)`.
Original 662 differential comparisons retained. Expanded tests compare exact
finite milliseconds or null for NaN, with no input normalization/filter/sorting.
Oracle uses pinned worktree tsx, parent watchdog, ASCII-LF JSON transport (not
Node readline); UTF-16 arrays become JS strings without replacement.

Additional RED: extended Chicago dates `+010000-03-12T02:30`,
`+010000-11-05T01:30`, `+275000-03-09T02:30` differed by one hour with
the old nearest-year resolver. A calendar-equivalent 400-year mapping fixes
weekday/leap-year dependent future transitions and retains compatible gaps.

Scope: faithful scanner/composer adaptation with finite differential evidence,
not exhaustive V8 grammar proof, whole Date-object port, or all-platform zone
parity. Runtime usage telemetry omitted. Host tzdb/ICU version differences and
untested zones remain unproven. No fallback parsers remain.

Independent number/date/JSON/coercion/collation/filesystem tests, check/Clippy and unresolved source-faithful Date readability debt: [Batch A proof ledger](../batch-a-proof.md). This is not exhaustive grammar/platform or whole-port proof.

## Node timers

`src/node_timers.rs::normalize_timeout_delay(f64)` models Node's numeric timer
constructor delay after caller-side ToNumber: outside inclusive
`1..=2147483647` (including NaN/infinity) becomes 1ms; otherwise unchanged,
including fractions. No JavaScript invalid-type coercion API or warning emission
is introduced. `tests/node_timers.rs` covers both branches and endpoint/fraction
cases; `tests/differential_node_timers.rs` observes the pinned runtime's actual
scheduled `Timeout._idleTimeout`, clearing timers immediately. This deterministic
runtime-field observation is parent-approved; callback timing/sub-ms scheduling
precision is not claimed. SSH provides the public behavioral timeout proof:
[SSH traceability](core-ssh-errors.md#ssh-timer-normalization).

Evidence: `/tmp/pi-rust-core-c-fixes/ledger.md`.

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
