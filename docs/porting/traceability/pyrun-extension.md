# Pyrun extension — frozen 4f9e6f8a4

Source: `packages/coding-agent/extensions/pyrun/src/`. Native mapping:
`crates/pi-coding-agent-extensions/src/bundled_extensions/pyrun/`.
Registered as `<first-party:pyrun>` through the native extension API.

## detached-bridge

| Export / branch | Proof |
| --- | --- |
| enqueue request, persist message/body/routing/field order | `pyrun_bridge_persisted_request_matches_frozen_bytes`: actual frozen/native SQLite writers, exact stored bytes after correlated random-ID/time normalization |
| parse request; required protocol/command/method/requestId/toolCallId/identity/replyTo | frozen parses native body and native reads frozen parsed result; native durable round trip rejects empty/non-string toolCallId |
| validate sender, running agent, supervisor/owner and process PID/start ticks | real runner-process bridge test validates current ownership and rejects stale process identity |
| enqueue result/null/undefined/error response | three frozen/native exact SQLite response byte cases, including arbitrary JSON with a lone UTF-16 unit |

RED/GREEN fixed `createdAt`'s position in persisted message objects. A legacy
manifest lacking toolCallId still omits it; the bridge parser rejects that request
as frozen does. Wrong-typed/exotic identity shapes and random/SQLite OS-fault shapes
are gaps under the repository parity policy; they are not new semantic approvals.

## detached-output

`PyrunArtifactWriter` now owns runner progress and terminal writes. It writes an
append-only head up to 25 MiB, then retains an ordered bounded tail within 50 MiB.
Settlement reserves terminal bytes and the 256-byte marker budget, drops oldest
whole records, writes exact omitted counters and marker text, then appends the tail
and mandatory terminal data. Terminal data wins even above the cap.

`pyrun_capped_artifacts_match_frozen_bytes_and_preserve_live_offsets` compares exact
SHA256, byte lengths and unchanged live prefixes for small output, 56 MiB progress
and a 51 MiB terminal record. OS write-fault shapes and numeric extremes are gaps
under the repository policy.

## detached-progress

`is_console_progress_update` and `ArtifactProgressAccumulator` preserve sparse
updates, first-stream selection, stream-change flush, the 100-record dense
threshold across reads, final-update fields, first-pending timer, non-console/
terminal/truncation flush, last result and close-without-flush. The observer owns
one accumulator across polls and closes it on every return.

`pyrun_progress_batches_across_reads_and_flushes_like_frozen` compares six actual
frozen/native scenarios, including 60+60 console records, stream/status changes,
truncation, terminal, timed flush and cancellation. Synchronous callback failures
propagate; detached timer callback failures use the shared uncaught authority.
Runtime callback/timer races are a repository-policy gap.

## detached-evaluation

| Surface | Proof |
| --- | --- |
| artifact paths, canonical parameters, manifest/tool-call correlation, collisions | native real-artifact tests and lossless parameter regressions |
| complete-line replay, terminal/error/lost runtime, multiple terminal rejection, truncation skip | replay tests, `repin_replays_capped_artifacts_without_treating_truncation_as_terminal` |
| cancelling/legacy cancelling settlement and missing activation snapshot | three real control-DB settlement tests |
| restored/live `.runner-error`, trimming/empty default, failure marker, child termination | `repin_restored_runner_error_writes_failure_before_lost_runtime`, `repin_observed_runner_error_wakes_and_keeps_the_original_diagnostic` |
| 1 MiB seek/read cursor, fragments, immediate unread-byte drain before liveness | fragmented UTF-16 regression, real live-runner observation |
| watcher activity, one-second fallback, three-second liveness; progress lifecycle | real foreground resume/detach/abort tests plus accumulator differential |
| initial launch, script/manifest writes, compiled current-executable dispatch, foreground completion | harness-free `pyrun_detached_lifecycle`, beginning with no artifact directory |
| fresh detach, real ownership/activation, one terminal result, finalization and runner exit | same harness-free executable; checks completed revision 2 and registry cleanup |
| foreground bridge claim/reply and detached lifetime | real runner bridge/process tests; activated detachment stops handling tool-call abort |

The old untested-initial-launch gap is closed. The test executable uses production
internal-runner dispatch; no alternate launch seam, host provider or user state is
used. Its runner is released only after observation/detachment, not after a guessed
sleep. Activation-write failure injection, watcher/stdio OS-fault shapes and
runtime scheduling races remain gaps under the repository policy; no new native
failure-injection API was added.

## detached-runner

Manifest version 3, canonical checksum/JS number formatting, activation atomic
write/read and invalid-manifest errors are compared bidirectionally with frozen in
`differential_pyrun_detached`. Invocation modes and launch errors retain their
existing focused tests. The compiled entry uses the production internal mode.

Runner settlement races evaluation with the frozen one-second control poll,
loads activation, claims response/status/cancel commands, correlates bridge
requests, writes capped output and finalizes once. Claim/status failures propagate,
empty response errors follow frozen falsiness, and a failed terminal commit uses
the source error tag rather than Rust enum debug text.

Eight real process cases cover exact output/revision, valid/stale bridge ownership,
two outstanding status requests, cancellation adoption, independent worker launch,
`pi` dispatch, runner-error artifact and missing-agent finalization. Test ownership
names the actual child PID; each child has a private process group and a bounded
result wait. Fresh-launch foreground/detached tests also prove process exit.

