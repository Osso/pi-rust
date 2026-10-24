# Node child-process argument strings

Source: Node validation inherited by frozen `utils/child-process.ts:24–35`, `harness/env/nodejs.ts` and the interactive external-editor spawn. Common pin: `4f9e6f8a4`.

Authority: `pi-js-compat::node_child_process`; invalid-value construction uses `NativeNodeError::invalid_argument_value` and the existing `node_fs::invalid_argument_value_message` inspection/truncation authority. Rejections are boxed native errors; adapters move/coerce that original box or move its original value, without a second error reconstruction or double box.

| Observable feature | Consumer / proof |
|---|---|
| Empty file rejects before argv | `validate_spawn_arguments`; `pi-js-compat/tests/node_child_process.rs` actual Node differential |
| NUL file and first NUL argv reject with exact index | Same differential; external editor delegates instead of maintaining a second loop |
| Unmatched units remain valid without NUL; exec failures are not argument failures | Valid/missing executable controls in the differential |
| Inspected value truncates in UTF-16 | Long surrogate-containing argv differential |
| TypeError constructor, code and coded `String(error)` retained | Differential compares constructor, own fields excluding stack, and string value |
| Harness command/cwd/env order and labels | Existing harness orchestration retained; local function only converts the shared rejection to ExecutionError |

Invalid JavaScript runtime input types are excluded by the typed Rust API (AGENTS.md). Windows process branches remain the existing approved gap. Shared actual-Node differential passed (six controls); workspace all-target check passed with zero diagnostics at fa6f20bf. Config integration calls the shared validator before encoding/spawn, driven by four actual frozen-module RED cases in `differential_config_spawn_validation` (empty file, NUL file/argv and UTF-16 inspection clipping).

## uv_spawn failure delivery

`spawn_error_is_async` additionally maps Node v26.11.1 `lib/internal/child_process.js::ChildProcess.prototype.spawn`'s exact five-error set (EACCES/EAGAIN/EMFILE/ENFILE/ENOENT) to asynchronous error delivery, everything else synchronous. Proven against the installed Node builtin source and twelve concrete native error inputs in `tests/node_spawn_error_boundary.rs`; [bash artifact cleanup](bash.md#local-artifact-cleanup-and-spawn-error-boundary) supplies actual frozen EACCES-vs-E2BIG process/filesystem proof. No fallback behavior or new dependency. The shared classification does not change existing spawn callers until explicitly routed.

[Shell transport contract](../../specs/node-shell-transport.md). [Config error contract](../../specs/config-path-errors.md).
