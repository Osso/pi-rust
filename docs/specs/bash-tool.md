# Bash tool

Match `coding-agent/src/core/tools/bash.ts` at immutable `4f9e6f8a4`. [Module map](../porting/traceability/bash.md).

## What it must do

- [x] Preserve truthy command-prefix behavior, pass the spawn hook's command/cwd/env to both execution paths, and inherit ordinary PI_* environment values without stripping or injecting legacy session metadata.
- [x] Preserve custom-operation timeout values, initial/stream/final updates, split timeout error mapping, detached status text and truthy optional log paths. Custom exit codes retain the full declared binary64 domain through the tool/executor/session/UI carriers, with JS number status spelling. Current26-case proof is a checkpoint, not whole-module closure.
- [ ] Complete local process, timeout/abort/stdio/durable-output/adoption and runner-owned restore/detach behavior with exact error/callback boundaries and native UTF-16.
- [x] Render elapsed durations with JavaScript `toFixed(1)` and treat empty full-output paths as absent for footer removal and warnings.
- [x] Reuse call/result components across frames; incompatible prior components preserve native TypeError identity, including recovery on subsequent valid frames.
- [ ] Complete stateful renderer cache/timer semantics and every exported symbol/branch.
- [ ] Close focused behavioral proofs, warning-free all-target check before each checkpoint, owned-source lint and whole coverage map. Shared changes require workspace checks; coverage edits require `python3 scripts/source-inventory.py` PASS without new row failures.

## How it works

- [Traceability and retained/retired proof inventory](../porting/traceability/bash.md).

## Tests asserting this spec

- `crates/pi-coding-agent/tests/differential_bash_core.rs`:26 frozen custom-operation histories, exact exec inputs/results/updates, raw strings and split UTF-8, inherited PI_* variables and spawn-hook cwd.
- `crates/pi-coding-agent/tests/differential_bash_core.rs::bash_executor_preserves_custom_binary64_exit_values`:10 direct frozen executor numeric returns, before JSON projection.
- `crates/pi-coding-agent/tests/differential_interactive_components.rs::bash_execution_matches_the_pinned_typescript_source`: seven bounded current-pin consumer frames, including fractional/exponent error labels.
- `crates/pi-coding-agent/tests/bash_tool_detach.rs`: retained real shell spawn-hook behavior.
- `crates/pi-coding-agent/tests/differential_bash_render_sequence.rs`: ten frozen sequential frames, reused identities, collapsed/expanded/narrow/raw-unit output and cross-kind error recovery.
- `crates/pi-coding-agent/tests/differential_bash_render_state.rs`: twelve frozen frames cover half-tie/exponent duration formatting, empty/scalar/raw-UTF16 output paths and truncation footer truthiness.
- Remaining existing local/runner/render suites require current-pin audit before counting as whole-module proof.

## Out of scope

Unrelated lanes, network/deployment, unapproved semantic deviations and new emulation of Node/V8/libuv runtime oddities. Valid binary64 values, arbitrary human text and external process/file JSON are not typed-core exclusions.