`.runner-error` omits Node stack text as approved. Windows-only invocation and
unusual filesystem/process faults are repository-policy gaps. The standalone
source entry's missing-path diagnostic is represented by the compiled internal
entry's own frozen diagnostic, as mapped in `internal-runner`.

## runner

| Export / branch | Proof |
| --- | --- |
| command/legacy environment resolution and JSON string-array arguments | native resolution tests; command-only registration does not parse arguments |
| resolved option persistence; unset properties omitted | actual frozen resolver differential |
| spawn, inherited/isolated/current overlay environment, explicit overrides | actual TS/native child environment differential |
| process generation reuse, FIFO JSONL, final result tags and progress | real canonical Pyrun differential; scripted runner tests |
| two outstanding requests and callback failure isolation | actual frozen/native queued-request RED/GREEN; first rejects and second completes |
| stdin/parse/abort rejection and generation exit rejection | request IDs model source object identity; abort/parse/spawn/stderr tests |
| pi_request handled null versus unhandled undefined | existing exact `{\"result\":null}` versus `{}` wire tests |
| detached group / inherited group disposal and abort | real TS/native private parent/grandchild process-group differential; detached runner supplies its own PID |

Node microtask interleaving between independent producers, Windows taskkill,
exotic stdio/proc fault shapes and sorted native environment enumeration are
repository-policy gaps. Native progress snapshots and owned return values follow
the standing approvals.

## index

Whole-module comparison covers renderers, schemas, preparation, all parameter
normalizers, session ID/name/path matching, scoped models/model selection, tool/
command dispatch, compaction, restart, enqueue, custom handlers, executor caching,
sandbox selection, background routing, mailbox handling and shutdown.

`pyrun_registration` proves exact frozen prompt definition, missing/present command
registration, delayed argument parsing, non-persisted foreground execution,
standalone/active-tool guards, custom prompt overrides (including empty values),
custom handler disposal, metadata-only session names in a nondefault context DB,
empty-string rendering and queued/inherited runner behavior. Existing
`pyrun_extension` tests cover dispatcher outputs and validation failures.

Mailbox handling parses and validates ownership before dispatch, forwards the
original toolCallId/signal and persists result/undefined or exact error text.
Real process bridge tests prove the underlying routing and ownership boundary.
`PyrunPiRequestHandler` carries the source callable plus optional dispose property;
shutdown disposes the runner before handlers, with errors propagated.

The `!ctx.switchSession` branch is **frozen-unreachable with the frozen runner**:
`runner.ts:946–949` always installs the wrapper and print/RPC/interactive hosts bind
it. Main confirmed this classification; no Option/availability API or gap was
added. Native context APIs remain unchanged.

Typed APIs exclude adversarial/wrong-typed shapes; native extensions and sandbox
workers retain the standing approved deviations. No old-only prompt guideline or
uncapped/batch-per-read path remains.

## Supporting modules and proof ledger

`eval-tool.ts` and `detached-runner-entry.ts` were not assigned for a new whole-row
audit; their existing native/differential coverage is retained. Callable footer
status failures continue through the shared CallbackError carrier.

| Proof scope | Saved evidence |
| --- | --- |
| 23 core-1 + 5 core-2 + 20 extension Pyrun tests | `/tmp/bundled3-pyrun-integration2.log` |
| real runner, canonical runtime and registration integration | `/tmp/bundled3-pyrun-process-integration2.log` |
| final eight runner cases and fresh foreground/detached launch | `/tmp/bundled3-runner-final.log` |
| exact SQLite bridge request and three responses | `/tmp/bundled3-pyrun-bridge-both.log` |
| latest environment RED/GREEN | `/tmp/bundled3-pyrun-env-{red,green}.log` |
| workspace all-target compile after final production edit | `/tmp/bundled3-workspace-check-last.log` |
| fourteen strict frozen-source fixtures, approved ES2024 noEmit scope | `oracle/bundled-ext-goal-pyrun-typecheck.mjs`; `/tmp/bundled3-oracle-typecheck-final.log` |

The first integration run exposed fixture ordering/ownership failures; it was not
reported as passing. Its hung test groups were cancelled and verified exited.
The corrected fixtures retain every original output/revision/completion assertion.
No full workspace test suite, live provider, push or deployment was run.

## UTF-8 artifact decoding (2026-10-26)

| Common-pin source | Contract | Proof |
|---|---|---|
| `extensions/pyrun/src/detached-evaluation.ts:328` | `readFileSync(path, "utf8")`: invalid artifact bytes replace with U+FFFD; complete following records survive; incomplete tail excluded | `detached_evaluation::utf8_tests::complete_artifact_records_replace_invalid_utf8`; RED `/tmp/utf8-pyrun-red.log`, GREEN `/tmp/utf8-pyrun-green.log` |

## Retained UTF-8 boundary (2026-10-26)

`detached_runner.rs:155,164` activation/checksummed-manifest reads remain strict (parent-approved machine-written corruption exclusion); `runner.rs:647` reads kernel `/proc/.../stat`, not user text. Frozen counterparts: `detached-runner.ts:366,370` and `runner.ts:113`. See the [complete file-read audit](../utf8-decoding-sweep.md).
