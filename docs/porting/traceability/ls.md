# coding-agent ls tool traceability

Frozen `packages/coding-agent/src/core/tools/ls.ts` at immutable `4f9e6f8a4`; [source lock](../source-lock.json). Rust: `core/tools/ls.rs`. [Contract](../../specs/ls-tool.md).

## Symbol and branch map

| Frozen symbols / branches | Rust representation | Behavioral proof |
|---|---|---|
| `lsSchema`, `LsToolInput`, `DEFAULT_LIMIT`, `LsToolDetails`, `LsToolResult` | Schema + raw `JsValue` arguments/details, `f64` limits and `AgentToolResult`; default500 | `differential_ls_tool`, `differential_ls_numbers`; exact content/details including nonfinite wire projection |
| `LsOperations`, `defaultLsOperations`, `LsToolOptions` | `LsOperations`/`LocalLsOperations`, optional owned operation provider | `differential_ls_filesystem`:8 real-filesystem cases; injected numbers/abort differentials |
| `createLsToolDefinition`: name/label/approval/description/prompt/parameters; custom operations disable RTK | `create_ls_tool_definition`, `LsTool::with_options`, schema and contribution | Existing tool-definition/index/wrapper proofs; `differential_ls_tool`; custom fixtures work independently of installed RTK |
| Execute pre-abort, active abort, retained async producer, listener settlement | Shared [first-winner observer](../../specs/tool-promise-observer.md) | `differential_ls_abort`:9 actual frozen gated histories; read observer regression retained |
| Resolve `path || "."`; exists false/reject; root stat false/reject; readdir catch prefix | `list_with_operations`, shared path/native-error authorities | `ls_tool`; `core_ops_ls_exists_rejection_precedes_stat_and_readdir`; custom stat/readdir errors; filesystem missing/file/EACCES/post-exists ENOENT/post-stat ENOENT/ENOTDIR |
| Case-folded locale sort; check limit before stat; skip failed entry stat; directory suffix; empty result without notices | `list_with_operations`, UTF-16 collator |17 injected numeric cases assert exact calls and results; filesystem dotfiles, ordinary entries, directory links and broken/cyclic/invalid-UTF-8-name skips |
| Byte/entry notices, their order, details presence, ordinary/empty/first-line-too-long output | Shared `truncate_head`, binary64 notice spelling | `ls_tool` byte cap; numeric differential; RTK combined byte/entry-limit and oversized-first-line frozen fixtures |
| `executeRtkLs`, `runRtkLs`, `attachCommandOutput`, `isMissingExecutableError`, `formatRtkLsError`, `isCommandError` | RTK spawn/shared capture + `RtkError`; missing ENOENT alone permits source-required filesystem path | `ls_rtk` executable/missing-binary paths; `differential_ls_rtk_io`: ordinary stdout/stderr preference, BOM/U+0085 trimming, quiet failure, argv/cwd diagnostics and ordering, whitespace-only overflow labels; native multi-megabyte both-stream termination/safety |
| `formatRtkLsOutput`: trimEnd/filter/slice, independent entry comparison and byte truncation | JS whitespace, shared [slice index](../../specs/js-slice-index.md), native output formatter |17 executable numeric cases; RTK IO multiline/single-line/combined truncation cases |
| `formatLsCall`, `formatLsResult`, render callbacks | Lossless text, source numeric truthiness/spelling, reused `Text` component; native TypeError for incompatible component | `differential_ls_render_sequence`:16 frozen call/result frames, collapsed/expanded/hint, both warning types, raw text, images, zero/NaN/infinities/fractions; existing renderer tests |
| `createLsTool` | Shared result-cap wrapper around native tool | Shared wrapper/index proofs plus wrapped execution differential |

## RED → GREEN evidence

Artifacts: `/tmp/pi-rust-repin-core-tools/`; command/revision proof ledger: `ledger.md`.

- Numeric REDs: `ls-numbers-red.log`, `ls-rtk-numbers-red.log`; GREEN `ls-numbers-green-1.log`. Signed/fractional/nonfinite limits no longer silently default or coerce to unsigned integers.
- Retained-producer RED: `ls-abort-producer-red.log`; GREEN `ls-read-abort-green-1.log`. Active abort no longer cancels custom producer work or invents a post-readdir check.
- Renderer RED: `ls-render-sequence-red.log`; GREEN `ls-render-sequence-green-3.log`. Reuse/hint spacing/warning truthiness fixed; direct nonfinite callback values stay numbers, not JSON null fixtures.
- RTK RED: `ls-rtk-io-red.log`, `ls-rtk-validation-red-2.log`; GREEN `ls-rtk-capture-checkpoint-green.log`, `ls-rtk-validation-green.log`, `ls-rtk-truncation-green.log`. Bounded shared capture removes byte-slice panic/unbounded reads; frozen trim/error/validation text retained. Final `ls-closure-focused.log` GREEN9/0 covers15 ordinary/validation/truncation cases, including both whitespace-only overflow labels, native safety, common-pin execution and ordinary/executable/fallback controls.
- Filesystem RED: `ls-filesystem-red.log`; GREEN `ls-filesystem-green.log`. Actual read/open failures now carry native `scandir` errors until the outer frozen prefix. Fixtures mutate real files at the observed exists→stat→readdir boundaries, not mocked error strings.

## Final gate

At `f2a469ec`: `ls-closure-check.jsonl/.stderr` package all-target0 errors/0 warnings; focused9/0 and `ls-closure-custom-exists.log`1/0. Earlier unchanged numeric/renderer/abort/shared-capture and filesystem proofs remain valid. Parent `cc27ef61` removes the HarnessEvent prerequisite lint blocker and is merged. Clippy now reaches coding-agent; its package-wide pre-existing lint debt remains parent-owned. Five ls borrow-only lint findings were removed alongside six find findings without changing semantics. Ls port map is complete; final lint gate remains open until the current own-file diagnostics are rechecked.

## Explicit exclusions and gaps

| Status | Case | Classification |
|---|---|---|
| gap | Node execFile chunk-dependent UTF-16 overflow prefix | Approved2026-10-23 native-runtime policy: [shared capture](../../specs/child-process-capture.md) caps both streams, terminates/reaps early and safely decodes/clamps UTF-8. Configured cap, ordinary output and maxBuffer error labels remain exact. |
| excluded | Invalid JavaScript runtime input types / malformed required typed metadata | Approved typed-Rust API boundary. `TruncationResult.maxBytes` is required; the renderer's nullish compatibility branch is not produced by valid typed results. |
| approved ownership | JS object identity/mutation through returned values | Owned values per2026-10-22 approval; built-in renderer component reuse is explicitly preserved rather than waived. |

No other ls semantic deviation. Shared filesystem, collation, path, wrapper and truncation contracts remain their own authorities; this row does not claim those whole modules are newly re-pinned.
