# Coding-agent write tool traceability

Pinned source: `packages/coding-agent/src/core/tools/write.ts` at immutable `4f9e6f8a4`. Current-pin module mapping complete; final milestone lint/independent gate remains separate.

Rust: `pi_coding_agent::core::tools::write`. [Contract](../../specs/write-tool.md).

| Frozen symbols / branches | Native mapping and behavioral proof |
|---|---|
| `writeSchema`, `WriteToolInput` (14–19); definition name/label/description/schema/prompt fields (181–197) | `parameters`, `WriteTool`, prompt contribution, `create_write_tool_definition`; current-pin `differential_tools_index` descriptor/schema corpus, including removed experimental sampling under PI_EXPERIMENTAL=1 |
| `WriteOperations`, `defaultWriteOperations`, `WriteToolOptions` (25–40) | Native operations/options; sixteen actual-frozen default filesystem fixtures compare result JSON, final contents, modes and root entries. `lossless_tool_paths::custom_write_receives_original_path_cwd_and_content_units` compares injected operation arguments and result against frozen |
| `WriteHighlightCache`, `WriteCallRenderComponent`, 50-line constant (42–58) | Per-component raw path/language/content, normalized/highlighted lines; `differential_write_render_sequence` preserves history past line50; no full-rehighlight substitution |
| `highlightSingleLine`, `refreshWriteHighlightPrefix` (60–74) | Native line/tail highlighting and first50 refresh; open-comment append/full-completion sequence. Empty normalized array/prefix guards are unreachable for valid internally created caches (`split` always yields at least one element); no frozen consumer mutates cache internals |
| Full/incremental cache rebuild (76–121): unavailable language, missing cache, path/language change, non-prefix rewrite, unchanged content, append | Same native transitions; frozen sequence covers plain/highlighted languages, replacement, equal-content repeat, aliases, invalid content, empty content, CR/LF split and complete/incremental transitions. Inherited ordinary-object language properties preserve native TypeError; interactive/HTML catch differential |
| `trimTrailingEmptyLines`, `formatWriteCall` (123–162): missing/invalid args, path/file_path nullish precedence, tabs/display normalization, plain/highlighted output, trailing blank lines, collapsed/expanded preview/hint | Actual-frozen call sequences plus lossless renderer corpus. Collapsed >10-line RED exposed missing literal space before keyHint; narrow fix matches plain/highlighted preview and expanded transition |
| `formatWriteResult` (164–179): non-error, empty/image-only text selection, newline joining, nonempty error | Eleven-step actual-frozen result sequence covers success, empty single text vs two empty texts, images mixed with text, raw UTF-16 and repeated errors |
| `execute` (198–228): cwd/path resolve, queue, abort before/after mkdir/write, success count and original path | Nine gated frozen scenarios, including pre-abort with invalid NUL path (key rejection precedes abort); actual files prove no early queue release, operation error wins over abort and independent files progress. [Shared queue](file-mutation-queue.md) / [Node realpath](../../specs/node-realpath.md) preserve original key-resolution failures. Filesystem corpus also proves Node UTF-8 replacement, empty/truncated writes, symlinks and failure boundaries |
| `renderCall` (230–249): previous-slot component reuse and cache lifetime | Stateful sequence uses the previously returned slot component as specified by ToolRenderContext; actual interactive callback history exercises catch/reset |
| `renderResult` (250–261): Container clear / Text setText and same-slot reuse | Native fallible callback reuses component and preserves cross-kind TypeError. Eleven direct-result steps plus six actual ToolExecutionComponent update frames prove consumer catch/reset; no interactive production edit |
| `createWriteToolDefinition`, `createWriteTool` (181–267) | Native definition/renderers and standalone wrapped AgentTool factory; descriptor/schema corpus, wrapped write execution differential and shared wrapper proofs. Native-only WriteTool convenience type is not an extra frozen export |

## Classified frozen behavior

- Success says bytes but counts UTF-16 `content.length`; preserve it, not UTF-8 length.
- Incremental highlighting colors appended tail independently and refreshes only the first50-line prefix. It can differ from a full highlight; preserve visible history.
- Result renderer assumes previous component kind. Container→nonempty error throws `text.setText is not a function`; Text→empty/success throws `component.clear is not a function`. Classified frozen renderer quirk; preserve native TypeError until interactive catch clears the slot. No source fix or added fallback.

## Proof ledger

Artifacts under `/tmp/pi-rust-repin-core-tools/`:
- `write-render-sequence-red-2.log` / `language-prototype-green-2.log`: cache RED, streaming/language GREEN; production checkpoint3543237e/c65842f8.
- `write-filesystem-realpath-red.log` / `write-realpath-green-4.log`: NUL/ELOOP RED, shared native boundary and queue/read/path regressions10/0; 3e404f91 workspace all-target0/0 in `write-realpath-workspace.jsonl/.stderr`.
- `write-result-reuse-red.log` / `write-result-reuse-green-1.log`: replacement/cross-kind RED, four tests GREEN covering real consumer frames and direct history; production dba6631d.
- `write-preview-branches-red.log` / `write-preview-branches-green.log`: collapsed-hint spacing RED; expanded branch matrix and result sequence GREEN2/0. At5785f150: `write-closure-check.jsonl/.stderr` package all-target0errors/0warnings; `write-closure-lossless.log`12/0 including raw custom-write arguments and renderer paths; `write-queue-preabort-key-green.log`1/0 across nine scenarios. Earlier filesystem/realpath scopes remain unchanged. Format/readability audited; pre-existing global clippy findings remain deferred to parent final gate.

## Boundaries

Invalid execution input types are excluded by the approved typed API contract; renderer-invalid arguments remain covered. JS object mutation through externally retained references follows the approved owned-value rule; the frozen built-in consumes only its previously returned slot component. Existing approved Mathematica/R grammar gap remains; no new write deviation.
