# Shared local Date formatting

The sole `pi_js_compat::date::to_locale_string` authority formats the frozen consumers' JavaScript Dates. [Traceability](../porting/traceability/date-format.md) records the full-range Node proof.

## What it must do

- [x] Apply TimeClip at Date construction, including fractional milliseconds, invalid numbers and the inclusive ±8.64e15ms bounds.
- [x] Retain the original Gregorian year over the full JavaScript Date range; local calendar fields must not stop at Jiff's calendar bounds.
- [x] Format DateTimeEnUs, TimeEnUs and TimeEnGb from the same local-calendar authority; Invalid Date is a string, not an unavailable-calendar error.
- [x] Resolve historical second offsets, future DST and local day/year crossings; equivalent-year mapping changes only the offset lookup.
- [x] Route tool completion and example date displays through this authority, without a separate local formatter or early integer clamp.

## How it works

- [Date formatting proof](../porting/traceability/date-format.md)
- [Date parsing contract](date-parse.md)

## Implementation inventory

- `crates/pi-js-compat/src/date.rs`: sole parameterized locale formatter, local-calendar fields and shared Gregorian/equivalent-year authority.
- `crates/pi-coding-agent-ui/src/modes/interactive/components/tool_execution.rs`: completion display delegates to TimeEnGb.
- `crates/pi-coding-agent/examples/extensions/{input_transform,message_renderer,entry_renderer,overlay_qa_tests}.rs`: mechanical date-display consumers.

## Tests asserting this spec

- `crates/pi-js-compat/tests/differential_date_format.rs`:1891 instants ×7 explicit zones, three formats plus local getters; inherited-zone display also compared.
- `crates/pi-coding-agent/tests/differential_tool_execution.rs`:45 actual frozen/native non-image sequences, including24 full-range/TimeClip completion cases.
- Existing date parser and native calendar tests remain the regression gate.

## Known gaps (current cycle)

- [ ] Parent final independent/readability gates.
- [ ] Existing general example differentials have a pre-existing missing frozen model-runtime test-helper import; this does not qualify as passing example lifecycle proof.

## Out of scope

Locale layouts remain pinned to en-US and the source's explicit en-GB clock. The local time zone is not pinned. No new locale, parser grammar, platform restriction or dependency is introduced.
