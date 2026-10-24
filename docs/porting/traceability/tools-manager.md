# coding-agent tools-manager traceability

Frozen `packages/coding-agent/src/utils/tools-manager.ts` at immutable4f9e6f8a4. Rust: `utils/tools_manager.rs`; [contract](../../specs/tools-manager.md).

| Frozen symbol / branch | Rust representation | Proof |
|---|---|---|
| `ToolConfig`, `TOOLS` fd/rg records | `Tool` closed argument domain + private config; fd/fdfind priority, rg name, Termux package | Executable fixtures in `tests/tools_manager.rs`; invalid JS union values excluded |
| `commandExists`: spawnSync --version, piped output, spawn error false/nonzero exit true | Native command with inherited compatibility environment and captured pipes | Actual frozen fd/fdfind/rg probes compare command names/argv/order; nonzero version case; stdout/stderr do not leak |
| `getToolPath`: aliases then null | `get_tool_path(Tool)` returns owned optional command name | Eight source/native cases, including absent binaries and managed-bin precedence regressions |
| `ensureTool`: found, missing silent, missing non-silent | `ensure_tool(Tool, silent)`; exact source console hint only | Frozen messages plus actual native child stdout; no offline/progress/install logging |
| Android package-manager hint | Pure missing-tool formatter with host platform at call boundary | Native fd/ripgrep punctuation/package unit test; final gate pending |
| find/grep silent lookups; interactive startup non-silent pair | Free functions; no Fetch/config manager construction. Startup HTTP dispatcher unaffected | Mechanical callers updated; final caller checks pending |

## Removed old-only behavior

Entire managed-bin/config/HOME/offline/HTTP/download/archive/install authority removed, including asset/release helpers and Fetch dependency. Retired download tests replaced rather than retained as alternate contracts. Bwrap has no direct tools-manager call; its existing native find/grep delegates use the same updated consumers.

## Evidence

`/tmp/pi-rust-repin-core-tools/tools-manager-red.log`: three actual frozen mismatches (managed binary incorrectly wins/appears available; wrong offline message). `tools-manager-green-1.log`: eight-case differential GREEN; native console capture and version-argv probe order checked. Fixtures subsequently locate the old managed files under a configured agent `bin/` to prove production lookup ignores that state; final recheck pending.

Approved owned values apply. Invalid JS tool-name union values, including inherited-object lookup keys, are excluded by native `Tool`; no Object.prototype emulation. No download compatibility path retained.
