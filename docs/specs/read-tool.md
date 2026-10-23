# Read tool

Frozen authority: `packages/coding-agent/src/core/tools/read.ts` at 4f9e6f8a4. [Traceability and remaining audit scope](../porting/traceability/read.md).

- Preserve file/image output, renderer classification, cancellation ordering and operation error boundaries from frozen.
- Offset, limit, stat size and byte-range endpoints retain JavaScript binary64 semantics through the arithmetic; only the consuming array/OS boundary coerces values.
- A range reader is one optional callback, not a callback plus a separate capability flag. Absent callbacks select full reads. Present callbacks select bounded head reads only for the frozen size/offset/limit conditions.
- Local reads check R_OK and retain distinct open/read/close errors. A range read performs one positioned read and closes the handle even after read failure.
- Custom operations retain their diagnostics; local errors use the [shared Node/libuv authority](node-libuv-errors.md).
