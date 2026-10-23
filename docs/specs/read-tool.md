# Read tool

Frozen authority: `packages/coding-agent/src/core/tools/read.ts` at 4f9e6f8a4. [Traceability and remaining audit scope](../porting/traceability/read.md).

- Preserve file/image output, renderer classification, cancellation ordering and operation error boundaries from frozen.
- After MIME detection, read the optional live extension context model. Non-vision models append the frozen image-omission note to both processed and omitted image text, without removing an otherwise produced image block. No context/model or an image-capable model adds no note.
- Offset, limit, stat size and byte-range endpoints retain JavaScript binary64 semantics through the arithmetic; only the consuming array/OS boundary coerces values.
- A range reader is one optional callback, not a callback plus a separate capability flag. Absent callbacks select full reads. Present callbacks select bounded head reads only for the frozen size/offset/limit conditions.
- Local reads check R_OK and retain distinct open/read/close errors. A range read performs one positioned read and closes the handle even after read failure.
- Custom operations retain their diagnostics; local errors use the [shared Node/libuv authority](node-libuv-errors.md).
- The agent-config definition canonicalizes requested path and allowed root before access. Reject symlink/parent/sibling escapes with the frozen original-path diagnostic; preserve realpath errors and an empty root's frozen unrestricted behavior.
