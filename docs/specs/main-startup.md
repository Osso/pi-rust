# Main startup

The CLI startup follows frozen `packages/coding-agent/src/main.ts` at `4f9e6f8a4`. [Module audit](../porting/traceability/main.md).

## What it must do

- [x] Apply global `httpProxy` before bootstrap HTTP configuration and after runtime settings load. Trim it, and preserve existing HTTP_PROXY/HTTPS_PROXY values, including empty strings.
- [x] Keep one AuthStorage across runtime recreation so runtime API keys survive switches.
- [x] Open cross-project sessions and restore archived selections as specified in [session cwd](session-cwd.md).

- [x] Interactive main startup and package commands present the real trust selector and persist the selected decision; UI failures propagate.

## How it works

- [Main trace](../porting/traceability/main.md).

## Implementation inventory

- `crates/pi-coding-agent/src/main.rs`: startup orchestration and runtime factory.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/main.rs`: isolated-process proxy and recreated-runtime credential tests.

## Known gaps (current cycle)

No open realistic-input item; final integration verification belongs to the parent.

## Out of scope

The debug REPL and unsupported platform/runtime faults follow existing repository approvals.
