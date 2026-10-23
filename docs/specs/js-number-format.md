# Shared binary64 number formatting

`crates/pi-js-compat/src/number.rs` supplies JavaScript number text and generic serde numeric conversions. JSON/coercion consumers use the shared formatter. [Implementation and proof map](../porting/traceability/js-compat.md#number--parseint--parsefloat--numbertostring-srcnumberrs).

## What it must do

- [x] Format binary64 with ECMAScript shortest decimals, including even-digit ties, decimal/scientific thresholds, negative-zero `0`, and String `NaN`/`Infinity`/`-Infinity`.
- [x] Shared JSON compact/pretty writers and scalar coercion agree literally with same-bit Node String/JSON.stringify; JSON non-finite values emit `null`.
- [x] Generic serde attributed f64/Option<f64> fields accept finite values near ±1e20 without integer range errors and preserve binary64 meaning, modulo JSON negative-zero normalization.
- [x] Keep exact representable i64/u64 numeric storage or original f64, never substitute the shortest decimal integer for the stored binary value; non-finite serde values become null.
- [x] `parse_int(&[u16], radix)` is the sole parseInt API: scan JS whitespace, optional sign, radix0/16 `0x` handling and longest digit prefix directly over native units. Surrogates terminate the prefix; never replace them. Callers share it rather than local parsers.
- [x] Retain other public helper APIs and parseNumber/parseInt/parseFloat/toFixed behavior; toFixed's existing non-finite/large-number formatter delegation uses the corrected primitive.

## Default locale number formatting

- [x] `number::to_locale_string(f64) -> JsString` is the single default en-US authority: comma grouping, zero to three fractional digits, decimal half-expand rounding, signed zero/rounded negative zero, NaN and ±∞. No arbitrary Intl options or host-locale selection are added.
- [x] Expand the shared shortest decimal before fractional rounding; do not substitute binary toFixed rounding or truncate fractions.
- [x] Interactive token display and custom-compaction notifications consume the shared helper; local grouping/rounding copies are deleted. Date locale formatting is a different API and stays unchanged.
- [x] Prove literal outputs from1043 identical binary64 values against pinned Node's built-in en-US formatter (edges, half-rounding neighbors, exponents, nonfinite and1000 deterministic bit samples). This bounded corpus is not exhaustive binary64 proof.

## How it works

- [Shared module trace and evidence](../porting/traceability/js-compat.md).
- [JSON writer trace](../porting/traceability/js-json.md).
- [Exact dependency purpose](../dependencies.md#pi-js-compat).

## Implementation inventory

- `crates/pi-js-compat/src/number.rs` — primitive formatting and numeric serde helpers; `number_locale.rs` holds the one default en-US formatter.
- `crates/pi-js-compat/src/js_json.rs` — existing shared writer and String-coercion consumers, unchanged.

## Tests asserting this spec

- `crates/pi-js-compat/tests/differential_number_bits.rs` — 4,346 same-bit samples; literal formatter/JSON/coercion output; numeric serde bits and no-error proof.
- `oracle/js-compat-oracle.ts` — Node DataView reconstruction from big-endian hex and built-in String/JSON.stringify; ASCII-LF framing.
- `crates/pi-js-compat/tests/{number,differential_number}.rs` — existing parsing/formatting/toFixed regression corpus.

- `crates/pi-js-compat/tests/differential_number_locale.rs` and `oracle/number-locale-oracle.mjs` —1043 same-bit default en-US outputs; interactive old integer-only helper RED on negative zero.

## Known gaps (current cycle)

- [ ] Exhaustive binary64 differential proof is not supplied; bounded edge corpus plus deterministic bit samples exercise the published ECMAScript primitive.
- [ ] Direct generic serde JSON spelling is not byte-exact JSON.stringify (exact integer digits and exponent thresholds may differ). Consumers needing byte parity must use the shared writer; no caller migration is claimed here.

## Out of scope

- Full HTTP/tool-body serialization migration, provider changes, or global serde arbitrary_precision.
- New numeric parsing/toFixed algorithms; untested parser and fixed-format cases remain unproven.
- Source runtime/pin changes: Node v26.11.1 records the observed local oracle version only.
