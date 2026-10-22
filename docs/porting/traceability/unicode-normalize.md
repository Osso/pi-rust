# Native Unicode normalization

Rust: `crates/pi-agent-core/src/unicode.rs`. Source authority: frozen Node/V8 `String.prototype.normalize`, exercised by `oracle/unicode-normalize-oracle.ts` under the locked source's tsx.

| Surface | Native contract | Proof |
|---|---|---|
| normalize_nfc/nfd/nfkc/nfkd | `(&JsString)->JsString` | `tests/differential_unicode_normalize.rs`: existing scalar corpus plus 12 raw-unit cases × four forms |
| Lone surrogate | Retain exact unit as inert starter | isolated high/low, repeated/reversed lone units |
| Valid supplementary pair | Normalize as one scalar inside its run | supplementary pair adjacent to lone high unit |
| Composition boundary | Never compose or reorder across a lone unit | base/lone/mark; Hangul L/lone/V; leading marks after lone unit |
| Compatibility transforms | Apply to scalar runs without changing lone units | ligature/Angstrom with lone separators |

ICU4X normalize_utf16 replaces lone units, unlike V8's ICU4C. The implementation partitions complete scalar runs, applies ICU4X only inside each run, and copies lone units verbatim. No scalar/native alternate API or UTF8 conversion.

Implementation proof ledger and mutant result: `/tmp/pi-rust-repin-lossless-error-consumers/REPORT.md`. Full harness caller migration follows separately; standalone Unicode commit does not claim full-core compilation.
