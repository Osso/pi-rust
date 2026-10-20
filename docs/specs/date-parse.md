# Shared Date.parse

## Contract

- Parse JavaScript string inputs using the Node v26.11.1 V8 ISO and legacy scanner/composer behavior, including source quirks rather than semantic corrections.
- Accept lossless UTF-16 inputs; existing UTF-8 string APIs remain wrappers.
- Return exact integral epoch milliseconds or NaN, preserving inclusive ±8.64e15 TimeClip without native integer overflow.
- Strict date-only ISO forms use UTC; unzoned date-times and legacy dates use local time with compatible DST gap/fold resolution.
- Preserve the existing Date clock, field, and ISO-format APIs.

## Scope and proof

[Traceability](../porting/traceability/js-compat.md) records bounded Node comparisons,
implementation [reference hashes](../porting/v8-date-parser-source-lock.json), and
[license](../porting/vendor/V8-LICENSE). The implementation reference does not
replace the pinned TS runtime baseline. Finite tests do not prove every string,
time zone, platform, or Date-object operation. Typed string inputs are not
excluded by the invalid-JavaScript-runtime-type waiver.
