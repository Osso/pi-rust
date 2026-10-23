# Coding-agent edit tool

Match `coding-agent/src/core/tools/edit.ts` at immutable4f9e6f8a4. [Traceability](../porting/traceability/edit.md). The [agent-core Edit record](edit.md) is a separate module.

## Arguments
- [x] Prepare arbitrary JSON arguments: parse string edits only when the parsed value is an array; malformed/non-array strings remain unchanged.
- [x] Append valid legacy oldText/newText after existing edits, drop both legacy keys, preserve all other values, UTF-16 and JS key order. Overriding an existing edits property retains its slot.
- [ ] Validate nonempty edit arrays before path resolution/execution; typed invalid runtime field types remain explicit exclusions, never silent replacements.

## Execution
- [ ] Match local/injected access/read/write, original path/content units and shared mutation queue.
- [x] Keep queue locked through operation settlement. Access failure catch observes abort first; read/write rejection retains its original ordering.
- [x] Match access-error code/String(error) selection at the actual catch boundary, including original name/code, empty messages/names and failing JSON code coercion.
- [x] Match Node byte decoding, BOM/line endings, edit application and generated diff/patch/details in the twenty-one-case default filesystem corpus.

## Preview and result
- [ ] Match renderable-input/args-key rules, pending/stale async requests and invalidation.
- [ ] Retain call-preview/result state through the frozen built-in consumers; preserve background selection and matching preview/error suppression.
- [ ] Reuse result container, clear stale children and render exact diff/error output.

## Proof
`differential_edit_write_tools` compares fourteen preparation cases and exact JSON.stringify output; RED exposed legacy merging moving edits to the end, GREEN preserves its slot. `differential_edit_operation_errors` compares fifteen actual-frozen cases: carried Error/TypeError branding, arbitrary JSON codes, coercion failure, empty values, access-abort winning and read/write rejection winning over abort. `differential_edit_filesystem` compares twenty-one real-file scenarios, exact results/details, final contents, modes and entries. Directory-read RED uses the shared promises-read formatter instead of open. `differential_edit_queue` compares nine gated real-file scenarios, pending-operation settlement, same/different-file progress and exact phase-specific abort/error ordering. Preview/result lifecycle remains open.

No new dependencies or approved deviations.
