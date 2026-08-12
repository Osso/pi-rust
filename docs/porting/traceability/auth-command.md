# Coding-agent auth command traceability

Pinned source: `packages/coding-agent/src/cli/auth-command.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Current coverage maps command metadata, pure help detection, and parsing. Fifty direct requests compare the real pinned module with Rust through one persistent oracle process: fourteen metadata/help requests (`12000..=12013`) and thirty-six parser requests (`12014..=12049`).

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `auth-command.ts:4` | Define exact `check`, `api_key`, and `bearer_token` command kinds | `AuthCommandKind`, focused all-kind tests, and differential requests `12000..=12002` |
| `auth-command.ts:18-22` | Associate each kind with one exact CLI usage string | Exact focused assertions and differential requests `12000..=12002` |
| `auth-command.ts:23-25` | Map every kind to its exact public command name | Exact focused assertions and differential requests `12000..=12002` |
| `auth-command.ts:27-29` | Return usage text by kind | Exhaustive typed match and differential requests `12000..=12002` |
| `auth-command.ts:31-35` | Require exact `auth`, then accept missing/`help` second argument or exact `--help`/`-h` anywhere | Focused matrix and differential requests `12003..=12013` covering empty/non-auth, missing/explicit help, flag positions, unknown commands, duplicates, case, Unicode, and NUL |
| `auth-command.ts:6-13` | Return a command with kind, passthrough args, three booleans, and optional numeric expiry | Public accessors, focused field tests, and direct requests `12015..=12017`, `12020..=12022`, `12034..=12047` |
| `auth-command.ts:15` | Throw an exported `AuthCommandError` with inherited `Error` identity | Public error constructor/name/message, `Display`/`Error`, focused tests, and direct error requests `12018..=12019`, `12023..=12033`, `12048` |
| `auth-command.ts:47-48` | Ignore non-`auth` argument lists without inspecting later values | Focused proof and direct request `12014` |
| `auth-command.ts:50-62` | Select three exact subcommands or throw the dynamic unknown-command message | Focused proof and direct requests `12015..=12019` |
| `auth-command.ts:64-70,89-90` | Initialize defaults, scan left-to-right, and preserve every unrecognized argument | Focused proof and direct requests `12020..=12022` |
| `auth-command.ts:71-80` | Restrict `--min-expiry`, validate source `/iu` grammar, multiply the JavaScript number, and let the last valid option win | Focused matrix and direct requests `12022..=12029`, `12033..=12049`, including exact IEEE-754 bits, finite parsing, parse/multiplication infinity, and the uppercase/long-s source bug |
| `auth-command.ts:82-87` | Restrict and consume three check-only boolean flags | Focused proof and direct requests `12020`, `12030..=12033` |
| `auth-command.ts:92-94` | Omit absent expiry but preserve explicit zero and non-finite source output | Focused proof and direct requests `12015..=12017`, `12034..=12047` |
| `main.ts:60-67,230-251` | Use metadata, help detection, and parsing before later CLI validation/dispatch | Consumer mapping only; Rust main integration remains out of scope |

Rust uses typed enums/structs rather than TypeScript literal/object values. `min_expiry_ms` is a field-specific `f64`, not a generic JavaScript-number model. Direct responses compare its exact IEEE-754 bits at zero, safe-integer precision boundaries, finite `1e308`, parser-produced infinity, and multiplication-produced infinity. The source `/iu` regex accepts uppercase and long-s unit spellings, but its lowercase-only multiplier comparisons send all such spellings to hours; Rust intentionally preserves this source bug. JavaScript lone-surrogate arguments and invalid runtime input types remain outside the Rust string/slice boundary.
