# Mailbox creation-age retention

The frozen mailbox-retention module expires messages by creation time, never by a mutation timestamp. [Module proof](../porting/traceability/mailbox-retention.md).

## What it must do

- [x] Use24hours/86400000milliseconds.
- [x] Expire missing, non-string and unparseable creation values.
- [x] Compare exact shared Date.parse output and f64 time subtraction with an inclusive expiry boundary; preserve future timestamps and nonfinite clock comparison behavior.

## How it works

- [Source mapping](../porting/traceability/mailbox-retention.md).

## Implementation inventory

- `crates/pi-coding-agent-storage/src/core/session_control_db/retention.rs`: public constant and pure timestamp predicate.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/repin_storage_retention.rs`:120 actual-source unknown-ingress/date/clock combinations.

## Known gaps (current cycle)

None for this helper.

## Out of scope

SQL cleanup, control-DB persistence and mailbox-delivery transaction behavior are separate module contracts.
