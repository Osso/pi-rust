# coding-agent tools-manager traceability

Frozen `packages/coding-agent/src/utils/tools-manager.ts` at immutable4f9e6f8a4. Rust: `utils/tools_manager.rs`; [contract](../../specs/tools-manager.md).

| Frozen symbol / branch | Rust representation | Proof |
|---|---|---|
| `ToolConfig`, `TOOLS` fd/rg records | `Tool` closed argument domain + private config; fd/fdfind priority, rg name, Termux package | Executable fixtures in `tests/tools_manager.rs`; invalid JS union values excluded |
| `commandExists`: spawnSync --version, piped output, spawn error false/nonzero exit true | Native command with inherited compatibility environment and captured pipes | Actual frozen fd/fdfind/rg probes compare command names/argv/order; nonzero version case; stdout/stderr do not leak |
| `getToolPath`: aliases then null | `get_tool_path(Tool)` returns owned optional command name | Eight source/native cases, including absent binaries and managed-bin precedence regressions |
| `ensureTool`: found, missing silent, missing non-silent | `ensure_tool(Tool, silent)`; exact source console hint only | Frozen messages plus actual native child stdout; no offline/progress/install logging |
| Android package-manager hint | Pure missing-tool formatter with host platform at call boundary | Native fd/ripgrep punctuation/package unit GREEN (`tools-manager-android-unit.log`) |
| find/grep silent lookups; interactive startup non-silent pair | Free functions; no Fetch/config manager construction. Startup HTTP dispatcher unaffected | `tools-manager-caller-regressions-2.log` GREEN7/0; workspace all-target0 errors/0 warnings |

## Removed old-only behavior

Entire managed-bin/config/HOME/offline/HTTP/download/archive/install authority removed, including asset/release helpers and Fetch dependency. Retired download tests replaced rather than retained as alternate contracts. Bwrap has no direct tools-manager call; its existing native find/grep delegates use the same updated consumers.

## Evidence

`/tmp/pi-rust-repin-core-tools/tools-manager-red.log`: three actual frozen mismatches (managed binary incorrectly wins/appears available; wrong offline message). `tools-manager-green-1.log`: eight-case differential GREEN; native console capture and version-argv probe order checked. Final `tools-manager-green-final.log` GREEN:8 exact ordinary cases plus an explicit source/native runtime-gap case. Configured old agent `bin/` files are ignored; child stdout and probe histories are real. Workspace all-target0 errors/0 warnings (`tools-manager-workspace.*`); subsequent console-only lint fix uses the existing `main_runner::console_log` authority with unchanged public API, proven by the final differential. `tools-manager-clippy-own-clean.jsonl/.stderr`: zero own primary diagnostics after the console fix; package-wide clippy still exits101 on parent-owned536 lib/581 lib-test findings. No package-ready claim.

## Explicit runtime gap

Classified under the approved2026-10-23 native-runtime policy: Node spawnSync's implicit default maxBuffer can make an otherwise spawnable --version probe unavailable. Native does not emulate that default cap. `tools-manager-version-buffer-red.log` proves2MB output yields source null/native fd; a direct Node probe reports ENOBUFS/errno-105/SIGTERM. Final behavioral test retains separate assertions for both outcomes and matching real argv probes; no hidden mismatch waiver or generic fallback. Configured RTK capture limits are a different explicit contract and remain enforced.

Approved owned values apply. Invalid JS tool-name union values, including inherited-object lookup keys, are excluded by native `Tool`; no Object.prototype emulation. No download compatibility path retained.
