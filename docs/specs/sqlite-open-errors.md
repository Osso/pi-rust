# SQLite open errors

Frozen `core/sqlite.ts` at `4f9e6f8a4` delegates to actual `node:sqlite`.

## What it must do

- [x] Writable and read-only opens report SQLite's error message without rusqlite's added path suffix.
- [x] Preserve native Error branding and the exposed `code`, `errcode`, and `errstr` properties through `ControlDbError` and `CallbackError`.
- [x] Keep contention classification and non-open error mapping unchanged. Do not repair error text at tool callers.

## How it works

- [SQLite traceability](../porting/traceability/sqlite.md).

## Implementation inventory

- `crates/pi-coding-agent/src/core/sqlite.rs`: open-boundary error projection.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/differential_sqlite_open_errors.rs`: real frozen/Node writable and read-only opens against a regular-file parent, directory, and missing parent.
- `tests/session_control_db.rs` and `tests/cross_implementation_control_db.rs`: retained database/SQLite behavior.

## Known gaps (current cycle)

- Final focused and checkpoint gates pending; the direct six-case differential is RED before the correction.

## Out of scope

- Other SQLite error families, statement/binding API changes, unrelated control-store behavior, and Bun runtime changes.
