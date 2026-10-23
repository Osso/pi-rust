# Process cwd and chdir

Mirror Node26.11.1 process working-directory state through one shared authority. [Locked built-in source](../porting/node-process-cwd-source-lock.json) and [branches/consumer inventory](../porting/traceability/process-cwd.md).

## What it must do

### Cached process state

- [x] Cache successful cwd reads; retain that value after external directory deletion.
- [x] Failed reads leave the cache empty and preserve native Node ENOENT/uv_cwd metadata.
- [x] Successful chdir clears the cache; failed chdir leaves it unchanged.
- [x] Decode OS paths through the shared Node byte decoder; chdir uses Node's UTF-8 C-string prefix, including NUL truncation and surrogate replacement at the OS boundary.
- [x] Preserve chdir name/message/code/errno/syscall/path/dest for normal existing old cwd, including missing/file/empty destinations.

### Consumers

- [x] Use the shared getter in path resolution, SDK/settings/startup, executable lookup, restart, architect, bwrap worker, catalog generator and native examples; no cwd fallback to `/` or `.`.
- [x] Carry cwd errors through native wrappers to their real output/catch boundaries rather than converting them to text early.
- [x] Absolute Architect project paths do not read process cwd; relative paths do.
- [x] Isolate process-cwd mutation tests in child processes; fixture directory changes update the shared authority.

## How it works

- [Process cwd traceability](../porting/traceability/process-cwd.md)
- [Shared Node error metadata](node-libuv-errors.md)

## Implementation inventory

- `crates/pi-js-compat/src/process_cwd.rs` — one synchronized process cache and cwd/chdir entry points.
- [Consumer inventory](../porting/traceability/process-cwd.md#workspace-consumer-inventory) — exact production and example paths.
- `crates/pi-coding-agent/examples/sdk/error.rs` — native source retained through sample entrypoint formatting.

## Tests asserting this spec

- `crates/pi-js-compat/tests/process_cwd.rs`, `oracle/process-cwd-primitive-oracle.mjs` — real Node26.11.1 primitive sequence and explicit pending discrepancy assertion.
- `crates/pi-coding-agent/tests/process_cwd.rs`, `oracle/process-cwd-oracle.mts` — actual frozen SessionManager warm/cold behavior, eight native consumers, and actual frozen Architect absolute-path case.

## Known gaps (current cycle)

- [ ] **Pending user approval:** failed chdir when the old physical cwd is deleted. Node can expose unstable old-path data; native intentionally uses an empty old path. Only that path and its message insertion differ; name/code/errno/syscall/dest/cache behavior remain asserted. This is not yet an approved semantic deviation.
- [ ] Six existing SDK example targets and custom-prompt date/newline expectations are RED independently of this cwd work; parent reproduced all six at bb562e5c and assigned reconciliation to226. No assertions were removed.

## Out of scope

Working-directory recovery lifecycle hooks remain in [agent-session-runtime](agent-session-runtime.md). Invalid JavaScript runtime input types and approved Windows gaps follow [project rules](../../AGENTS.md). Raw fixture OS snapshots and Command cwd setup are not ported process.cwd calls.
