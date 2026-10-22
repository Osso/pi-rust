# Raw minimatch module

Rust authority: `crates/pi-js-compat/src/minimatch/` (AST, braces, classes, escape helpers, POSIX optimization). Dependency pin: minimatch 10.2.5 under the immutable [common source](../source-lock.json). Worker contract and proof: [worker matching](worker-matching.md).

| Export/branch | Test or limit |
|---|---|
| `minimatch(path, pattern, options)`; `Minimatch::new`/`matches` | `differential_minimatch`: default/dot/nocase/matchBase and remaining options; 5,544 raw-unit comparisons in `worker_matching` |
| `brace_expand` returns `Vec<JsString>` | existing pinned expansion corpus; raw alternatives exercised through the whole matcher |
| AST literals, escapes, braces, extglobs, classes/ranges/POSIX classes, dot/traversal and globstar | full raw `JsString`/`u16` parser/storage; source differentials include distinct high/low units, pairs, replacement/private-use code points, and nocase |
| Fast tests / suffix lengths | raw unit length and raw suffix operations; astral `@(?)` RED distinguishes the old scalar engine |
| Optimization levels 0/1 | existing options corpus |
| Node-required level 2 pattern/file transformations | actual frozen-worker matrix; redundant multi-pattern elimination is omitted (no Boolean behavior change claimed from that omission) |
| Invalid/overlong pattern | existing public Boolean API retains `false`; worker adds frozen thrown error before calling it |

The decimal-number parser converts only grammar-validated ASCII number tokens to f64, retaining the existing correctly-rounded numeric behavior. Arbitrary pattern/haystack text is never encoded into scalar substitutes or rejected for unmatched units. Native regex source and matching use the [shared adapter](js-regex.md).
