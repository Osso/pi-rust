# Inline external editor

Frozen source: `coding-agent/src/modes/interactive/interactive-mode.ts:5447–5507`. No frozen standalone `external-editor.ts` exists. [Contract](../../specs/external-editor.md).

## Literal flow re-port

The old native `external_editor.rs` module, `ExternalEditorOptions`/`ExternalEditorResult`, private directory/`prompt.md` lifecycle and corresponding old-only test expectations are deleted, not retained as a parity gap (parent decision2026-10-24).

Before production changes, `tmpdir_external_editor_matches_frozen_inline_flow` runs an isolated native process and the actual frozen method with a real TUI and fake editor process. Genuine RED `tmpdir-external-red.log`: native path parent is `tmpdir/pi-editor-<random>`, frozen parent is `tmpdir` and frozen filename is `pi-editor-0.pi.md`. The fixture also records TUI stop/start file snapshots, initial UTF-8 bytes, argv including empty arguments, read-back and cleanup, and a heartbeat proving event-loop progress while the editor runs. Full directory snapshots remain asserted; source-first execution stabilizes incidental tsx cache creation. Native timestamp is validated against before/after clock bounds before only its nonce is normalized.

Logs and exact revision/scope ledger: `/tmp/pi-rust-repin-lossless-interactive/tmpdir-external-*`. Shared `node_os::tmpdir` from parent `a01ef666` supplies the inline path. Final real-process/callback GREEN at `ec373860`: `external-editor-final-matrix.log` (2 tests,14 isolated cases,74.52 seconds). Cases: successful edit, nonzero exit, signal, empty content, missing executable, synchronous spawn ENOTDIR, read ENOENT, read EISDIR, raw-surrogate getText/getExpandedText/setText/terminal stop/start failures and setText+restart failure. Full file/TUI/argv/heartbeat traces and original error fields match actual source. [Checkpoint gates](#result-api-checkpoint).

## Required branch map

| Frozen branch | Proof status |
|---|---|
| :5448–5452 command lookup/empty warning | command via real settings; empty warning branch code-mapped (actual settings always resolves a default) |
| :5454–5459 expanded text/temp name/write before stop | actual-source/process/callback13-case GREEN; getters' raw errors occur before file/finally |
| :5462–5482 stop/banner/space-split/async spawn/error/close | real process success/nonzero/signal/missing/synchronous ENOTDIR, empty argv, async heartbeat and raw stop-error GREEN; ENOTDIR had a genuine null-versus-original-error RED (`external-editor-sync-spawn-red.log`), fixed via the shared classifier |
| :5485–5490 status0 read-back and one-newline removal | raw input/empty/double-newline/read ENOENT/EISDIR/raw setText-error GREEN |
| :5493–5507 unlink/start/forced-render finally | exact terminal/file snapshots and raw setText+restart precedence GREEN; forced-render request code-mapped |

Windows `shell:true` is the existing platform gap. Old-only private-directory helper tests were removed, not rewritten as parity evidence.

## Shared Result carriers

`CallbackError` moved unchanged into `pi-js-compat::callback_error` (`131e7027`); `pi-ai::request::CallbackError` remains a pub-use. That pure move passed workspace/all-targets0/0 before API changes. `EditorComponent::{get_text,get_expanded_text,set_text}` and `Terminal::{start,stop}` now return Results; concrete builtin editor methods remain infallible behind Ok trait adapters. TuiExt, extension lifecycle/editor adapters, render queues/regions, and mechanical consumers retain the original carrier. Main inline try/finally uses Results only; a genuine Rust panic is not caught or repurposed. The existing extension-editor component also stops converting panics to synthetic JS throws, using its fallible host Results instead.

Actual frozen throw probe6/6 (`external-editor-frozen-throw-probe.log`) confirmed unguarded source calls: :5454 getters happen before try; :5488 setter throws through finally; tui.ts:877/:959 terminal start/stop have no guard. Cleanup precedes restart; a restart rejection replaces an earlier setter rejection. Shared renderer width-error ordering has its [own contract](../../specs/tui-width-errors.md).

## Result API checkpoint

Native revision `ec373860`, parent `3175f4f8` merged. Main inline editor2/2,14 isolated actual-source/process/callback cases; TUI87/87; synchronous lifecycle/forced-repaint1/1;31 distinct coding-agent consumer tests GREEN (config/custom-entry6 from the first batch,25 from the completed second batch). Declared private child-entry ignores remain unchanged; their parent probes execute the required child paths. No full-suite/whole-mode acceptance inferred.

Workspace/all-targets:0 errors/0 warnings (`editor-result-workspace-checkpoint.*`). Package all-target clippy for pi-tui/pi-coding-agent/pi-js-compat:0 errors/0 owned findings,1697 other warnings explicit (`editor-result-clippy-final.*`, `editor-result-owned-clippy-final.json`). Strict pinned tsgo includes actual frozen interactive/tui modules and both new oracles; Biome2/no fixes; fmt check PASS. Inventory PASS686/186679,403 done/19 gap/87 partial/177 todo. Logs and exact command/revision/invalidation ledger: `/tmp/pi-rust-repin-lossless-interactive/editor-result-proof-ledger.md`.

Changed-line Rust/TS readability audited manually: typed error propagation, explicit Result try/finally, existing shared spawn-classification authority, no fallback/suppressions/in-chain panic transport. Old-only helper/API/tests deleted. No pushes/deployment/agents/live providers/user editor; remaining clipboard/share tmpdir consumers and queues6/7 are outside this checkpoint.
