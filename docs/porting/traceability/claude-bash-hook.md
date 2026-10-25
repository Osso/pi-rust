# `claude-bash-hook` extension traceability

Source: `packages/coding-agent/extensions/claude-bash-hook/src/index.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`. Rust:
`crates/pi-coding-agent-extensions/src/bundled_extensions/claude_bash_hook/mod.rs`.

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
- Adversarial values and OS/runtime faults (including early-close EPIPE) follow the approved user-observable gap policy. Runtime scheduling and inherited-descriptor edge cases are not reproduced.
- `stderr` is captured but unused, matching the source.

## Re-pin whole-module audit (2026-10-25)

All exported functions and helpers in the table were compared to the frozen source. Registration now resolves and checks executable availability first; missing and non-executable commands register no reviewer. Timeout covers stdin and output, sends SIGTERM, rejects with exact Node `The operation was aborted`, and retains a child reaper until close instead of dropping it into SIGKILL. Input uses lossless JS stringify and inherited environment overrides; spawn failures use the shared Node spawn-message authority. `unavailable_hook_does_not_register_a_reviewer` and `hook_timeout_reports_node_abort_and_sends_sigterm` both failed before implementation and pass after it. The 12-case frozen process differential also passes. Logs: [lane ledger](../bundled-ext-b-proof.md).

## Known test flake

`tests/claude_bash_hook_extension.rs` writes an executable script and spawns it
from several parallel test threads; a concurrent fork can inherit the write fd
and the spawn fails with `Text file busy (os error 26)`. Reproduced 1 in 3 serial
runs of that binary; owned by the `ca-errno` spawn-race slice.

## Baseline pipe lifecycle (2026-10-25)

The hook test scripts printed and exited without reading their JSON stdin.
A 147456-byte evaluation deterministically reproduces EPIPE in the frozen
Node oracle itself (unhandled stdin error), before any result. Fixture hooks
now drain stdin to EOF before responding, on both sides; capture fixtures
already did this. The differential retains that large request and exact result
assertions. Runtime hook code is unchanged. Early-close/EPIPE is an OS/runtime
fault gap, not a new recovery policy. Readability: one explicit `cat` per script.
