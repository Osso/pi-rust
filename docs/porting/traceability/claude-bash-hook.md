# `claude-bash-hook` extension traceability

Source: `packages/coding-agent/extensions/claude-bash-hook/src/index.ts`. Rust:
`crates/pi-coding-agent/src/bundled_extensions/claude_bash_hook/mod.rs`.

| TypeScript symbol / branch | Rust item | Proof |
|---|---|---|
| `default` factory registering an approval reviewer | `claude_bash_hook_extension`, `register` | `claude_bash_hook_extension.rs::registers_an_approval_reviewer` |
| `reviewToolWithClaudeBashHook` | `review_tool_with_claude_bash_hook` | differential; behavioral tests |
| `toClaudeBashHookToolName` (`bash` → `Bash`, `pyrun_eval`) | `to_claude_bash_hook_tool_name` | differential `non-reviewable-tool`; behavioral |
| `hasReviewableInput` (non-empty command/code) | `has_reviewable_input` | differential `empty-command`, `missing-code` |
| `readClaudeBashHookCommand` (`PI_CLAUDE_BASH_HOOK`, default path) | `read_claude_bash_hook_command` | behavioral `missing_hook_binary_is_unavailable`; options command seam |
| Hook input object (`access_mode`, `approval_policy`, `cwd`, `supports_updated_input`, `tool_input`, `tool_name`) | `hook_input` in `review_tool_with_claude_bash_hook` | behavioral `reviews_pyrun_eval_with_the_hook`, `maps_updated_input_and_bash_tool_names` |
| `runHookProcess` (`--harness codex`, stdin JSON, timeout, ENOENT → empty result) | `run_hook_process` | differential; behavioral missing-binary test |
| `parseHookSpecificOutput` / `parseJsonObject` | `parse_hook_specific_output` | differential `malformed`, `empty-output` |
| `mapHookOutput` (`allow`/`deny`/`block`/`ask`/unknown) | `map_hook_output` | differential `allow`, `deny`, `block-default`, `ask`, `unknown-decision` |
| `formatHookReason` (trimmed string or default) | `format_hook_reason` | differential `block-default` |
| Reviewer mapping to `allow`/`ask`/`deny`/`undefined` | `to_reviewer_result` | behavioral reviewer test |

## Q18 native input object boundary

Actual sandboxed Cargo RED at `4fe83515`: `hook_process_preserves_native_input_keys_and_output_reason` panics constructing the payload (`key must be a string`) before spawning the fixture process. The cloned `tool_input` must remain a `JsValue::Object`, not a bare map passed through serde's scalar key serializer. This single-authority correction preserves native CWD, command and distinct lone-unit keys through stdin JSON; the fixture process returns a lone-unit reason. Production correction `29bce163`: full behavioral Cargo GREEN8, zero warnings, including raw real-process regression (`hook-native-incrate-green.log`). Frozen differential's result adapter also requires wrapping its returned native updated-input map; that fixture correction has its own focused rerun.

## Discrepancies / gaps

- `ClaudeBashHookOptions::{command, timeout_ms}` are test seams. Production
  callers use `ClaudeBashHookOptions::default()`, which follows the source
  `PI_CLAUDE_BASH_HOOK`/`DEFAULT_HOOK_PATH` lookup and the 30s timeout.
- On timeout the child is killed by `kill_on_drop` and the reviewer fails with
  `claude-bash-hook timed out`; TypeScript rejects with the abort reason from
  `AbortController`. No source test covers this path.
- `stderr` is captured but unused, matching the source.

## Known test flake

`tests/claude_bash_hook_extension.rs` writes an executable script and spawns it
from several parallel test threads; a concurrent fork can inherit the write fd
and the spawn fails with `Text file busy (os error 26)`. Reproduced 1 in 3 serial
runs of that binary; owned by the `ca-errno` spawn-race slice.
