# Provider payload extension example

Opt-in native counterpart of pinned `coding-agent/examples/extensions/provider-payload.ts`.

## What it must do

- [x] Append pretty JSON request payloads and two newlines to `cwd/CONFIG_DIR_NAME/provider-payload.log`, without changing the payload.
- [x] Append `[status] `, compact JSON response headers in observable record order, and two newlines to the same file.
- [x] Preserve existing content; create the file but not its parent directory. Surface append errors through extension runtime error reporting.

## How it works

- [Native embedding](../../crates/pi-coding-agent/examples/extensions/README.md)
- [Traceability](../porting/traceability/extension-example-provider-payload.md)

## Implementation inventory

- `crates/pi-coding-agent/examples/extensions/provider_payload.rs`: explicit registration and synchronous append handlers.
- `oracle/example-provider-payload-oracle.ts`: pinned example through real TS factory and runner, private temporary filesystem.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/example_provider_payload.rs`: native registry and runner; exact file-content, unchanged-payload and error-event comparison against TS.

## Known gaps (current cycle)

None. Targeted differential passes all three private-filesystem scenarios.

## Out of scope

- Autoloading, live configuration or logs, dynamic TypeScript loading, network calls, invalid untyped inputs. Native opt-in embedding is the approved extension-loading deviation.
