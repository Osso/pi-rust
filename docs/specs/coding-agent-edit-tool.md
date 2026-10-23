# Coding-agent edit tool

Match `coding-agent/src/core/tools/edit.ts` at immutable4f9e6f8a4. [Traceability](../porting/traceability/edit.md). The [agent-core Edit record](edit.md) is a separate module.

## Arguments
- [x] Prepare arbitrary JSON arguments: parse string edits only when the parsed value is an array; malformed/non-array strings remain unchanged.
- [x] Append valid legacy oldText/newText after existing edits, drop both legacy keys, preserve all other values, UTF-16 and JS key order. Overriding an existing edits property retains its slot.
- [ ] Validate nonempty edit arrays before path resolution/execution; typed invalid runtime field types remain explicit exclusions, never silent replacements.

## Execution
- [ ] Match local/injected access/read/write, original path/content units and shared mutation queue.
- [ ] Keep queue locked through operation settlement. Access failure catch observes abort first; read/write rejection retains its original ordering.
- [ ] Match access-error code/String(error) selection at the actual catch boundary.
- [ ] Match Node byte decoding, BOM/line endings, edit application and generated diff/patch/details.

## Preview and result
- [ ] Match renderable-input/args-key rules, pending/stale async requests and invalidation.
- [ ] Retain call-preview/result state through the frozen built-in consumers; preserve background selection and matching preview/error suppression.
- [ ] Reuse result container, clear stale children and render exact diff/error output.

## Proof
`differential_edit_write_tools` compares fourteen preparation cases and exact JSON.stringify output; RED exposed legacy merging moving edits to the end, GREEN preserves its slot. Whole execution/preview cycle remains open.

No new dependencies or approved deviations.
