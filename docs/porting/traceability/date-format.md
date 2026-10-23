# Shared Date locale formatting

Authority: frozen callers use `new Date(number)` plus default en-US locale display or the tool component's explicit en-GB h23 clock. [Contract](../../specs/date-format.md). Single implementation: `crates/pi-js-compat/src/date.rs`.

| Surface / branch | Representation | Proof |
| --- | --- | --- |
| to_locale_string / explicit-zone test wrapper | one implementation selected by LocaleDateFormat: DateTimeEnUs, TimeEnUs, TimeEnGb | `differential_date_format`, actual pinned Node Intl outputs |
| TimeClip | full f64 input; truncate only at actual Date construction; inclusive ±8.64e15ms; invalid values become Invalid Date | bounds ±1ms, fractions, negative zero, NaN/±Infinity |
| local_date_fields / utc_date_fields | manual Gregorian calendar after local offset, retaining full original year |1891 instants ×7 zones versus Node local getters |
| local offset beyond Jiff calendar | offset-only Gregorian400-year lookup for future POSIX rules, ancient LMT; no narrowing of original instant/calendar | UTC, America/Chicago, Europe/Paris, Asia/Kolkata, Australia/Lord_Howe, Pacific/Apia, Africa/Monrovia; year0/BCE, negative/positive large years, historical second offsets, year/day crossings |
| Date.parse equivalent offset year | same mapped-year authority as formatting; existing wall-clock gap disambiguation retained | existing parser differential/native regression gate |
| tool completion | delegates to TimeEnGb; no local Jiff/clock implementation |45 frozen/native non-image sequences,24 wide/invalid/fractional completion cases across generic/default/self shells |
| examples | input-transform uses DateTimeEnUs; message-renderer/overlay clock use TimeEnUs; entry-renderer uses DateTimeEnUs | mechanical shared-API adaptation; direct helper corpus does not assert complete example lifecycle |

RED `test-date-format-red-1.log`:284 inherited-zone locale cases and1988 explicit-zone calendar cases returned Invalid Date/None where Node returned valid wide dates. GREEN `test-date-format-green-1.log`: both full-range tests pass after expanding the corpus with1024 deterministic uniformly distributed full-range instants. Each1891-value corpus is compared across three locale layouts and local getters in seven zones; inherited process-zone display also matches. `test-tool-clock-wide-green-1.log`:45 source/native tool sequences pass.

Manual Gregorian splitting already existed for full-range UTC/ISO output. Only the timezone offset lookup uses Jiff's representable equivalent year; the original year is never replaced in output. No libc, duplicate formatter, alternate path or dependency was added. No pi-ai or agent-core source call sites needed changes.

Examples' general source harness remains blocked by its absent common-pin `coding-agent/test/model-runtime-test-utils.ts` import. That is recorded separately, not counted as GREEN. Parent owns final independent/readability gates.
