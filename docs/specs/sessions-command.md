# Sessions CLI

Port `packages/coding-agent/src/cli/sessions-command.ts` from frozen `4f9e6f8a4`, preserving its commands, output and persistence effects. [Traceability](../porting/traceability/sessions-command.md) maps the implementation.

## What it must do

- [x] Preserve exact help, argument validation, stdout/stderr writes and exit codes for archive, compress-archived and truncate-tool-output.
- [x] Archive with a five-day default or positive finite day count; the default effect compresses selected files, removes old files and relocates archived metadata.
- [x] Compress only archived .jsonl candidates, excluding resident ids/directories; dry-run changes no files. Preserve order, continue after per-file failures and report failures with exit1.
- [x] Preserve complete truncation reports and UTF-16 error text.

## How it works

- [Module trace](../porting/traceability/sessions-command.md).
- [Archive storage proof](../porting/traceability/session-archive-storage.md).

## Implementation inventory

- `crates/pi-coding-agent/src/cli/sessions_command.rs`: dispatch, validation, effects and reports.
- `crates/pi-coding-agent-storage/src/core/session_archive_storage.rs`: shared file publication/metadata relocation.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/sessions_command.rs`: native persistence/report fixtures and actual-source differentials.
- `oracle/sessions-command-oracle.ts`: unchanged frozen command with injected effects.

## Known gaps (current cycle)

None within the user-observable parity bar.

## Out of scope

Pathological inputs, numeric extremes and OS/runtime fault shapes, per the repository parity policy.
