# Inline external editor

Frozen source: `coding-agent/src/modes/interactive/interactive-mode.ts:5447–5507`. No frozen standalone `external-editor.ts` exists. [Contract](../../specs/external-editor.md).

## Literal flow re-port

The old native `external_editor.rs` module, `ExternalEditorOptions`/`ExternalEditorResult`, private directory/`prompt.md` lifecycle and corresponding old-only test expectations are deleted, not retained as a parity gap (parent decision2026-10-24).

Before production changes, `tmpdir_external_editor_matches_frozen_inline_flow` runs an isolated native process and the actual frozen method with a real TUI and fake editor process. Genuine RED `tmpdir-external-red.log`: native path parent is `tmpdir/pi-editor-<random>`, frozen parent is `tmpdir` and frozen filename is `pi-editor-0.pi.md`. The fixture also records TUI stop/start file snapshots, initial UTF-8 bytes, argv including empty arguments, read-back and cleanup, and a heartbeat proving event-loop progress while the editor runs. Full directory snapshots remain asserted; source-first execution stabilizes incidental tsx cache creation. Native timestamp is validated against before/after clock bounds before only its nonce is normalized.

Logs and exact revision/scope ledger: `/tmp/pi-rust-repin-lossless-interactive/tmpdir-external-*`. Shared `node_os::tmpdir` from parent `a01ef666` supplies the inline path. Exact real-process/callback GREEN at `bc245b39`: `external-editor-process-callback-matrix.log` (1 test,13 isolated children,115 seconds). Cases: successful edit, nonzero exit, signal, empty content, missing executable, read ENOENT, read EISDIR, raw-surrogate getText/getExpandedText/setText/terminal stop/start failures and setText+restart failure. Full file/TUI/argv/heartbeat traces and original error fields match actual source. Shared Result API migration/static gates are still in progress; no checkpoint acceptance yet.

## Required branch map

| Frozen branch | Proof status |
|---|---|
| :5448–5452 command lookup/empty warning | command via real settings; empty warning branch code-mapped (actual settings always resolves a default) |
| :5454–5459 expanded text/temp name/write before stop | actual-source/process/callback13-case GREEN; getters' raw errors occur before file/finally |
| :5462–5482 stop/banner/space-split/async spawn/error/close | real process success/nonzero/signal/missing, empty argv, async heartbeat and raw stop-error GREEN |
| :5485–5490 status0 read-back and one-newline removal | raw input/empty/double-newline/read ENOENT/EISDIR/raw setText-error GREEN |
| :5493–5507 unlink/start/forced-render finally | exact terminal/file snapshots and raw setText+restart precedence GREEN; forced-render request code-mapped |

Windows `shell:true` is the existing platform gap. Old helper tests are not common-pin evidence.

## Shared Result carriers

`CallbackError` moved unchanged into `pi-js-compat::callback_error` (`131e7027`); `pi-ai::request::CallbackError` remains a pub-use. That pure move passed workspace/all-targets0/0 before API changes. `EditorComponent::{get_text,get_expanded_text,set_text}` and `Terminal::{start,stop}` now return Results; concrete builtin editor methods remain infallible behind Ok trait adapters. TuiExt, extension lifecycle/editor adapters, render queues/regions, and mechanical consumers retain the original carrier. Main inline try/finally uses Results only; a genuine Rust panic is not caught or repurposed. The existing extension-editor component also stops converting panics to synthetic JS throws, using its fallible host Results instead.

Actual frozen throw probe6/6 (`external-editor-frozen-throw-probe.log`) confirmed unguarded source calls: :5454 getters happen before try; :5488 setter throws through finally; tui.ts:877/:959 terminal start/stop have no guard. Cleanup precedes restart; a restart rejection replaces an earlier setter rejection. Shared renderer width-error ordering has its [own contract](../../specs/tui-width-errors.md).
