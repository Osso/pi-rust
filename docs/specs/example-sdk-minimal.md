# Minimal SDK example

Port of pinned `packages/coding-agent/examples/sdk/01-minimal.ts`.
[Traceability](../porting/traceability/example-sdk-minimal.md) records proof levels.

## What it must do

- Supply default SDK options from the runnable current-thread Tokio main.
- Subscribe before the exact prompt `What files are in the current directory?`.
- Write only assistant `message_update` / `text_delta` content to stdout.
- After a successful prompt, print every message value as JSON in order, then a blank line.
- Dispose on success and prompt failure, releasing session-scoped resources.
- Report failures with the source's `Error` name and exact message, without Rust Debug string quoting; retain failure exit status.

## Approved format deviation

Alessio approved on 2026-10-20: question, “May runnable Rust SDK examples
print message objects as JSON instead Node inspect syntax and omit JS stack
frames while preserving message values, error names/messages, stream order/disposal?”
Answer: **“JSON + omit JS frames (Recommended)”**.

Only object display syntax and JavaScript stack frames differ. All message
fields, stream text, prefixes, ordering, blank lines, and error name/message/status
remain preserved. No literal source stdout byte-parity claim.

## How it works

- [Module traceability](../porting/traceability/example-sdk-minimal.md).

## Implementation inventory

- `crates/pi-coding-agent/examples/sdk/01_minimal.rs`: native SDK composition and executable main.
- `crates/pi-coding-agent/Cargo.toml`: explicit nested `sdk_01_minimal` example target.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/example_sdk_minimal.rs`: pinned program factory/observer oracle and real native SDK fixtures.
- `oracle/sdk-minimal-oracle.ts`: entire unchanged pinned program body with controlled SDK import seam.

## Known gaps (current cycle)

- [ ] Parent integration final verification; coverage remains partial until then.
- Default/live SDK configuration and provider behavior have no runtime proof in this slice; main is compiled, never executed.

## Out of scope

Default user configuration and live-provider execution are intentionally untested.
No other SDK examples or bundled extension discovery changes. Typed Rust APIs
retain existing invalid-JavaScript-input exclusions; no new input exclusions.
