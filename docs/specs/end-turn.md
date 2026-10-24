# End turn tool

Match entire `packages/coding-agent/src/core/tools/end-turn.ts` at immutable `4f9e6f8a4`.

- Exact required-string reason schema, metadata, three prompt guidelines and sequential mode.
- Reject reasons whose JavaScript whitespace trim is empty. Otherwise preserve the original reason, including surrounding whitespace, NUL and arbitrary UTF-16 units.
- Return one text block `Turn ended: {reason}`, original reason in details and `terminate: true`; no isError field.
- Context, abort signal, updates, tool-call ID and execution context do not affect execution. No custom renderers.
- Invalid JavaScript runtime field types remain outside the typed input contract; explicit blank validation is in scope.

[Traceability](../porting/traceability/end-turn.md).
