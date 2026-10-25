# Bash tool

Match `coding-agent/src/core/tools/bash.ts` at immutable `4f9e6f8a4`. [Module map](../porting/traceability/bash.md).

## What it must do

- [x] Preserve truthy command-prefix behavior, pass the spawn hook's command/cwd/env to both execution paths, and inherit ordinary PI_* environment values without stripping or injecting legacy session metadata.
- [x] Preserve custom-operation timeout values, initial/stream/final updates, split timeout error mapping, detached status text and truthy optional log paths. Custom exit codes retain the full declared binary64 domain through the tool/executor/session/UI carriers, with JS number status spelling. Current26-case proof is a checkpoint, not whole-module closure.
- [x] Preserve absent custom-executor environments; local operations resolve shell defaults only when env is absent. Tool-hook environments remain explicitly supplied. Executor streaming strips the initial UTF-8 BOM (including split prefixes), preserves later BOMs, and never flushes incomplete trailing bytes, matching frozen TextDecoder calls. The accumulator and executor use one shared decoder; per-read Buffer text decoding remains distinct.
- [x] Present environment maps retain declared string-or-undefined values for hooks/custom operations. Local spawn and detached JSON omit undefined entries; absent env, empty env and undefined properties remain distinct before those boundaries.
- [x] Local shell output uses the shared rearmed post-exit idle-grace wait, not fixed-duration reader joins. Detached output remains drained without callbacks; durable files poll every 1000ms, so short foreground artifact-backed commands can return no streamed output (frozen behavior). Open/spawn/argv diagnostics use shared Node authorities.
- [x] Source-optional adopt callback receives native child PID, original command/cwd, shared exit completion, detach timestamp and cancellable timeout ownership. Foreground exit remains in the race while adoption awaits it; rejection cleans foreground artifacts. Preserve frozen's omitted artifacts field in the adoption payload as a source discrepancy.
- [x] Streaming pipe read errors and durable timer read failures reach the shared uncaught authority with original Node name/message/code/errno/syscall/path. A detach poll invoked inside pre-aborted Promise setup rejects; a later detach-event poll throw remains uncaught. Shared bounded-capture error behavior is unchanged.
- [x] A static background lifecycle needs no resolver callback. Restored terminal jobs stream an existing labeled output file or propagate its read error; timeout status spells absent timeout as `undefined`, matching interpolation. Replay occurs before launch timeout validation.
- [x] Runner launch/manifest timeout milliseconds preserve finite binary64 fractions and existing TS integer bytes/checksums. Runtime adds signed milliseconds to the shared JS wall clock; negative manifest values expire immediately, never abs() into a future delay. Null/malformed manifest-schema semantics remain under whole runner ownership.
- [x] Initial onUpdate runs before registering the detach handle; its synchronous registry observation cannot detach a call that has not yet registered.
- [x] Foreground completion files are external JSON ingress, not typed custom-operation input: preserve raw exit-code values, nullish defaults and JS truthiness. Read/JSON failures retain Node messages; file text uses Node UTF-8 repair and JS trim.
- [x] Runner detach activates synchronously before reporting success and protects the detached runner from immediate foreground abort. Retain frozen reentrant output duplication when onUpdate detaches before the outer forward call stores its offset (DISCREPANCY). Update throttle records its timestamp before invoking callbacks.
- [x] Foreground runner cancellation uses the same POSIX SIGKILL process-tree authority as frozen bash, not the detached runner's separate TERM helper. TERM-resistant groups cannot survive cancellation.
- [x] Durable output descriptors close immediately around spawn preparation, before child observation/tracking. A close failure rejects and overrides a simultaneous spawn-validation failure, preserving source finally semantics.
- [x] Stdin-transport writes remain asynchronous under backpressure; timeout/abort kills the process group before later output. Source-explicit stdin pipe-error suppression remains unchanged.
- [x] Local foreground artifact cleanup follows frozen recursive-force removal for files/directories and ignores absent/non-directory-parent paths; invalid paths reject instead of silently succeeding. Cleanup rejection takes precedence over timeout/spawn errors. Async spawn errors enter cleanup; synchronous spawn/argument failures do not.
- [x] Spawn-hook and executor onChunk failures reject at their synchronous boundaries with exact UTF-16 messages; a callback-triggered abort follows executor's cancelled-result branch instead. Existing hooks remain optional.
- [x] Synchronous custom-operation onData → onUpdate failures reject execution, preserving exact UTF-16 error messages; they do not become uncaught process errors. Actual stream/timer event callback failures remain uncaught.
- [x] Executor callbacks run outside collector locks, including synchronous onData reentry. Spill files retain complete sanitized bytes through rolling-buffer truncation, byte/line thresholds and ordinary-operation errors.
- [x] Open/write/close failures and writes after stream end are native unhandled stream events after the originating callback, never silent drops, non-native panics or ordinary ToolError substitutes. Close streams on success, cancellation and ordinary rejection.
- [x] Complete local process, timeout/abort/stdio/durable-output/adoption and runner-owned restore/detach behavior with exact error/callback boundaries and native UTF-16.
- [x] Render elapsed durations with JavaScript `toFixed(1)` and treat empty full-output paths as absent for footer removal and warnings.
- [x] Reuse call/result components across frames; incompatible prior components preserve native TypeError identity, including recovery on subsequent valid frames.
- [x] Partial result rendering starts one per-state one-second invalidation interval on the existing owner executor; final/error results cancel it. Restarted intervals have distinct cancellation lifetimes, so queued stale ticks cannot invalidate. Generic UI elapsed text is unchanged; there was no driver-poll timer to remove.
- [x] Current-pin public API/branch map covers renderer cache/state and all exports, using the project's explicit policy boundaries.
- [x] Close focused behavioral proofs, warning-free all-target check before each checkpoint, owned-source lint and whole coverage map. Shared changes require workspace checks; coverage edits require `python3 scripts/source-inventory.py` PASS without new row failures.

