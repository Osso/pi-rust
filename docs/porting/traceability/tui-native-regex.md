# Shared native regex matching for TUI consumers

Parent authorized extending `pi-js-compat/src/js_regex.rs` after worker199 completed.
No direct-regress adapter or alternate regex authority is introduced in TUI.

- `JsRegex::{find,find_from}` return `Option<JsMatch>`.
- `JsRegex::{captures,captures_from}` return `Option<JsCaptures>`.
- `captures_iter`, `find_iter` iterate nonoverlapping matches including empty matches.
- `JsMatch::{start,end,range}` count UTF-16 units; `text()` returns original native slice.
- `JsCaptures::{get,name,len}` preserve undefined captures and native group text.
- `replace_all` expands native JS replacement syntax (`$$`, `$&`, prefix/suffix,
  numbered/named captures); `replace_all_with` invokes a native-string callback.

One shared `Arc<JsString>` backs each capture iteration's input. No scalar text copies,
replacement encodings, or unchecked scalar regex fallback. Unicode starting positions
inside a pair round to its start as Node RegExp `/u` does; non-u indexes individual units.

`raw_regex_matches` invokes native Node RegExp through `raw-regexp-oracle.mjs` and
compares raw native JSON for 7 concrete patterns × 7 starting positions (49 cases),
including high/low/pair/mixed text, lookbehind, unmatched groups, named/numbered
substitution, empty matches, u/non-u iteration, and callback replacements. Missing API
RED; targeted mandatory-runner jobs4 GREEN. Existing `is_match` execution path retained.
