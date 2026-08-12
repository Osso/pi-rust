# Coding-agent auth command traceability

Pinned source: `packages/coding-agent/src/cli/auth-command.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

The current slice maps only command-kind metadata and pure help detection. Focused Rust tests cover every kind, exact output string, and help branch over Rust-representable strings. Fourteen direct requests (`12000..=12013`) compare all metadata outputs and every help condition against the real pinned module through one persistent oracle process.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `auth-command.ts:4` | Define exact `check`, `api_key`, and `bearer_token` command kinds | `AuthCommandKind`, focused all-kind tests, and differential requests `12000..=12002` |
| `auth-command.ts:18-22` | Associate each kind with one exact CLI usage string | Exact focused assertions and differential requests `12000..=12002` |
| `auth-command.ts:23-25` | Map every kind to its exact public command name | Exact focused assertions and differential requests `12000..=12002` |
| `auth-command.ts:27-29` | Return usage text by kind | Exhaustive typed match and differential requests `12000..=12002` |
| `auth-command.ts:31-35` | Require exact `auth`, then accept missing/`help` second argument or exact `--help`/`-h` anywhere | Focused matrix and differential requests `12003..=12013` covering empty/non-auth, missing/explicit help, flag positions, unknown commands, duplicates, case, Unicode, and NUL |
| `main.ts:60-67,230-251` | Use metadata and help detection before auth parsing and dispatch | Consumer mapping only; Rust main integration remains out of scope |

Rust uses an enum rather than TypeScript string-literal union values. No parser, serializer, generic string conversion, or invalid runtime object boundary is added in this slice.
