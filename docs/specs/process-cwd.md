# Process cwd and chdir

One shared native working-directory authority with Node-compatible error records. Node cwd-cache semantics are explicitly excluded by the [2026-10-23 user decision](../../AGENTS.md). [Reference source](../porting/node-process-cwd-source-lock.json) and [branches/consumer inventory](../porting/traceability/process-cwd.md).

## What it must do

### Native process state

- [x] Read actual process cwd on every call; do not emulate Node's cached previous directory.
- [x] A removed cwd produces the native Node ENOENT/uv_cwd name/message/code/errno/syscall fields, including after an earlier successful read.
- [x] Successful chdir changes actual process state; failures preserve actual cwd and retain Node error fields.
- [x] Decode OS paths through the shared Node byte decoder; retain the existing UTF-8 OS boundary for chdir.
- [x] Preserve chdir name/message/code/errno/syscall/path/dest for existing old cwd, including missing/file/empty destinations.

### Consumers

- [x] Use the authority in path resolution, SDK/settings/startup, executable lookup, restart, architect, bwrap worker, catalog generator and native examples; no cwd fallback to `/` or `.`.
- [x] Carry errors through native wrappers to real output/catch boundaries rather than converting them to text early.
- [x] Absolute Architect project paths do not read process cwd; relative paths do.
- [x] Isolate process-cwd mutation tests in child processes; fixture changes use the shared authority.

## How it works

- [Process cwd traceability](../porting/traceability/process-cwd.md)
- [Shared Node error metadata](node-libuv-errors.md)

## Implementation inventory

- `crates/pi-js-compat/src/process_cwd.rs` — direct native cwd/chdir entry points; no cache or synchronization layer.
- [Consumer inventory](../porting/traceability/process-cwd.md#workspace-consumer-inventory) — production and example paths.
- `crates/pi-coding-agent/examples/sdk/error.rs` — source retained through sample entrypoint formatting.

## Tests asserting this spec

- `crates/pi-js-compat/tests/process_cwd.rs`, `oracle/process-cwd-primitive-oracle.mjs` — real Node26.11.1 sequence, exact native errors and explicitly asserted approved runtime differences.
- `crates/pi-coding-agent/tests/process_cwd.rs`, `oracle/process-cwd-oracle.mts` — actual frozen SessionManager warm/cold characterization, native direct-read rejection, eight consumer carriers and frozen Architect absolute-path behavior.

## Known gaps

- [x] **Approved 2026-10-23** ([project rule](../../AGENTS.md)): after removing a warmed directory, Node may return its cached previous path; native reads the OS and reports ENOENT. Native therefore does not retain that cached previous path for a recovery notice. Runtime recovery hooks are a separate slice, not accepted here.
- [x] **Approved 2026-10-23** ([project rule](../../AGENTS.md)): failed chdir with a deleted old cwd uses an empty old path instead of Node's unstable buffer. Its message uses that same empty path; other error fields remain asserted.

## Out of scope

New runtime-oddity emulation and [working-directory recovery hooks](agent-session-runtime.md). Invalid JavaScript runtime input types and Windows gaps follow [project rules](../../AGENTS.md). Raw fixture OS snapshots and Command cwd setup are not process.cwd calls. The six independent SDK baseline failures were assigned to226; their current checkpoint status belongs to the proof ledger, not the cwd contract.
