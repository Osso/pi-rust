# Coding-agent read tool — current-pin traceability

## Shared slice-index correction (current pin)

Ls/RTK reuse exposed a negative-fraction bug in read's private slice helper: sign was tested before ToIntegerOrInfinity. Actual-frozen limit:-0.5 and negative-subnormal RED (`read-negative-fraction-slice-red.log`) select no lines in frozen but the full file natively. Read now delegates [shared js_slice_index](../../specs/js-slice-index.md), truncating before sign/clamp.17-case numeric differential GREEN in `ls-numbers-green-1.log`; no new read deviation.

Frozen source: `packages/coding-agent/src/core/tools/read.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d` (505 lines).
Rust: `crates/pi-coding-agent/src/core/tools/read.rs`. Contract: [read-tool](../../specs/read-tool.md). Historical 532f7d12 evidence is superseded by the current-pin proofs below.

## Symbol and branch coverage

| Frozen statements / symbols | Native behavior | Current-pin proof |
|---|---|---|
| 26–41 readSchema, ReadToolInput, ReadToolDetails | Required string path; optional binary64 offset/limit; optional truncation details | `differential_read_tool`, `differential_read_numbers`; invalid JS runtime input types remain excluded by the typed-API decision |
| 44–60 CompactReadClassification, eight resource basenames | Skill/docs/resource classification, all exact resource names; no old-only AGENTS.override.md | `differential_tool_renderers::read_renderer_branch_matrix_matches_frozen` |
| 64–79 ReadFileStat, ReadOperations | Custom full read/access; optional range/stat/MIME; stat numbers keep NaN/Infinity; empty MIME is text | `differential_read_numbers` (15 custom-operation cases, operation traces and tagged nonfinite metadata) |
| 81–99 readLocalRange, defaultReadOperations | R_OK; one positioned read; bounded returned bytes; explicit close with finally error precedence; native open/stat/access/read/close messages | `read_tool`, `differential_read_filesystem`, `custom_operation_errors`, `lossless_local_path_validation`; shared [node-fs authority](node-fs-errors.md) |
| 101–122 isPathWithinRoot, resolveAgentConfigReadPath, resolveReadToolPath | Empty root unrestricted; requested/root realpaths in parallel; root equality/contained paths allowed, sibling/parent/symlink escapes denied before access | `differential_read_config_root` (10 real-filesystem cases, including missing/NUL/root aliases) |
| 124–139 options/render args | Default image resize true; custom operations; file_path nullish preference for rendering | `read_tool`, `differential_read_model_note`, renderer differential |
| 142–160 formatReadLineRange, formatReadCall, trimTrailingEmptyLines | Fractional/zero/NaN line ends use JS arithmetic/truthiness; shared hyperlink/shortening path errors; trailing empty-line trimming | `differential_tool_renderers`, `differential_render_home`, `differential_read_classification` |
| 162–167 getNonVisionImageNote | Optional live context model read after MIME; text-only models append the note to processed and omitted images without dropping a produced image block | `differential_read_model_note` (8 cases: absent/text/image/mixed context × processed/omitted) |
| 169–204 canUseHeadRangeRead, readTextBuffer | Size > 10KB, optional callback present, offset <= 1, absent limit or limit > 2000; otherwise full read; optional stat metadata | `differential_read_numbers`, `read_tool::avoids_full_reads_for_oversized_files` |
| 206–256 toPosixPath, getPiDocsClassification, getCompactReadClassification | POSIX literal backslashes; skill before docs before resources; README/docs/examples containment; path/config errors propagate | `differential_read_classification` (12 isolated-process cases, including deleted cwd, hyperlink and malformed package URL); renderer basename matrix |
| 258–280 formatCompactReadCall | Exact skill styling and docs/resource labels, range and expand hint | Classification and renderer differentials |
| 282–322 formatReadResult | Collapsed success hidden; errors still highlighted; expanded/all vs collapsed/10 lines; more-lines hint; first-line/line/byte truncation warnings and nullish maxima | Renderer differential plus 17-case read branch matrix |
| 324–344 createReadToolDefinitionInternal metadata | Name/label/read-only approval, description, schema, snippet/guidelines; no retired constrainedSampling | `differential_tools_index`, `differential_tool_renderers`, execution differentials |
| 345–376 promise setup, path/access/MIME and cancellation | Pre-abort rejects before work; prompt abort settlement independent from retained underlying work; path/access checkpoints; MIME continuation follows frozen | `read_tool::aborted_reads_return_the_source_error`, `differential_read_abort` (12 gated operation/result/trace cases) |
| 377–402 image branch | MIME truthiness, full read, processImage options, omitted note/hints/image content | `read_tool`, model-note differential; empty detector string guarded in numeric-operation differential |
| 403–462 text branch | Node UTF-8 decode; split newline; truthy offset/max arithmetic; out-of-range error; native Array.slice index coercion; user limit; range/first-line/line/byte/remaining/no-notice branches | `differential_read_tool` (18 text/file cases), `differential_read_numbers`, `read_tool` (12 regressions) |
| 463–471 final settlement and rejection | Final abort check; listener removal; late IO failures cannot overwrite abort; custom error messages remain exact | Abort differential; `custom_operation_errors` |
| 475–488 renderCall/renderResult | Same rendered Text output and fallback boundary; native Text values are owned rather than reused through a JS reference | Renderer differentials, `tool_execution_renderer_errors`; consumer audit below |
| 492–505 exported createReadToolDefinition, createAgentConfigReadToolDefinition, createReadTool | Definition with optional context; restricted root factory; context-free AgentTool wrapper and shared output cap | Execution/config-root/model-note/index differentials; [wrapper traceability](tool-definition-wrapper.md) |

