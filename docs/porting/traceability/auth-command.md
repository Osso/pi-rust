# Coding-agent auth command traceability

Pinned source: `packages/coding-agent/src/cli/auth-command.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Current coverage maps command metadata, pure help detection, and parsing. Focused Rust tests cover every parser branch and representable string/numeric boundary. Fourteen existing direct requests (`12000..=12013`) compare metadata and help behavior; parser differential proof remains the current-cycle gap.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `auth-command.ts:4` | Define exact `check`, `api_key`, and `bearer_token` command kinds | `AuthCommandKind`, focused all-kind tests, and differential requests `12000..=12002` |
| `auth-command.ts:18-22` | Associate each kind with one exact CLI usage string | Exact focused assertions and differential requests `12000..=12002` |
| `auth-command.ts:23-25` | Map every kind to its exact public command name | Exact focused assertions and differential requests `12000..=12002` |
| `auth-command.ts:27-29` | Return usage text by kind | Exhaustive typed match and differential requests `12000..=12002` |
| `auth-command.ts:31-35` | Require exact `auth`, then accept missing/`help` second argument or exact `--help`/`-h` anywhere | Focused matrix and differential requests `12003..=12013` covering empty/non-auth, missing/explicit help, flag positions, unknown commands, duplicates, case, Unicode, and NUL |
| `auth-command.ts:6-13` | Return a command with kind, passthrough args, three booleans, and optional numeric expiry | Public `AuthCommand` accessors plus focused exact-default, argument-order, flag, and optional-zero tests |
| `auth-command.ts:15` | Throw an exported `AuthCommandError` with inherited `Error` identity | Public error constructor/name/message, `Display`, `Error`, and focused exact-message proof |
| `auth-command.ts:47-48` | Ignore non-`auth` argument lists without inspecting later values | Focused empty, non-auth, and later-auth-looking fixtures |
| `auth-command.ts:50-62` | Select three exact subcommands or throw the dynamic unknown-command message | Focused all-kind and missing/empty/help/Unicode-NUL error fixtures |
| `auth-command.ts:64-70,89-90` | Initialize defaults, scan left-to-right, and preserve every unrecognized argument | Focused passthrough order, Unicode, NUL, unknown/help flags, positional values, and duplicates |
| `auth-command.ts:71-80` | Restrict `--min-expiry`, validate source `/iu` grammar, multiply the JavaScript number, and let the last valid option win | Focused kind/error ordering, invalid classes, all accepted unit spellings, repeated options, exact IEEE-754 bits, finite boundary, and infinity proof |
| `auth-command.ts:82-87` | Restrict and consume three check-only boolean flags | Focused supported, repeated, interleaved, and per-kind exact-error fixtures |
| `auth-command.ts:92-94` | Omit absent expiry but preserve explicit zero and non-finite source output | `Option<f64>` getters plus focused absence, zero-bit, precision, and positive-infinity fixtures |
| `main.ts:60-67,230-251` | Use metadata, help detection, and parsing before later CLI validation/dispatch | Consumer mapping only; Rust main integration remains out of scope |

Rust uses typed enums/structs rather than TypeScript literal/object values. `min_expiry_ms` is a field-specific `f64`, not a generic JavaScript-number model. Source and Rust probes established identical bits for the selected integer, precision-loss, finite-overflow-boundary, and infinity cases; persistent differential proof remains pending. The source `/iu` regex accepts uppercase and long-s unit spellings, but its lowercase-only multiplier comparisons send all such spellings to hours; Rust intentionally preserves this source bug. JavaScript lone-surrogate arguments and invalid runtime input types remain outside the Rust string/slice boundary.
