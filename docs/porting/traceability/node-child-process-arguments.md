# Node child-process argument strings

Source: Node validation inherited by frozen `utils/child-process.ts:24–35`, `harness/env/nodejs.ts` and the interactive external-editor spawn. Common pin: `4f9e6f8a4`.

Authority: `pi-js-compat::node_child_process`; invalid-value construction uses `NativeNodeError::invalid_argument_value` and the existing `node_fs::invalid_argument_value_message` inspection/truncation authority.

| Observable feature | Consumer / proof |
|---|---|
| Empty file rejects before argv | `validate_spawn_arguments`; `pi-js-compat/tests/node_child_process.rs` actual Node differential |
| NUL file and first NUL argv reject with exact index | Same differential; external editor delegates instead of maintaining a second loop |
| Unmatched units remain valid without NUL; exec failures are not argument failures | Valid/missing executable controls in the differential |
| Inspected value truncates in UTF-16 | Long surrogate-containing argv differential |
| TypeError constructor, code and coded `String(error)` retained | Differential compares constructor, own fields excluding stack, and string value |
| Harness command/cwd/env order and labels | Existing harness orchestration retained; local function only converts the shared rejection to ExecutionError |

Invalid JavaScript runtime input types are excluded by the typed Rust API (AGENTS.md). Windows process branches remain the existing approved gap. Shared tests and workspace check are pending at the consolidation checkpoint; config integration is a separate follow-up, driven by `differential_config_spawn_validation` RED.

[Shell transport contract](../../specs/node-shell-transport.md). [Config error contract](../../specs/config-path-errors.md).
