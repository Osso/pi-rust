# Shared raw ECMAScript regex adapter

Rust authority: `crates/pi-js-compat/src/js_regex.rs`. Validation's `pi-ai/utils/validation_regex.rs` is a re-export, not a second implementation. Dependency origin, exact patch and retirement: [dependencies](../../dependencies.md).

| Export/branch | Proof |
|---|---|
| `JsRegex::new` | legacy UCS-2 construction/matching; frozen worker regex matrix |
| `JsRegex::unicode` | explicit Unicode mode merges valid pairs but retains unmatched units; pair-versus-unit assertions |
| `JsRegex::with_flags` | raw pattern units become u32 code units in legacy mode, decoded code points in Unicode mode; `i`/`iu` canonicalization corpus |
| `JsRegex::is_match` | direct regress native `find_from_ucs2` or `find_from_utf16`; no scalar encoding of pattern/haystack |
| Constructor failures | regress error contract retained for validation; worker independently classifies/formats tested V8 failures, listed in [worker trace](worker-matching.md) |

Concrete Node comparisons: 616 frozen-worker regex/literal cases; 512 `i`/`iu` literal/class/range/word-class/backreference cases with long-s, Kelvin, dotless-i, raw unmatched units and pairs. The guard-free Unicode folding path is unchanged. These tests do not prove all ECMAScript syntax/diagnostic branches. Parent validation integration remains a separate compiler-blocked proof obligation.
