# Coding-agent auth command traceability

Pinned source: `packages/coding-agent/src/cli/auth-command.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

The current slice maps only command-kind metadata and pure help detection. Focused Rust tests cover every kind, exact output string, and help branch over Rust-representable strings. Direct TypeScript differential proof remains the current-cycle gap.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `auth-command.ts:4` | Define exact `check`, `api_key`, and `bearer_token` command kinds | `AuthCommandKind` plus focused all-kind tests |
| `auth-command.ts:18-22` | Associate each kind with one exact CLI usage string | `get_auth_command_usage` plus exact focused assertions |
| `auth-command.ts:24-26` | Map every kind to its exact public command name | `get_auth_command_name` plus exact focused assertions |
| `auth-command.ts:28-30` | Return usage text by kind | Exhaustive typed match plus exact focused assertions |
| `auth-command.ts:32-38` | Require exact `auth`, then accept missing/`help` second argument or exact `--help`/`-h` anywhere | Focused empty, prefix, position, unknown-subcommand, duplicate, case, Unicode, and NUL fixtures |
| `main.ts:60-67,230-251` | Use metadata and help detection before auth parsing and dispatch | Consumer mapping only; Rust main integration remains out of scope |

Rust uses an enum rather than TypeScript string-literal union values. No parser, serializer, generic string conversion, or invalid runtime object boundary is added in this slice.
