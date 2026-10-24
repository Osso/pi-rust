# Extension examples H: built-in tool renderer

Source: pinned `coding-agent/examples/extensions/built-in-tool-renderer.ts`.
Native source: `crates/pi-coding-agent/examples/extensions/built_in_tool_renderer.rs`.
Embeddable via `#[path]`, explicitly registered `ExtensionRegistry` factory; never default-loaded.

## Observable coverage

| Source branch | Rust | Behavioral proof |
|---|---|---|
| Factory captures process cwd; register read/bash/edit/write in order, edit self shell | `built_in_tool_renderer_extension` | `example_built_in_tool_renderer`: full ordered metadata and real runner execute IO |
| read call: path, offset/limit truthiness | `render_call` | all rendered ANSI lines at widths 1/7/16/40/80/120 |
| bash call: UTF-16 80-unit cutoff, 77-unit prefix, timeout | `render_call` | short/exact/long/astral commands, absent/zero/integral-float/fractional timeout; JS numeric interpolation |
| edit/write calls, write newline count | `render_call` | Unicode path, empty/trailing-newline content |
| Every partial result | `render_result` | both expanded states for each fixture, partial takes precedence |
| read image/empty/text/error, truncation, expanded 15-line cap | `render_read` | empty/image/text/trailing-newline/styled/18-line/error fixtures |
| bash first positive-digit exit match, zero/no match, JS trim line count, truncation, expanded 20-line cap | `render_bash` | empty/blank/BOM/zero/nonzero/negative-before-leading-zero/23-line fixtures |
| edit Error first-line, absent/empty diff, +/- counts exclude file headers, expanded 30-line cap | `first_error`, `render_edit`, `diff_color` | Error-with-diff, empty, headers/add/remove/context/trailing-newline, 34-line diff |
| write Error first-line or Written | `first_error`, `render_result` | absent/text/error results |
| Original tool execution delegation, success/error propagation | original `create_*_tool` via definition wrapper | actual read offset/limit, bash success/nonzero, multi-edit success/missing-text, write; complete results/errors and file bytes |

Proof compares complete output against unchanged pinned extension through real TS loader/runtime/runner and pinned pi-tui `Text`, using shared `tests/support/{extensions,oracle}.rs` plus batch-specific `extension_examples_h.rs`. 94 render scenarios × six widths, seven executions. Fixtures remain in private temp dirs under cargo runner isolation. No oracle/binary is run directly.

Typed inputs follow core exclusion of invalid JS runtime types. **2026-10-22 exact-UTF-16 decision** ([AGENTS.md](../../../AGENTS.md)) supersedes the former String-boundary U+FFFD projection. The command fixture with76 ASCII units before an astral pair splits it at slice(0,77): compare exact JsString units, including the unmatched high surrogate followed by `...`. No `toWellFormed` oracle shim or serde_json output projection remains. Actual frozen ModelRegistry.inMemory replaces retired model-runtime-test-utils. RED `q15b-example_built_in_tool_renderer-red.log` exposed the raw-surrogate transcript boundary; GREEN `q15b-example_built_in_tool_renderer-current-4.log` covers all94×6 renders and seven executions. JS stack frames remain excluded. No production changes or new dependencies.

Four originally assigned examples (border-status-editor, modal-editor, rainbow-editor, overlay-test) were reassigned to `fix/extension-ui-editor-focus` after missing editor host fixed-cell and extension component focus APIs were reported; this slice leaves those sources/coverage rows untouched.

## Accepted review regression (2026-10-20)

F2: `example_built_in_tool_renderer` compares float-backed truncation totalLines 321.0 at all widths/expanded/partial states; visible count is 321, not JSON spelling 321.0.
