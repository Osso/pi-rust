# Inline external editor

Frozen source: `coding-agent/src/modes/interactive/interactive-mode.ts:5447–5507`. No frozen standalone `external-editor.ts` exists. [Contract](../../specs/external-editor.md).

## Re-port in progress

The old native `external_editor.rs` module, `ExternalEditorOptions`/`ExternalEditorResult`, private directory/`prompt.md` lifecycle and corresponding old-only test expectations are being deleted, not retained as a parity gap (parent decision2026-10-24).

Before production changes, `tmpdir_external_editor_matches_frozen_inline_flow` runs an isolated native process and the actual frozen method with a real TUI and fake editor process. Genuine RED `tmpdir-external-red.log`: native path parent is `tmpdir/pi-editor-<random>`, frozen parent is `tmpdir` and frozen filename is `pi-editor-0.pi.md`. The fixture also records TUI stop/start file snapshots, initial UTF-8 bytes, argv including empty arguments, read-back and cleanup, and a heartbeat proving event-loop progress while the editor runs. Full directory snapshots remain asserted; source-first execution stabilizes incidental tsx cache creation. Native timestamp is validated against before/after clock bounds before only its nonce is normalized.

Logs and exact revision/scope ledger: `/tmp/pi-rust-repin-lossless-interactive/tmpdir-external-*`. This RED is not GREEN/parity evidence. Shared `node_os::tmpdir` is owned by231; consumers wait for its landing.

## Required branch map

| Frozen branch | Proof status |
|---|---|
| :5448–5452 command lookup/empty warning | pending |
| :5454–5459 expanded text/temp name/write before stop | actual-source/process RED |
| :5462–5482 stop/banner/space-split/async spawn/error/close | success/heartbeat fixture RED; other outcomes pending |
| :5485–5490 status0 read-back and one-newline removal | raw-input fixture RED; failure matrix pending |
| :5493–5507 unlink/start/forced-render finally | terminal/file snapshots RED; error precedence pending |

Windows `shell:true` is the existing platform gap. Old helper tests are not common-pin evidence.
