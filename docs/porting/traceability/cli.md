# cli traceability

Source: `packages/ai/src/cli.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-ai/src/bin/pi-ai.rs` (binary target `pi-ai`, mirroring the
package `bin` entry).
Tests: `crates/pi-ai/tests/cli.rs` (15 argv/stdin cases run against both CLIs
with byte-identical stdout/stderr and equal exit codes),
`crates/pi-ai/src/bin/pi-ai.rs` unit tests (5: credential JSON shape, auth-file
round trip/order/indent, invalid auth files, JS array indexing, line endings).

## Behavior → proof

| TypeScript | Rust | Proof |
|---|---|---|
| `PROVIDERS = builtinProviders().filter(auth.oauth)` | `oauth_providers()` | `list`/`help`/`login` differential cases |
| no args / `help` / `--help` / `-h` usage text | `print_usage` | differential `no args`, `help`, `--help`, `-h` |
| `list` rows `id.padEnd(20) + " " + name` | `{:<20} {}` | differential `list`, `list ignores extra args` |
| unknown command | `Err("Unknown command: <cmd>")` → `Error: ...`, exit 1 | differential `unknown command` |
| `login [provider]` with a non-OAuth/unknown provider | same message | differential `login unknown provider`, `login provider without oauth` |
| `login` without provider: numbered names, `Enter number (1-N): `, `Number.parseInt(x, 10) - 1` | `select_provider` + `js_number::parse_int` | differential `login selection out of range/not a number/empty` |
| out-of-range/NaN selection → `Unknown provider: ` | `array_index` → empty id | same cases (exit 1, empty id) |
| `login(providerId)` looks up the provider and calls `auth.oauth.login` | `login()` | differential `codex selection *` reaches the codex select prompt |
| prompt `select`: `\n<message>`, numbered labels, range prompt, invalid → `Invalid selection` | `answer_prompt` | differential `codex selection out of range/zero/blank` |
| prompt `text`/`secret`/`manual_code`: `message (placeholder): ` | same | exercised by OAuth flows; formatting unit-covered by differential prompts |
| notify `auth_url` / `device_code` / `info` / `progress` | `CliInteraction::notify` | reached before the select prompt in codex flows; deterministic text checked by the OAuth module tests |
| `loadAuth` (missing/invalid → `{}`) | `load_auth_from` | unit tests `missing_or_invalid_auth_files_load_empty` |
| `saveAuth` (`JSON.stringify(auth, null, 2)`, insertion order) | `save_auth_to` | unit test `auth_file_round_trips_in_insertion_order_with_two_space_indent` |
| credential stored as `OAuthCredential` | `credential_to_value` (refresh/access/expires + extras) | unit test `credential_json_matches_the_typescript_shape` |
| `console.error("Error:", message)` + `process.exit(1)` | `eprintln!("Error: {message}")` + `exit(1)` | every failing differential case |

## Deviations / exclusions

- EOF while a prompt is pending: Node's readline never resolves the pending
  question and the process exits 0 once stdin closes. Rust exits 0 immediately
  on `read_line == 0` after flushing the prompt. The observable bytes and exit
  code match (verified manually; not part of the differential cases because the
  Anthropic flow prints a random PKCE URL before EOF).
- `auth.json` entries are typed as JSON objects; a valid non-object document
  (`[]`, `"x"`) is treated as empty. TypeScript casts and would stringify the
  non-object array/string, an invalid auth store that no login path produces.
- Provider selection uses `Number.parseInt` (`pi_ai::js_number::parse_int`);
  fractional values (`1.5`) resolve to index 0 in both.
- The successful login write path (network) is not exercised; the credential
  JSON shape and auth-file I/O are unit-tested instead.

## Discrepancies

- None.