## How it works

- [Traceability and retained/retired proof inventory](../porting/traceability/bash.md).

## Implementation inventory

- `crates/pi-coding-agent-tools/src/core/tools/bash.rs`: definition/execution, local/runner adapters and renderers.
- `crates/pi-coding-agent-tools/src/core/bash_executor.rs`: sanitized streaming, spill/error/cancellation result.
- [Output accumulator](../porting/traceability/output-accumulator.md): default accumulator behavior consumed by bash; configurable API is separately scoped.
- [Child process](../porting/traceability/child-process.md): shared capture, callbacks, error and deferred spawn boundaries.
- [Atomic writer](bash-atomic-writer.md): cleared detached activation/manifest persistence prerequisite.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/differential_bash_environment.rs`: actual frozen undefined own-key/custom-exec value and default/empty/undefined real-child environments; real runner22 histories include the detached env boundary.
- `crates/pi-coding-agent/tests/differential_bash_core.rs`:26 frozen custom-operation histories, exact exec inputs/results/updates, raw strings and split UTF-8, inherited PI_* variables and spawn-hook cwd.
- `crates/pi-coding-agent/tests/differential_bash_core.rs::bash_executor_preserves_custom_binary64_exit_values`:10 direct frozen executor numeric returns, before JSON projection.
- `crates/pi-coding-agent/tests/differential_interactive_components.rs::bash_execution_matches_the_pinned_typescript_source`: seven bounded current-pin consumer frames, including fractional/exponent error labels.
- `crates/pi-coding-agent/tests/bash_tool_detach.rs`: retained real shell spawn-hook behavior.
- `crates/pi-coding-agent/tests/differential_bash_executor_stream.rs`: ten frozen executor histories cover callback chunks, BOM/invalid/incomplete UTF-8, ANSI/CR/binary sanitation, cancellation/error paths and optional environment absence.
- `cargo test -j4 -p pi-coding-agent --test bash-runner-lifecycle`: test-only harness=false target runs real native/frozen tool, lifecycle, coordinator and runner processes; twenty-two histories cover normal/nonzero/timeout/abort, raw completion ingress and filesystem failures, synchronous detach, completed-call refusal and immediate post-detach abort, including callback frames.
- `crates/pi-coding-agent/tests/differential_bash_descriptor.rs`: real descriptor-close EBADF after successful spawn and simultaneous invalid argv, including artifact retention.
- `crates/pi-coding-agent/tests/differential_bash_stdin_lifetime.rs`: oversized stdin with a non-reading real shell proves timeout/abort occur before delayed output; successful transport retained in `bash_stdin.rs`.
- `crates/pi-coding-agent/tests/differential_bash_hook_chunk_errors.rs`: actual frozen hook-before-initial-update ordering, chunk failure propagation, raw D800 errors and callback-triggered cancellation.
- `crates/pi-coding-agent/tests/differential_bash_callback_errors.rs`: actual frozen/custom operation and isolated native process compare ordinary callback rejection vs uncaught crash, exact raw-surrogate errors and before/after data trace.
- `crates/pi-coding-agent/tests/differential_bash_executor_spill.rs`: nineteen frozen histories compare complete file/output/callback hashes, permissions, random-name shape, Node temp-directory priority, reentry, real syscall failures and late writes after normal/error completion.
- `core::tools::bash::process_tests::foreground_runner_cancellation_kills_a_term_resistant_process_group`: real TERM-resistant group, frozen tool cancellation signal/exit/marker versus native private helper; isolates the signal policy from the real runner's parent-death gate.
- `crates/pi-coding-agent/tests/differential_bash_manifest_numbers.rs` and `detached_bash_runner::negative_manifest_timeout_expires_before_the_payload_can_complete`: frozen manifest bytes/checksums for six finite cases and real signed-deadline expiry.
- `crates/pi-coding-agent/tests/differential_bash_restored.rs`: nine histories cover persisted terminal states, absent/fractional timeout, output read failure, stdin rejection before restore, lost_runtime resuming launch validation, and initial-callback registry ordering.
- `crates/pi-coding-agent/tests/differential_bash_uncaught.rs`: isolated real stdout/stderr EIO and missing durable-file polling compare the frozen process-hook error record and native process exit.
- `crates/pi-coding-agent/tests/differential_bash_adopt.rs`: frozen native payload, sync-ready/async-rejection, exit/adoption race, artifact cleanup and timeout cancellation transfer.
- `crates/pi-coding-agent/tests/differential_bash_local.rs`: twenty-three real-process frozen histories, including late descendant tail, fast/slow durable files, open/NUL/spawn diagnostics and timeout/abort ordering.
- `crates/pi-coding-agent/tests/differential_bash_render_timer.rs`: frozen timer start/idempotence/stop/restart/error/cancel ordering, with native ticks asserted on the owner thread.
- `crates/pi-coding-agent/tests/differential_bash_render_sequence.rs`: seventeen frozen sequential frames, reused identities, collapsed/expanded/narrow/raw-unit output and cross-kind error recovery.
- `crates/pi-coding-agent/tests/differential_bash_render_state.rs`: twelve frozen frames cover half-tie/exponent duration formatting, empty/scalar/raw-UTF16 output paths and truncation footer truthiness.
Historical suites not linked in the current matrix are not promoted into acceptance evidence.

## Known gaps (current cycle)

- [ ] [Shared child envp ordering](../porting/traceability/child-process.md#open-shared-spawn-item): parent deferred replacement of std Command's sorted environment to final-gate/user decision; unchanged failing regression is explicitly ignored. Bash closure is permitted with this cross-reference, not a claim the shared item passes.
- [ ] Coding-agent package-global lint debt remains parent-owned; current scoped source is clean, not the whole package.

## Out of scope

Unrelated lanes, network/deployment, unapproved semantic deviations and new emulation of Node/V8/libuv runtime oddities. Valid binary64 values, arbitrary human text and external process/file JSON are not typed-core exclusions.
