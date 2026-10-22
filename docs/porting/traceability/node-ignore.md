# node-ignore — Q18 UTF-16 defaults

Frozen source: `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d/node_modules/ignore/index.js`, npm `ignore@7.0.5`.

## Contract and coverage

| Source behavior | Native API / proof |
| --- | --- |
| default factory, string add, CRLF split, comments/blanks, leading escapes | `Ignore::new/default/add`; frozen differential rule corpus |
| all 15 REPLACERS, trailing wildcard mode `regex`, range sanitation | `make_regex_prefix`; generated atom combinations, deterministic raw-unit fuzz, escaped/invalid ranges |
| legacy non-u case-insensitive regex | shared `JsRegex` with `i`; case and surrogate rules/paths |
| ignored/unignored skipping, parent directories, trailing slash | `ignores`; negation sequences and expanded nested path corpus |
| ignore cache reset after add, independent clone | `additions_and_clone_do_not_reuse_stale_results` |
| strict path validation | empty TypeError and relative-path RangeError, exact UTF-16 messages |
| lazy regex compilation and SyntaxError | reversed non-ASCII/escaped/surrogate ranges; escaped nested bracket unterminated class; invalid rules skipped when prior rule/parent decides |
| package manager caller | replaces `ignore::gitignore`, passes root-relative paths with directory slash; coding-agent build still blocked by other Q18 lanes |

`IgnoreError.name` is `TypeError`, `RangeError`, or `SyntaxError`. The frozen replacement pipeline escapes non-class regex syntax; remaining compile failures are reversed ranges or unterminated classes. Native error text maps these to V8 text and embeds the exact raw regex source. No raw string encoding occurs.

Excluded from this bounded API: non-string JS runtime inputs, factory options, arrays/Ignore objects in add, `test`, `checkIgnore`, `filter`, `createFilter`, marks, Windows path conversion. These are not used by the assigned consumers; they are not claimed as full package API parity.

## Proof ledger

Logs under `/tmp/pi-rust-repin-lossless-js-libs/`: `red-node-ignore.log` (missing module API), `red-node-ignore-lazy-errors.log` (behavioral add-time panic vs frozen lazy error), `green-node-ignore-lazy-errors2.log` (2 tests passed). Earlier `green-node-ignore-expanded.log` covers non-error matcher behavior. Runtime invocations use configured Cargo sandbox runner.

Behavioral mutants: `mutant-case-sensitive.log`, `mutant-skip-parent.log`, `mutant-wildcard-codepoint.log`; each fails frozen differential assertions, restored afterwards. Exact command: `cargo test -j4 -p pi-js-compat --test differential_node_ignore`. Parent owns independent lint/type/integration gates; these focused development proofs do not establish Q18 acceptance.
