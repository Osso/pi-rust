# coding-agent json traceability

Pinned source: `packages/coding-agent/src/utils/json.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential proof: request `26003` executes the real pinned module across 20 comment, string, trailing-comma, and malformed cases. Focused tests: `crates/pi-coding-agent/tests/json_utils.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `json.ts:2-5` | Remove `//` comments outside complete string literals | `utils::json::remove_line_comments`; focused comment cases, differential `26003` |
| `json.ts:2-5` | Preserve string literals including escaped quotes and backslashes | `string_literal_end` matches `\\.` and `[^"\\]`; focused escaped-string cases |
| `json.ts:2-5` | Remove commas before `}`/`]` with intervening whitespace | `remove_trailing_commas` and `is_trailing_comma`; focused trailing-comma cases |
| `json.ts:4` | Unterminated quote does not swallow the remainder; later commas still match | Scanner only copies complete literals; differential malformed case 14 |

## Discrepancies

- None observed. JavaScript `\s` and Rust `char::is_whitespace` are aligned, with U+FEFF added explicitly to the trailing-comma whitespace set.

## Proof inventory

- Focused cases: 14.
- Differential request: `26003`, 20 cases.
