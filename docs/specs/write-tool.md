# Write tool

Match frozen `coding-agent/src/core/tools/write.ts` at `4f9e6f8a4`: queued whole-file writes and their stateful previews. [Traceability](../porting/traceability/write.md) maps implementation and proofs.

## What it must do

### Execution
- [ ] Resolve the original string path against the tool cwd; injected operations receive its exact UTF-16 path/content.
- [x] Serialize operations on the resolved file; check abort before mkdir, after mkdir and after write, without releasing the queue while an awaited operation remains active.
- [x] Preserve operation failure ordering; abort observations report the frozen Operation aborted message, not the signal reason.
- [ ] Create parent directories, write Node UTF-8 bytes, and report the frozen UTF-16 content.length count labelled bytes.

### Rendering
- [ ] Preserve streaming history across calls, including the observable 50-line highlight-prefix boundary and complete/incremental transitions.
- [ ] Rebuild on path/language changes or non-prefix content; clear unavailable-language/invalid-content state.
- [ ] Match path aliases, invalid-content text, tabs, display normalization, trailing-empty-line removal, collapsed/expanded previews and expansion hints.
- [ ] Show only nonempty text errors in result rendering; successful results render an empty container.

## How it works
- [Module traceability](../porting/traceability/write.md)
- [Shared mutation queue](../porting/traceability/file-mutation-queue.md)

## Implementation inventory
- `crates/pi-coding-agent/src/core/tools/write.rs` — write schema, operations, execution and rendering.
- `crates/pi-coding-agent/src/core/tools/file_mutation_queue.rs` — shared file serialization.
- `crates/pi-coding-agent/src/core/tools/render_utils.rs` — display/path normalization.

## Tests asserting this spec
- `crates/pi-coding-agent/tests/edit_write_tools.rs`
- `crates/pi-coding-agent/tests/differential_edit_write_tools.rs`
- `crates/pi-coding-agent/tests/differential_write_render_sequence.rs`
- `crates/pi-coding-agent/tests/differential_write_queue.rs`
- `crates/pi-coding-agent/tests/lossless_tool_paths.rs`

## Known gaps (current cycle)
- [ ] Current-pin full execution/failure/abort ordering matrix.
- [ ] Current-pin stateful render sequence proof and consumer reuse audit.

## Out of scope
- Invalid execution input types excluded by typed WriteToolInput; renderer-invalid arguments remain in scope.
- Existing approved Mathematica/R highlighter grammar gaps; no new highlighter deviation.