## Error and returned-component consumer audit

`ToolError` remains message-only under the approved frozen-consumer audit. Frozen `agent-core/src/agent-loop.ts:798–804,845–850` converts Error.message (String only for non-Errors); afterToolCall receives that converted result (868–887). Coding-agent `agent-session.ts:2398–2403` does the same before its after hook; `1152–1170` emits tool_result with content/details/isError, not a rejection object. Tool-definition wrapper `119/155`, extension wrapper `26`, bwrap, SSH/minimal-mode/renderer overrides and sandbox/gondolin adapters propagate the promise without inspecting error fields. Permission-tool rejection is discarded at `permissions/mcp-permission-prompt.ts:93–101`. Print/RPC consume finalized result/events (`modes/json-event.ts:57–65`); no consumer reads an execute error's code/class/String(Error) prefix. Read renderer errors are different: PathError/ConfigError→HandlerError retains the native error, until ToolExecution's real renderer catch substitutes its fallback.

Frozen read renderers reuse their previous Text and mutate it through the returned reference. `tool-execution.ts:363–402` clears the render container, stores the returned component and adds it; it has no identity-dependent branch for this plain Text. Native owned Text output therefore follows the approved owned-return/reference-mutation decision (2026-10-22), with no lost built-in consumer behavior. Image/terminal/highlighting and shared path-helper contracts remain in their own module traceability.

## Source discrepancies preserved

Fractional/negative slice operands can produce fractional or counterintuitive remaining-line notices: frozen notice arithmetic uses uncoerced values. A fractional startLine on an oversized first line reads allLines[startLine] as undefined even though Array.slice coerced its index; Buffer.byteLength then throws the exact type error. Native reproduces this failure instead of silently flooring the property access. `differential_read_tool` and `differential_read_numbers` cover these cases.

## Proof ledger

Lane artifacts are under `/tmp/pi-rust-repin-core-tools/`. Read production behavior last changed at `bb7ddaba` (empty MIME truthiness); later config/runtime carrier changes do not alter read's execute/render decision logic.

| Evidence | Scope / result |
|---|---|
| read-optional-range-{red,green}.log; read-renderer-{red,green}.log | Missing callback-vs-flag behavior; 17 renderer branches corrected and matched |
| read-model-note-{red-2,green}.log; read-config-root-{red,green}.log | Live model notes and canonical config-root factory, behavioral RED/GREEN |
| read-abort-{red-2,green}.log | All 12 gated abort cases failed prompt settlement/order before the fix and matched afterward |
| read-classification-{red-2,green,green-2}.log; config-regressions-17.log | POSIX/classification/config/hyperlink failures; current-pin classification, config, keybindings/system-prompt batch 26 passed |
| read-empty-mime-{red,green}.log | Empty custom MIME rejected the wrong branch before correction; all 15 numeric/custom-operation cases pass |
| read-final-matrix-23.log | 31 tests passed across 11 read/error/renderer/lossless targets; one isolated child entry point ignored by runner |
| config-all-consumer-carriers-green.log; config-helper-workspace-35.log | Config consumer native identity/UTF-16 follow-up passed; workspace all-target check warning-free at db3ad23a |

No unimplemented read.ts export or branch remains under the approved POSIX/typed-API/owned-value contracts. Workspace milestone lint/readability/independent gates are parent-owned, not implied by this module's behavioral proof.
